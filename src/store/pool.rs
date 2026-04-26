use std::path::Path;

use anyhow::{Context, Result};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;

pub type SqlitePool = Pool<SqliteConnectionManager>;

const POOL_MAX_SIZE: u32 = 8;

pub fn build_pool(db_path: &Path, extension_path: &Path) -> Result<SqlitePool> {
    let ext_owned = extension_path.to_path_buf();
    let manager = SqliteConnectionManager::file(db_path).with_init(move |conn| {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        // SAFETY: loading native code is inherently unsafe; the path comes from
        // configuration validated at startup, and the extension is the
        // sqlite-vec dynamic library described in ADR-0007. The entry point is
        // pinned to `sqlite3_vec_init` rather than letting SQLite derive it
        // from the filename — that derivation depends on basename munging that
        // breaks for `sqlite-vec.dylib` (the dash collapses).
        unsafe {
            conn.load_extension_enable()?;
            conn.load_extension(&ext_owned, Some("sqlite3_vec_init"))?;
            conn.load_extension_disable()?;
        }
        Ok(())
    });
    Pool::builder()
        .max_size(POOL_MAX_SIZE)
        .build(manager)
        .with_context(|| {
            format!(
                "building r2d2 pool for {} (sqlite-vec extension at {})",
                db_path.display(),
                extension_path.display()
            )
        })
}
