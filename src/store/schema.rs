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

    fn user_version(conn: &Connection) -> i64 {
        conn.query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap()
    }

    #[test]
    fn fresh_in_memory_db_advances_user_version() {
        let mut conn = Connection::open_in_memory().unwrap();
        assert_eq!(user_version(&conn), 0);
        apply_migrations(&mut conn).unwrap();
        assert_eq!(user_version(&conn), MIGRATIONS.len() as i64);
    }

    #[test]
    fn re_apply_is_noop() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply_migrations(&mut conn).unwrap();
        let after_first = user_version(&conn);
        apply_migrations(&mut conn).unwrap();
        assert_eq!(user_version(&conn), after_first);
    }

    #[test]
    fn files_table_is_created() {
        let mut conn = Connection::open_in_memory().unwrap();
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
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "user_version", (MIGRATIONS.len() + 1) as i64)
            .unwrap();
        let err = apply_migrations(&mut conn).unwrap_err();
        assert!(format!("{err:#}").contains("ahead of code-known migrations"));
    }

    #[test]
    fn migration_0002_adds_content_column() {
        let mut conn = Connection::open_in_memory().unwrap();
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
        let mut conn = Connection::open_in_memory().unwrap();
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
        let mut conn = Connection::open_in_memory().unwrap();
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
        let mut conn = Connection::open_in_memory().unwrap();
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
    fn migrations_advance_user_version_to_2() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply_migrations(&mut conn).unwrap();
        assert_eq!(user_version(&conn), 2);
    }
}
