---
name: sqlite-vec-extension
description: Use when working with vector storage, embedding indexes, or semantic search in Hypomnema. Covers loading the sqlite-vec extension at runtime, schema patterns for vec0 virtual tables versus regular metadata tables, insert and delete patterns, and the SQL shape of vector search queries. Apply whenever code touches embeddings, semantic similarity, or the index.sqlite file.
---

# sqlite-vec in Hypomnema

sqlite-vec is a SQLite extension, not a separate database. It loads into a standard SQLite connection at runtime and adds vector types and operations via virtual tables.

Check the upstream docs for current API syntax: https://github.com/asg017/sqlite-vec — this skill captures the patterns we use, but exact function names and SQL idioms evolve.

## Loading the extension

rusqlite needs the `load_extension` feature enabled. The extension itself ships as a dynamic library (`.dylib` on Mac, `.so` on Linux, `.dll` on Windows) that lives alongside the Hypomnema binary. Load it exactly once per connection, during pool initialization.

```rust
let manager = SqliteConnectionManager::file(&db_path)
    .with_init(|conn| {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        unsafe {
            conn.load_extension_enable()?;
            conn.load_extension(&vec_ext_path, None::<&str>)?;
            conn.load_extension_disable()?;
        }
        Ok(())
    });
```

The `unsafe` is unavoidable — loading native code always is. The extension path is read from config with a sensible default; don't hardcode it in source.

## Schema split

Vector data goes in a `vec0` virtual table. Metadata goes in a regular table. Join at query time. This split matters: `vec0` tables have limited column support and don't index non-vector columns well.

```sql
CREATE TABLE chunks (
    id INTEGER PRIMARY KEY,
    file_path TEXT NOT NULL,
    heading_path TEXT,
    content TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    start_byte INTEGER NOT NULL,
    end_byte INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE INDEX idx_chunks_file_path ON chunks(file_path);

CREATE VIRTUAL TABLE chunk_vectors USING vec0(
    chunk_id INTEGER PRIMARY KEY,
    embedding FLOAT[768]
);
```

The dimension (`768` for nomic-embed-text) is baked into the schema. Don't make this a runtime variable in v0 — pick one model, bake the dimension, revisit if model-switching ever becomes real.

## Insert

Embeddings are raw `f32` bytes. Use `bytemuck::cast_slice` to convert `&[f32]` to `&[u8]` without copying.

```rust
// Inside spawn_blocking, with embedding already computed:
let tx = conn.transaction()?;
tx.execute(
    "INSERT INTO chunks (file_path, heading_path, content, content_hash, start_byte, end_byte, created_at)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    params![path_str, heading_path, content, content_hash, start, end, now],
)?;
let chunk_id = tx.last_insert_rowid();
tx.execute(
    "INSERT INTO chunk_vectors (chunk_id, embedding) VALUES (?1, ?2)",
    params![chunk_id, bytemuck::cast_slice::<f32, u8>(&embedding)],
)?;
tx.commit()?;
```

## Delete before re-embed

When a file changes, delete its chunks and re-insert. Don't update in place — chunk boundaries shift when content changes, and updating vectors in vec0 is awkward.

```rust
tx.execute(
    "DELETE FROM chunk_vectors WHERE chunk_id IN (SELECT id FROM chunks WHERE file_path = ?1)",
    [path_str],
)?;
tx.execute("DELETE FROM chunks WHERE file_path = ?1", [path_str])?;
// ... then insert new chunks
```

Do the delete and insert in the same transaction.

## Semantic search query

```sql
SELECT
    c.id, c.file_path, c.heading_path, c.content, c.start_byte, c.end_byte,
    v.distance
FROM chunk_vectors v
JOIN chunks c ON c.id = v.chunk_id
WHERE v.embedding MATCH ?1
  AND k = ?2
ORDER BY v.distance;
```

The `MATCH` operator and the `k` pseudo-column are sqlite-vec idioms. `k` is the number of nearest neighbors to return. Put it in the WHERE, not as a LIMIT. Confirm the exact syntax against upstream docs — the sqlite-vec API has moved around during its development.

## Smells

- Storing embeddings anywhere other than a `vec0` virtual table.
- Putting non-vector columns in the `vec0` table beyond the primary key — it won't give you useful indexes over them.
- Running embedding generation inside `spawn_blocking` — embedding is a network call (HTTP to TEI/Ollama) and belongs on the async runtime. Only the SQL write is blocking.
- Hardcoded extension path in source code.
- Updating a vector row in place rather than delete-and-insert.
- Mismatched embedding dimensions between config and schema — fail loudly at startup if these disagree.

## Schema migrations

For v0: if the schema needs to change, drop the database file and re-index from scratch. Don't write migration code yet. Re-indexing is free — the vault is the source of truth, and scans are fast at this scale.
