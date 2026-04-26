use anyhow::{Context, Result, bail};
use rusqlite::Connection;
use tracing::info;

pub const MIGRATIONS: &[&str] = &[
    // 0001 — initial files table per step-2 workplan § Resolution 4.
    "CREATE TABLE files (
        path           TEXT PRIMARY KEY,
        size           INTEGER NOT NULL,
        mtime          TEXT    NOT NULL,
        content_hash   TEXT    NOT NULL,
        indexed_at     TEXT    NOT NULL
    ) STRICT;",
    // 0002 — content storage for grep-shaped queries per step-5 workplan
    // § Resolution A. The DELETE clears any rows present before the schema
    // bump so the next bulk scan repopulates with bodies.
    "ALTER TABLE files ADD COLUMN content TEXT NOT NULL DEFAULT '';
     DELETE FROM files;",
    // 0003 — chunks + chunks_vec per step-6 workplan § Task 6.1.
    // STRICT applies only to the regular `chunks` table; vec0 virtual tables
    // do not accept STRICT. The 768 dimension is the schema-baked source of
    // truth per ADR-0007; runtime validation lives in
    // `Store::validate_dimension`.
    "CREATE TABLE chunks (
        id            INTEGER PRIMARY KEY,
        file_path     TEXT    NOT NULL,
        chunk_index   INTEGER NOT NULL,
        heading_path  TEXT    NOT NULL,
        content       TEXT    NOT NULL,
        content_hash  TEXT    NOT NULL,
        start_byte    INTEGER NOT NULL,
        end_byte      INTEGER NOT NULL,
        created_at    TEXT    NOT NULL,
        UNIQUE (file_path, chunk_index)
    ) STRICT;
    CREATE INDEX idx_chunks_file_path ON chunks(file_path);
    CREATE VIRTUAL TABLE chunks_vec USING vec0(
        chunk_id INTEGER PRIMARY KEY,
        embedding FLOAT[768]
    );",
    // 0004 — recreate chunks_vec with schema-baked cosine distance per
    // step-7 workplan § Resolution F. Truncate `chunks` and clear
    // `files.content_hash` so the next scan re-reads, re-chunks, and
    // re-embeds; the vault is the source of truth per ADR-0006.
    // Order matters: drop chunks_vec before deleting chunks (chunks_vec
    // is dropped, not joined). The dimension validation regex in
    // `Store::validate_dimension` matches `embedding FLOAT[<dim>]` and
    // ignores trailing column-level options like `distance_metric=...`.
    "DROP TABLE chunks_vec;
     DELETE FROM chunks;
     UPDATE files SET content_hash = '';
     CREATE VIRTUAL TABLE chunks_vec USING vec0(
         chunk_id INTEGER PRIMARY KEY,
         embedding FLOAT[768] distance_metric=cosine
     );",
];

pub fn apply_migrations(conn: &mut Connection) -> Result<()> {
    let current: i64 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .context("reading PRAGMA user_version")?;
    let target = MIGRATIONS.len() as i64;

    if current == target {
        return Ok(());
    }
    if current > target {
        bail!(
            "database user_version ({current}) is ahead of code-known migrations ({target}); refusing to run"
        );
    }

    for (idx, migration) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let next_version = (idx + 1) as i64;
        let tx = conn
            .transaction()
            .with_context(|| format!("beginning transaction for migration {next_version}"))?;
        tx.execute_batch(migration)
            .with_context(|| format!("applying migration {next_version}"))?;
        tx.pragma_update(None, "user_version", next_version)
            .with_context(|| format!("setting user_version to {next_version}"))?;
        tx.commit()
            .with_context(|| format!("committing migration {next_version}"))?;
        info!("store: applied migration {next_version}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EmbeddingConfig;

    fn user_version(conn: &Connection) -> i64 {
        conn.query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap()
    }

    /// In-memory SQLite connection with the sqlite-vec extension loaded.
    /// Migration 0003 uses `vec0` virtual-table syntax that requires the
    /// extension; tests must mirror the production `with_init` shape.
    fn test_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        let path = EmbeddingConfig::default().resolved_extension_path();
        // SAFETY: same constraints as the production load in `pool::build_pool`.
        // The path resolves from default config (or the HYPOMNEMA_VEC_EXT_PATH
        // env-var override when set by the test runner).
        unsafe {
            conn.load_extension_enable()
                .expect("rusqlite load_extension feature not enabled");
            conn.load_extension(&path, Some("sqlite3_vec_init"))
                .unwrap_or_else(|e| {
                    panic!(
                        "loading sqlite-vec extension at {} failed: {e}\n\
                     hint: place the dylib at the default path or set HYPOMNEMA_VEC_EXT_PATH",
                        path.display()
                    )
                });
            conn.load_extension_disable().unwrap();
        }
        conn
    }

    #[test]
    fn fresh_in_memory_db_advances_user_version() {
        let mut conn = test_conn();
        assert_eq!(user_version(&conn), 0);
        apply_migrations(&mut conn).unwrap();
        assert_eq!(user_version(&conn), MIGRATIONS.len() as i64);
    }

    #[test]
    fn re_apply_is_noop() {
        let mut conn = test_conn();
        apply_migrations(&mut conn).unwrap();
        let after_first = user_version(&conn);
        apply_migrations(&mut conn).unwrap();
        assert_eq!(user_version(&conn), after_first);
    }

    #[test]
    fn files_table_is_created() {
        let mut conn = test_conn();
        apply_migrations(&mut conn).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'files'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn ahead_of_code_is_rejected() {
        let mut conn = test_conn();
        conn.pragma_update(None, "user_version", (MIGRATIONS.len() + 1) as i64)
            .unwrap();
        let err = apply_migrations(&mut conn).unwrap_err();
        assert!(format!("{err:#}").contains("ahead of code-known migrations"));
    }

    #[test]
    fn migration_0002_adds_content_column() {
        let mut conn = test_conn();
        apply_migrations(&mut conn).unwrap();
        let mut stmt = conn.prepare("PRAGMA table_info(files)").unwrap();
        let names: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert!(
            names.iter().any(|n| n == "content"),
            "content column missing; columns: {names:?}"
        );
    }

    #[test]
    fn migration_0002_clears_rows_from_pre_existing_db() {
        let mut conn = test_conn();
        conn.execute_batch(MIGRATIONS[0]).unwrap();
        conn.pragma_update(None, "user_version", 1i64).unwrap();
        conn.execute(
            "INSERT INTO files (path, size, mtime, content_hash, indexed_at)
             VALUES ('a.md', 1, '2026-01-01T00:00:00Z', 'sha256:00', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        let before: i64 = conn
            .query_row("SELECT count(*) FROM files", [], |row| row.get(0))
            .unwrap();
        assert_eq!(before, 1);

        apply_migrations(&mut conn).unwrap();

        let after: i64 = conn
            .query_row("SELECT count(*) FROM files", [], |row| row.get(0))
            .unwrap();
        assert_eq!(after, 0);
    }

    #[test]
    fn content_column_is_not_null_with_empty_default() {
        let mut conn = test_conn();
        apply_migrations(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO files (path, size, mtime, content_hash, indexed_at)
             VALUES ('a.md', 1, '2026-01-01T00:00:00Z', 'sha256:00', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        let content: String = conn
            .query_row("SELECT content FROM files WHERE path = 'a.md'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(content, "");
    }

    #[test]
    fn content_column_accepts_arbitrary_utf8() {
        let mut conn = test_conn();
        apply_migrations(&mut conn).unwrap();
        let body = "héllo café";
        conn.execute(
            "INSERT INTO files (path, size, mtime, content_hash, indexed_at, content)
             VALUES ('a.md', 1, '2026-01-01T00:00:00Z', 'sha256:00', '2026-01-01T00:00:00Z', ?1)",
            [body],
        )
        .unwrap();
        let read_back: String = conn
            .query_row("SELECT content FROM files WHERE path = 'a.md'", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(read_back, body);
    }

    #[test]
    fn migration_0003_creates_chunks_table() {
        let mut conn = test_conn();
        apply_migrations(&mut conn).unwrap();
        let mut stmt = conn.prepare("PRAGMA table_info(chunks)").unwrap();
        // (cid, name, type, notnull, dflt_value, pk)
        let cols: Vec<(String, String, i64, i64)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(5)?,
                ))
            })
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        let by_name: std::collections::HashMap<&str, &(String, String, i64, i64)> =
            cols.iter().map(|c| (c.0.as_str(), c)).collect();
        let expected = [
            ("id", "INTEGER", 0, 1),
            ("file_path", "TEXT", 1, 0),
            ("chunk_index", "INTEGER", 1, 0),
            ("heading_path", "TEXT", 1, 0),
            ("content", "TEXT", 1, 0),
            ("content_hash", "TEXT", 1, 0),
            ("start_byte", "INTEGER", 1, 0),
            ("end_byte", "INTEGER", 1, 0),
            ("created_at", "TEXT", 1, 0),
        ];
        for (name, ty, notnull, pk) in expected {
            let got = by_name
                .get(name)
                .unwrap_or_else(|| panic!("missing column {name}; columns: {cols:?}"));
            assert_eq!(got.1, ty, "type mismatch for {name}");
            assert_eq!(got.2, notnull, "notnull mismatch for {name}");
            assert_eq!(got.3, pk, "pk mismatch for {name}");
        }
    }

    #[test]
    fn migration_0003_creates_chunks_vec() {
        let mut conn = test_conn();
        apply_migrations(&mut conn).unwrap();
        let sql: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'chunks_vec'",
                [],
                |row| row.get(0),
            )
            .expect("chunks_vec virtual table should exist");
        assert!(
            sql.contains("USING vec0"),
            "expected `USING vec0` in {sql:?}"
        );
    }

    #[test]
    fn migration_0003_chunks_vec_dimension_is_768() {
        let mut conn = test_conn();
        apply_migrations(&mut conn).unwrap();
        let sql: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'chunks_vec'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let re = regex::Regex::new(r"embedding\s+FLOAT\[(\d+)\]").unwrap();
        let caps = re
            .captures(&sql)
            .unwrap_or_else(|| panic!("no FLOAT[<dim>] in {sql:?}"));
        let dim: u32 = caps[1].parse().unwrap();
        assert_eq!(dim, 768);
    }

    #[test]
    fn migrations_advance_user_version_to_4() {
        let mut conn = test_conn();
        apply_migrations(&mut conn).unwrap();
        assert_eq!(user_version(&conn), 4);
    }

    #[test]
    fn migration_0004_chunks_vec_uses_cosine_metric() {
        let mut conn = test_conn();
        apply_migrations(&mut conn).unwrap();
        let sql: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'chunks_vec'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            sql.contains("distance_metric=cosine"),
            "expected `distance_metric=cosine` in {sql:?}"
        );
    }

    #[test]
    fn migration_0004_clears_files_content_hash_and_chunks() {
        let mut conn = test_conn();
        // Apply migrations 0001..=0003, stop short of 0004 so we can seed
        // pre-migration rows (post-0003 chunks rows + a non-empty files
        // content_hash) and then assert 0004 truncates / clears them.
        for (idx, migration) in MIGRATIONS.iter().enumerate().take(3) {
            conn.execute_batch(migration).unwrap();
            conn.pragma_update(None, "user_version", (idx + 1) as i64)
                .unwrap();
        }
        assert_eq!(user_version(&conn), 3);

        conn.execute(
            "INSERT INTO files (path, size, mtime, content_hash, indexed_at, content)
             VALUES ('a.md', 1, '2026-01-01T00:00:00Z', 'sha256:beef', '2026-01-01T00:00:00Z', 'hi')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO chunks (file_path, chunk_index, heading_path, content, content_hash, start_byte, end_byte, created_at)
             VALUES ('a.md', 0, '', 'hi', 'sha256:beef', 0, 2, '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        apply_migrations(&mut conn).unwrap();
        assert_eq!(user_version(&conn), 4);

        let content_hash: String = conn
            .query_row(
                "SELECT content_hash FROM files WHERE path = 'a.md'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(content_hash, "");

        let chunk_count: i64 = conn
            .query_row("SELECT count(*) FROM chunks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(chunk_count, 0);
    }

    #[test]
    fn chunks_unique_constraint_on_file_path_chunk_index() {
        let mut conn = test_conn();
        apply_migrations(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO chunks (file_path, chunk_index, heading_path, content, content_hash, start_byte, end_byte, created_at)
             VALUES ('a.md', 0, '', 'hello', 'sha256:00', 0, 5, '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
        let err = conn.execute(
            "INSERT INTO chunks (file_path, chunk_index, heading_path, content, content_hash, start_byte, end_byte, created_at)
             VALUES ('a.md', 0, 'x', 'world', 'sha256:01', 0, 5, '2026-01-01T00:00:00Z')",
            [],
        )
        .expect_err("expected UNIQUE (file_path, chunk_index) violation");
        let msg = format!("{err}");
        assert!(
            msg.to_lowercase().contains("unique"),
            "expected UNIQUE constraint error, got {msg}"
        );
    }
}
