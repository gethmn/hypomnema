---
name: rusqlite-in-async
description: Use when writing or reviewing code in the Hypomnema project that calls rusqlite methods from an async context. Covers the spawn_blocking pattern, connection pool handling, and the blocking-in-async trap that causes runtime deadlocks. Apply whenever rusqlite::Connection, rusqlite::Transaction, r2d2 pool access, or any SQL execution appears inside or near an async fn.
---

# rusqlite in async code

rusqlite is synchronous. Tokio tasks are not. Calling a rusqlite method directly from an async function blocks the runtime thread that task is running on — if enough tasks do this at once, all runtime threads block and the daemon deadlocks.

## The pattern

Every rusqlite call goes inside `tokio::task::spawn_blocking`.

```rust
use tokio::task;

async fn get_chunk_by_id(pool: Pool<SqliteConnectionManager>, id: i64) -> anyhow::Result<Chunk> {
    let chunk = task::spawn_blocking(move || -> anyhow::Result<Chunk> {
        let conn = pool.get()?;
        let chunk = conn.query_row(
            "SELECT id, path, content FROM chunks WHERE id = ?1",
            [id],
            |row| Ok(Chunk {
                id: row.get(0)?,
                path: row.get(1)?,
                content: row.get(2)?,
            }),
        )?;
        Ok(chunk)
    })
    .await??; // first ? for JoinError, second for the inner Result
    Ok(chunk)
}
```

Two `?`s because `spawn_blocking` returns `Result<T, JoinError>` where `T` is your closure's return type (itself a `Result`).

## Connection pool

r2d2 + r2d2_sqlite gives a blocking connection pool. Don't try to make the pool async — it isn't, and pretending otherwise produces subtle bugs. Acquire the connection *inside* `spawn_blocking`, not outside.

```rust
let manager = SqliteConnectionManager::file(&db_path)
    .with_init(|conn| {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Ok(())
    });
let pool = r2d2::Pool::builder().max_size(8).build(manager)?;
```

The pool itself is `Clone` and cheap to pass into `spawn_blocking` closures.

## Transactions

A transaction happens entirely inside one `spawn_blocking` call. Don't split a transaction across awaits — the connection isn't `Send` across await boundaries in a way that composes with transactions.

```rust
task::spawn_blocking(move || -> anyhow::Result<()> {
    let mut conn = pool.get()?;
    let tx = conn.transaction()?;
    for chunk in chunks {
        tx.execute("INSERT INTO chunks (...) VALUES (...)", params![/* ... */])?;
    }
    tx.commit()?;
    Ok(())
})
.await??;
```

## Smells

- An `.await` between acquiring a connection and using it — the connection should live entirely inside one `spawn_blocking`.
- A `rusqlite::Connection` or `rusqlite::Transaction` held across an `.await` boundary — almost always wrong.
- Loops that call a single-row rusqlite method inside a sequence of `spawn_blocking` calls — batch into one `spawn_blocking` or prepare-and-execute-N inside one call.
- `tokio::fs` adjacent to rusqlite calls — the two shouldn't interleave in the same function.

## Why not sqlx

sqlx has an async SQLite driver. We're sticking with rusqlite because sqlite-vec integration is cleaner with rusqlite's extension-loading API, and because SQLite is blocking at the syscall level anyway — async SQLite drivers mostly paper over `spawn_blocking` for you, and the paper-over has its own costs.

## If you're unsure

Before merging code that mixes rusqlite and async, trace each SQL call from the caller and confirm: (1) it's inside a `spawn_blocking`, (2) the connection lives only within that closure, (3) no `.await` appears between connection acquisition and connection drop.
