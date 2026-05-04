use std::path::Path;

use anyhow::{Context, Result};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;

pub type SqlitePool = Pool<SqliteConnectionManager>;

const POOL_MAX_SIZE: u32 = 8;

pub fn build_pool(db_path: &Path) -> Result<SqlitePool> {
    let manager = SqliteConnectionManager::file(db_path).with_init(move |conn| {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Ok(())
    });
    Pool::builder()
        .max_size(POOL_MAX_SIZE)
        .build(manager)
        .with_context(|| format!("building r2d2 pool for {}", db_path.display()))
}
