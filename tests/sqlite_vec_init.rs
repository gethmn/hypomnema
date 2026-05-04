//! Canary test for static sqlite-vec registration.
//!
//! Proves that `register_sqlite_vec()` (a) makes `vec_version()` callable on a
//! freshly opened SQLite connection, (b) is idempotent — repeat calls are
//! safe, and (c) the registration is process-wide, applying to a second
//! independent connection opened later from a different pool.

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use tempfile::tempdir;

#[tokio::test]
async fn register_sqlite_vec_works_across_two_independent_connections() {
    hypomnema::store::register_sqlite_vec();
    hypomnema::store::register_sqlite_vec();

    let dir = tempdir().expect("tempdir");
    let db_a = dir.path().join("a.sqlite");
    let db_b = dir.path().join("b.sqlite");

    let (v_a1, v_a2, v_b) = tokio::task::spawn_blocking(move || {
        let pool_a: Pool<SqliteConnectionManager> = Pool::builder()
            .max_size(8)
            .build(SqliteConnectionManager::file(&db_a))
            .expect("build pool A");

        let c1 = pool_a.get().expect("pool A conn 1");
        let c2 = pool_a.get().expect("pool A conn 2 (must be a distinct connection)");

        let v_a1: String = c1
            .query_row("SELECT vec_version()", [], |r| r.get(0))
            .expect("vec_version on pool A conn 1");
        let v_a2: String = c2
            .query_row("SELECT vec_version()", [], |r| r.get(0))
            .expect("vec_version on pool A conn 2");

        hypomnema::store::register_sqlite_vec();

        let pool_b: Pool<SqliteConnectionManager> = Pool::builder()
            .max_size(8)
            .build(SqliteConnectionManager::file(&db_b))
            .expect("build pool B");
        let c3 = pool_b.get().expect("pool B conn");
        let v_b: String = c3
            .query_row("SELECT vec_version()", [], |r| r.get(0))
            .expect("vec_version on pool B conn");

        (v_a1, v_a2, v_b)
    })
    .await
    .expect("blocking task panicked");

    assert!(
        v_a1.starts_with('v'),
        "vec_version() returned unexpected value: {v_a1:?}"
    );
    assert_eq!(v_a1, v_a2, "vec_version must agree across pool connections");
    assert_eq!(
        v_a1, v_b,
        "vec_version must agree across independent pools (registration is process-wide)"
    );
}
