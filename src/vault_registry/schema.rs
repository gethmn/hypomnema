use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OptionalExtension, params};

pub const SCHEMA_VERSION: &str = "1";

const CREATE_VAULTS: &str = "
    CREATE TABLE IF NOT EXISTS vaults (
        id          TEXT PRIMARY KEY NOT NULL,
        name        TEXT NOT NULL UNIQUE,
        path        TEXT NOT NULL UNIQUE,
        status      TEXT NOT NULL
                    CHECK (status IN ('active', 'paused', 'errored')),
        created_at  TEXT NOT NULL,
        last_error  TEXT
    );
";

const CREATE_META: &str = "
    CREATE TABLE IF NOT EXISTS meta (
        key   TEXT PRIMARY KEY NOT NULL,
        value TEXT NOT NULL
    );
";

pub fn ensure_schema(conn: &mut Connection) -> Result<()> {
    let tx = conn.transaction().context("beginning transaction")?;
    tx.execute_batch(CREATE_VAULTS)
        .context("creating vaults table")?;
    tx.execute_batch(CREATE_META)
        .context("creating meta table")?;

    let existing: Option<String> = tx
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .optional()
        .context("reading meta.schema_version")?;

    match existing {
        None => {
            tx.execute(
                "INSERT INTO meta (key, value) VALUES ('schema_version', ?1)",
                params![SCHEMA_VERSION],
            )
            .context("seeding meta.schema_version")?;
        }
        Some(v) if v == SCHEMA_VERSION => {}
        Some(v) => {
            bail!(
                "vaults.sqlite schema_version is {v}, expected {SCHEMA_VERSION}; \
                 the daemon may be older than the on-disk vaults.sqlite \
                 (downgrade is not supported)."
            );
        }
    }

    tx.commit().context("committing schema initialization")?;
    Ok(())
}
