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
}
