# pgvector

PostgreSQL extension that adds vector similarity search to standard SQL,
backed by IVFFlat or HNSW indexes.

## Index types

Two main index choices: IVFFlat for moderate-recall workloads, HNSW for
higher accuracy at the cost of build time and memory. HNSW is the default
we reach for in production.

```sql
CREATE INDEX ON items USING hnsw (embedding vector_cosine_ops);
```

## Comparison with sqlite-vec

sqlite-vec is the closest analogue in the SQLite world: vector similarity
search via a virtual table, with cosine distance as a first-class metric.
Hypomnema uses sqlite-vec instead of pgvector because the daemon is a
single-process tool and doesn't need a separate database server.
