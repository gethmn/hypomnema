mod schema;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, SecondsFormat, Utc};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use tokio::task;
use tracing::info;
use uuid::Uuid;

pub use schema::SCHEMA_VERSION;

pub type VaultRegistryPool = Pool<SqliteConnectionManager>;

const POOL_MAX_SIZE: u32 = 4;
const VAULTS_DB_FILE: &str = "vaults.sqlite";

/// Per-vault subdirectory under the daemon's `storage.data_dir`. Owns the
/// `<data_dir>/vaults/<vault_id>/` convention from step-9 workplan § Goal
/// recap. Used by `Store::open` and (in later step-9 tasks) by the per-vault
/// outbox writer + the daemon's reconcile loop, so the layout has one source
/// of truth.
pub fn vault_data_dir(data_dir: &Path, vault_id: &VaultId) -> PathBuf {
    data_dir.join("vaults").join(vault_id.as_str())
}

/// Surrogate identifier for a vault. Storage form is the canonical UUIDv7
/// hyphen-separated string (per Resolution A in the step-9 workplan); the
/// `vault_<uuid>` user-facing prefix is a step-10 display-only concern.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VaultId(String);

impl Default for VaultId {
    fn default() -> Self {
        Self::new()
    }
}

impl VaultId {
    /// Mint a fresh time-ordered ID. Two consecutive calls produce IDs whose
    /// lexicographic order matches creation order (see the
    /// `vault_id_new_returns_time_ordered_uuids` test).
    pub fn new() -> Self {
        Self(Uuid::now_v7().to_string())
    }

    /// Wrap an existing string. Used when reading a row from `vaults.sqlite`;
    /// not validated here because the schema's PRIMARY KEY + UNIQUE checks
    /// already constrain the contents.
    pub fn from_string(s: String) -> Self {
        Self(s)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for VaultId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VaultStatus {
    Active,
    Paused,
    Errored,
}

impl VaultStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            VaultStatus::Active => "active",
            VaultStatus::Paused => "paused",
            VaultStatus::Errored => "errored",
        }
    }

    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "active" => Ok(VaultStatus::Active),
            "paused" => Ok(VaultStatus::Paused),
            "errored" => Ok(VaultStatus::Errored),
            other => Err(anyhow!("invalid vault status {other:?}")),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultRow {
    pub id: VaultId,
    pub name: String,
    pub path: PathBuf,
    pub status: VaultStatus,
    pub created_at: DateTime<Utc>,
    pub last_error: Option<String>,
}

#[derive(Debug)]
pub struct VaultRegistry {
    pool: VaultRegistryPool,
    path: PathBuf,
}

impl VaultRegistry {
    /// Open (or create) `<data_dir>/vaults.sqlite`. Creates the schema on first
    /// open; verifies `schema_version=1` on subsequent opens.
    pub async fn open(data_dir: &Path) -> Result<Self> {
        let data_dir = data_dir.to_path_buf();
        task::spawn_blocking(move || open_blocking(data_dir))
            .await
            .context("spawn_blocking join error in VaultRegistry::open")?
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn pool(&self) -> VaultRegistryPool {
        self.pool.clone()
    }

    pub async fn list(&self) -> Result<Vec<VaultRow>> {
        let pool = self.pool.clone();
        task::spawn_blocking(move || {
            let conn = pool.get().context("acquiring vault_registry connection")?;
            list_blocking(&conn, None)
        })
        .await
        .context("spawn_blocking join error in VaultRegistry::list")?
    }

    pub async fn list_active(&self) -> Result<Vec<VaultRow>> {
        let pool = self.pool.clone();
        task::spawn_blocking(move || {
            let conn = pool.get().context("acquiring vault_registry connection")?;
            list_blocking(&conn, Some(VaultStatus::Active))
        })
        .await
        .context("spawn_blocking join error in VaultRegistry::list_active")?
    }

    pub async fn get_by_id(&self, id: &VaultId) -> Result<Option<VaultRow>> {
        let pool = self.pool.clone();
        let id = id.clone();
        task::spawn_blocking(move || {
            let conn = pool.get().context("acquiring vault_registry connection")?;
            get_one_blocking(
                &conn,
                "SELECT id, name, path, status, created_at, last_error \
                 FROM vaults WHERE id = ?1",
                params![id.as_str()],
            )
        })
        .await
        .context("spawn_blocking join error in VaultRegistry::get_by_id")?
    }

    pub async fn get_by_name(&self, name: &str) -> Result<Option<VaultRow>> {
        let pool = self.pool.clone();
        let name = name.to_string();
        task::spawn_blocking(move || {
            let conn = pool.get().context("acquiring vault_registry connection")?;
            get_one_blocking(
                &conn,
                "SELECT id, name, path, status, created_at, last_error \
                 FROM vaults WHERE name = ?1",
                params![name],
            )
        })
        .await
        .context("spawn_blocking join error in VaultRegistry::get_by_name")?
    }

    pub async fn insert(&self, row: VaultRow) -> Result<()> {
        let pool = self.pool.clone();
        task::spawn_blocking(move || {
            let conn = pool.get().context("acquiring vault_registry connection")?;
            insert_blocking(&conn, &row)
        })
        .await
        .context("spawn_blocking join error in VaultRegistry::insert")?
    }

    pub async fn update_status(
        &self,
        id: &VaultId,
        status: VaultStatus,
        last_error: Option<&str>,
    ) -> Result<()> {
        let pool = self.pool.clone();
        let id = id.clone();
        let last_error = last_error.map(|s| s.to_string());
        task::spawn_blocking(move || {
            let conn = pool.get().context("acquiring vault_registry connection")?;
            update_status_blocking(&conn, &id, status, last_error.as_deref())
        })
        .await
        .context("spawn_blocking join error in VaultRegistry::update_status")?
    }

    /// Delete the row for `id`. Returns `Ok(true)` if a row was removed and
    /// `Ok(false)` if no row matched (already gone — terminate's idempotent
    /// recovery path: if a previous terminate crashed between row delete and
    /// subdir removal, the next attempt finds the row already gone and
    /// continues with the orphan-subdir cleanup).
    pub async fn delete(&self, id: &VaultId) -> Result<bool> {
        let pool = self.pool.clone();
        let id = id.clone();
        task::spawn_blocking(move || {
            let conn = pool.get().context("acquiring vault_registry connection")?;
            let deleted = conn
                .execute("DELETE FROM vaults WHERE id = ?1", params![id.as_str()])
                .context("deleting vault row")?;
            Ok(deleted > 0)
        })
        .await
        .context("spawn_blocking join error in VaultRegistry::delete")?
    }
}

fn open_blocking(data_dir: PathBuf) -> Result<VaultRegistry> {
    std::fs::create_dir_all(&data_dir)
        .with_context(|| format!("creating data_dir {}", data_dir.display()))?;
    let db_path = data_dir.join(VAULTS_DB_FILE);
    let manager = SqliteConnectionManager::file(&db_path).with_init(|conn| {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Ok(())
    });
    let pool = Pool::builder()
        .max_size(POOL_MAX_SIZE)
        .build(manager)
        .with_context(|| format!("building r2d2 pool for {}", db_path.display()))?;
    let mut conn = pool.get().with_context(|| {
        format!(
            "acquiring initial connection from pool for {}",
            db_path.display()
        )
    })?;
    schema::ensure_schema(&mut conn)
        .with_context(|| format!("ensuring vaults.sqlite schema on {}", db_path.display()))?;
    drop(conn);
    info!("vault_registry: opened {}", db_path.display());
    Ok(VaultRegistry {
        pool,
        path: db_path,
    })
}

fn list_blocking(conn: &Connection, filter: Option<VaultStatus>) -> Result<Vec<VaultRow>> {
    let (sql, with_filter) = match filter {
        Some(_) => (
            "SELECT id, name, path, status, created_at, last_error \
             FROM vaults WHERE status = ?1 ORDER BY id",
            true,
        ),
        None => (
            "SELECT id, name, path, status, created_at, last_error \
             FROM vaults ORDER BY id",
            false,
        ),
    };
    let mut stmt = conn.prepare(sql).context("preparing list query")?;
    let raw_rows: Vec<RawRow> = if with_filter {
        let s = filter.unwrap().as_str();
        stmt.query_map(params![s], extract_raw)
            .context("running filtered list query")?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("collecting filtered list rows")?
    } else {
        stmt.query_map([], extract_raw)
            .context("running list query")?
            .collect::<rusqlite::Result<Vec<_>>>()
            .context("collecting list rows")?
    };
    raw_rows.into_iter().map(raw_to_vault).collect()
}

fn get_one_blocking(
    conn: &Connection,
    sql: &str,
    params: &[&dyn rusqlite::ToSql],
) -> Result<Option<VaultRow>> {
    let raw = conn
        .query_row(sql, params, extract_raw)
        .optional()
        .context("running get query")?;
    raw.map(raw_to_vault).transpose()
}

fn insert_blocking(conn: &Connection, row: &VaultRow) -> Result<()> {
    conn.execute(
        "INSERT INTO vaults (id, name, path, status, created_at, last_error) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            row.id.as_str(),
            row.name,
            path_to_string(&row.path)?,
            row.status.as_str(),
            row.created_at.to_rfc3339_opts(SecondsFormat::Micros, true),
            row.last_error,
        ],
    )
    .context("inserting vault row")?;
    Ok(())
}

fn update_status_blocking(
    conn: &Connection,
    id: &VaultId,
    status: VaultStatus,
    last_error: Option<&str>,
) -> Result<()> {
    let updated = conn
        .execute(
            "UPDATE vaults SET status = ?1, last_error = ?2 WHERE id = ?3",
            params![status.as_str(), last_error, id.as_str()],
        )
        .context("updating vault status")?;
    if updated == 0 {
        return Err(anyhow!("no vault row with id {id}"));
    }
    Ok(())
}

struct RawRow {
    id: String,
    name: String,
    path: String,
    status: String,
    created_at: String,
    last_error: Option<String>,
}

fn extract_raw(row: &rusqlite::Row<'_>) -> rusqlite::Result<RawRow> {
    Ok(RawRow {
        id: row.get(0)?,
        name: row.get(1)?,
        path: row.get(2)?,
        status: row.get(3)?,
        created_at: row.get(4)?,
        last_error: row.get(5)?,
    })
}

fn raw_to_vault(raw: RawRow) -> Result<VaultRow> {
    let status = VaultStatus::parse(&raw.status)
        .with_context(|| format!("decoding vault row id={}", raw.id))?;
    let created_at = DateTime::parse_from_rfc3339(&raw.created_at)
        .with_context(|| {
            format!(
                "parsing created_at {:?} for vault id={}",
                raw.created_at, raw.id
            )
        })?
        .with_timezone(&Utc);
    Ok(VaultRow {
        id: VaultId::from_string(raw.id),
        name: raw.name,
        path: PathBuf::from(raw.path),
        status,
        created_at,
        last_error: raw.last_error,
    })
}

fn path_to_string(path: &Path) -> Result<String> {
    path.to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("vault path is not valid UTF-8: {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample_row(id: VaultId, name: &str, path: &Path, status: VaultStatus) -> VaultRow {
        VaultRow {
            id,
            name: name.to_string(),
            path: path.to_path_buf(),
            status,
            created_at: Utc::now(),
            last_error: None,
        }
    }

    #[tokio::test]
    async fn vaults_sqlite_created_with_schema_at_first_open() {
        let dir = tempdir().unwrap();
        let registry = VaultRegistry::open(dir.path()).await.unwrap();
        assert_eq!(registry.path(), &dir.path().join(VAULTS_DB_FILE));
        assert!(registry.path().exists());

        let pool = registry.pool();
        let (vaults_count, schema_version): (i64, String) = task::spawn_blocking(move || {
            let conn = pool.get().unwrap();
            let vaults_count: i64 = conn
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'vaults'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            let v: String = conn
                .query_row(
                    "SELECT value FROM meta WHERE key = 'schema_version'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            (vaults_count, v)
        })
        .await
        .unwrap();
        assert_eq!(vaults_count, 1);
        assert_eq!(schema_version, SCHEMA_VERSION);
    }

    #[tokio::test]
    async fn open_against_existing_schema_succeeds() {
        let dir = tempdir().unwrap();
        {
            let _registry = VaultRegistry::open(dir.path()).await.unwrap();
        }
        let registry = VaultRegistry::open(dir.path()).await.unwrap();
        let rows = registry.list().await.unwrap();
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn open_against_wrong_schema_version_errors() {
        let dir = tempdir().unwrap();
        {
            let _registry = VaultRegistry::open(dir.path()).await.unwrap();
        }
        // Force-bump the on-disk schema_version to a future value.
        let db_path = dir.path().join(VAULTS_DB_FILE);
        task::spawn_blocking(move || {
            let conn = Connection::open(&db_path).unwrap();
            conn.execute(
                "UPDATE meta SET value = '2' WHERE key = 'schema_version'",
                [],
            )
            .unwrap();
        })
        .await
        .unwrap();

        let err = VaultRegistry::open(dir.path())
            .await
            .expect_err("re-open with bumped schema_version should error");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("schema_version"),
            "error should mention schema_version: {msg}"
        );
        assert!(
            msg.contains('2') && msg.contains(SCHEMA_VERSION),
            "error should mention both versions: {msg}"
        );
    }

    #[tokio::test]
    async fn insert_and_list_roundtrip() {
        let dir = tempdir().unwrap();
        let registry = VaultRegistry::open(dir.path()).await.unwrap();

        // Two distinct vaults; UUIDv7 minted in order so the lexicographic
        // ORDER BY id matches creation order.
        let id1 = VaultId::new();
        let id2 = VaultId::new();
        registry
            .insert(sample_row(
                id1.clone(),
                "alpha",
                Path::new("/tmp/alpha"),
                VaultStatus::Active,
            ))
            .await
            .unwrap();
        registry
            .insert(sample_row(
                id2.clone(),
                "beta",
                Path::new("/tmp/beta"),
                VaultStatus::Paused,
            ))
            .await
            .unwrap();

        let rows = registry.list().await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, id1);
        assert_eq!(rows[0].name, "alpha");
        assert_eq!(rows[0].status, VaultStatus::Active);
        assert_eq!(rows[1].id, id2);
        assert_eq!(rows[1].name, "beta");
        assert_eq!(rows[1].status, VaultStatus::Paused);

        let by_id = registry.get_by_id(&id1).await.unwrap().unwrap();
        assert_eq!(by_id.name, "alpha");
        let by_name = registry.get_by_name("beta").await.unwrap().unwrap();
        assert_eq!(by_name.id, id2);
        assert!(registry.get_by_name("missing").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn insert_with_duplicate_name_errors() {
        let dir = tempdir().unwrap();
        let registry = VaultRegistry::open(dir.path()).await.unwrap();
        registry
            .insert(sample_row(
                VaultId::new(),
                "shared",
                Path::new("/tmp/a"),
                VaultStatus::Active,
            ))
            .await
            .unwrap();
        let err = registry
            .insert(sample_row(
                VaultId::new(),
                "shared",
                Path::new("/tmp/b"),
                VaultStatus::Active,
            ))
            .await
            .expect_err("duplicate name should fail UNIQUE constraint");
        let msg = format!("{err:#}").to_lowercase();
        assert!(
            msg.contains("unique") && msg.contains("name"),
            "expected UNIQUE-name error, got {msg}"
        );
    }

    #[tokio::test]
    async fn insert_with_duplicate_path_errors() {
        let dir = tempdir().unwrap();
        let registry = VaultRegistry::open(dir.path()).await.unwrap();
        registry
            .insert(sample_row(
                VaultId::new(),
                "first",
                Path::new("/tmp/shared"),
                VaultStatus::Active,
            ))
            .await
            .unwrap();
        let err = registry
            .insert(sample_row(
                VaultId::new(),
                "second",
                Path::new("/tmp/shared"),
                VaultStatus::Active,
            ))
            .await
            .expect_err("duplicate path should fail UNIQUE constraint");
        let msg = format!("{err:#}").to_lowercase();
        assert!(
            msg.contains("unique") && msg.contains("path"),
            "expected UNIQUE-path error, got {msg}"
        );
    }

    #[tokio::test]
    async fn update_status_to_invalid_value_errors() {
        // The CHECK constraint should reject an UPDATE to anything outside
        // ('active','paused','errored') — verified by reaching into the
        // pool so we can craft the bad SQL directly (the typed
        // `update_status` API can't produce an invalid status).
        let dir = tempdir().unwrap();
        let registry = VaultRegistry::open(dir.path()).await.unwrap();
        let id = VaultId::new();
        registry
            .insert(sample_row(
                id.clone(),
                "v",
                Path::new("/tmp/v"),
                VaultStatus::Active,
            ))
            .await
            .unwrap();

        let pool = registry.pool();
        let id_str = id.to_string();
        let err = task::spawn_blocking(move || -> rusqlite::Result<()> {
            let conn = pool.get().unwrap();
            conn.execute(
                "UPDATE vaults SET status = 'banana' WHERE id = ?1",
                params![id_str],
            )?;
            Ok(())
        })
        .await
        .unwrap()
        .expect_err("CHECK constraint should reject 'banana'");
        let msg = format!("{err}").to_lowercase();
        assert!(
            msg.contains("check") || msg.contains("constraint"),
            "expected CHECK constraint error, got {msg}"
        );
    }

    #[tokio::test]
    async fn list_active_filters_paused_and_errored() {
        let dir = tempdir().unwrap();
        let registry = VaultRegistry::open(dir.path()).await.unwrap();
        let id_active = VaultId::new();
        registry
            .insert(sample_row(
                id_active.clone(),
                "a",
                Path::new("/tmp/a"),
                VaultStatus::Active,
            ))
            .await
            .unwrap();
        registry
            .insert(sample_row(
                VaultId::new(),
                "p",
                Path::new("/tmp/p"),
                VaultStatus::Paused,
            ))
            .await
            .unwrap();
        registry
            .insert(sample_row(
                VaultId::new(),
                "e",
                Path::new("/tmp/e"),
                VaultStatus::Errored,
            ))
            .await
            .unwrap();

        let active = registry.list_active().await.unwrap();
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].id, id_active);
        assert_eq!(active[0].status, VaultStatus::Active);
        assert_eq!(registry.list().await.unwrap().len(), 3);
    }

    #[test]
    fn vault_id_new_returns_time_ordered_uuids() {
        // UUIDv7 encodes a millisecond timestamp in its high bits, so two
        // consecutive mints sort lexicographically by creation time. With
        // millisecond resolution the same-millisecond case is real, so we
        // sample enough times to cross at least one tick.
        let mut ids: Vec<VaultId> = (0..32).map(|_| VaultId::new()).collect();
        let original = ids.clone();
        ids.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        assert_eq!(
            ids, original,
            "UUIDv7 mints should already be in lexicographic order"
        );
        // And consecutive mints should not collide.
        let unique: std::collections::HashSet<&str> = original.iter().map(|v| v.as_str()).collect();
        assert_eq!(
            unique.len(),
            original.len(),
            "consecutive UUIDv7 mints collided"
        );
    }
}
