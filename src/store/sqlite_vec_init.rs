use std::sync::Once;

static REGISTER: Once = Once::new();

/// Register sqlite-vec's static init function with SQLite for every connection
/// opened from this process. Idempotent; safe to call from multiple call sites.
/// Must run before the first rusqlite Connection is opened.
pub fn register_sqlite_vec() {
    REGISTER.call_once(|| {
        // SAFETY: sqlite3_auto_extension records the init pointer for SQLite to
        // invoke on every newly opened connection. The transmute matches the
        // upstream sqlite-vec 0.1.10-alpha.3 integration test: rusqlite's
        // binding expects `Option<unsafe extern "C" fn(...) -> c_int>` while
        // sqlite-vec's symbol is plain `extern "C" fn()`. The function pointer
        // remains valid for the entire process lifetime (static linkage).
        #[allow(clippy::missing_transmute_annotations)]
        unsafe {
            rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(
                sqlite_vec::sqlite3_vec_init as *const (),
            )));
        }
    });
}
