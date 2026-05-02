# 07 — Manual Testing: `content_get`

Step 19 ships `POST /content/get` (HTTP), `content_get` (MCP tool), and `hmn content get` (CLI).
All three surface the same read-only operation: fetch indexed file text by vault-relative path.

---

## Preparation

```bash
# 1. Start the daemon
cargo run --bin hmnd

# 2. In another terminal, create a test vault with known files
mkdir -p /tmp/test-vault/notes
printf '# File A\nSome content for A.\n' > /tmp/test-vault/notes/a.md
printf '# File B\nSome content for B.\n' > /tmp/test-vault/notes/b.md

# 3. Register the vault
hmn vault add /tmp/test-vault --name test-vault

# 4. Wait for the initial scan (about 1-2 s on most machines)
sleep 2

# 5. Confirm files are indexed
hmn search filesystem --glob '**/*.md' | jq .
```

---

## Test Cases

### T1 — Single-file retrieval (HTTP)

```bash
curl -s -X POST http://localhost:8080/content/get \
  -H 'Content-Type: application/json' \
  -d '{"paths": ["notes/a.md"]}' | jq .
```

**Verify:**
- HTTP 200
- `results[0].path == "notes/a.md"`
- `results[0].content == "# File A\nSome content for A.\n"`
- `results[0].content_hash` is a non-empty string
- `results[0].size > 0`
- `results[0].mtime` is a non-empty string
- `results[0].vault` is a UUID string
- `results[0].vault_name == "test-vault"`
- No `error` key on the item

---

### T2 — Multi-file batch with a missing file (HTTP)

```bash
curl -s -X POST http://localhost:8080/content/get \
  -H 'Content-Type: application/json' \
  -d '{"paths": ["notes/a.md", "notes/missing.md", "notes/b.md"]}' | jq .
```

**Verify:**
- HTTP 200 (even though one item is a miss)
- `results` array has 3 items
- Items are ordered by path ASC: `notes/a.md`, `notes/b.md`, `notes/missing.md`
- `results[0]` and `results[1]` are success items with `content` populated
- `results[2].path == "notes/missing.md"` and `results[2].error.code == "path_not_found"`

---

### T3 — Default fan-out across all vaults (HTTP)

```bash
# Register a second vault
mkdir -p /tmp/test-vault-2/notes
printf '# Shared file in vault 2\n' > /tmp/test-vault-2/notes/a.md
hmn vault add /tmp/test-vault-2 --name test-vault-2
sleep 2

# Query both vaults with no vault scope
curl -s -X POST http://localhost:8080/content/get \
  -H 'Content-Type: application/json' \
  -d '{"paths": ["notes/a.md"]}' | jq .
```

**Verify:**
- HTTP 200
- `results` has 2 items (one per vault)
- Both items have `path == "notes/a.md"` and different `vault` / `vault_name` values
- Items ordered by `(path ASC, vault_id ASC)`

---

### T4 — Explicit vault scoping (HTTP)

```bash
curl -s -X POST http://localhost:8080/content/get \
  -H 'Content-Type: application/json' \
  -d '{"paths": ["notes/a.md"], "vaults": ["test-vault"]}' | jq .
```

**Verify:**
- HTTP 200
- `results` has exactly 1 item
- `results[0].vault_name == "test-vault"`

---

### T5 — Error case: missing file (HTTP)

```bash
curl -s -X POST http://localhost:8080/content/get \
  -H 'Content-Type: application/json' \
  -d '{"paths": ["notes/does-not-exist.md"]}' | jq .
```

**Verify:**
- HTTP 200 (per-item errors do not fail the batch)
- `results[0].error.code == "path_not_found"`

---

### T6 — Error case: invalid path (HTTP)

```bash
# Absolute path
curl -s -X POST http://localhost:8080/content/get \
  -H 'Content-Type: application/json' \
  -d '{"paths": ["/etc/passwd"]}' | jq .
```

**Verify:**
- HTTP 422
- `error.code == "invalid_path"`

```bash
# Path traversal
curl -s -X POST http://localhost:8080/content/get \
  -H 'Content-Type: application/json' \
  -d '{"paths": ["../escape.md"]}' | jq .
```

**Verify:**
- HTTP 422
- `error.code == "invalid_path"`

---

### T7 — CLI retrieval (human-readable)

```bash
hmn content get notes/a.md notes/b.md
```

**Verify:**
- Each file printed with a header block:
  ```
  PATH: notes/a.md
  VAULT: test-vault
  HASH: <sha256:...>
  SIZE: <bytes>
  MTIME: <timestamp>
  ---
  # File A
  Some content for A.
  ```

---

### T8 — CLI retrieval (JSON)

```bash
hmn content get notes/a.md --json | jq .
```

**Verify:**
- Output matches the HTTP response envelope shape
- `results[0].content` matches the file body

---

### T9 — MCP tool list verification

```bash
hmn mcp --json | jq '.tools[] | select(.name == "content_get")'
```

**Verify:**
- Tool `content_get` appears in the list
- `description` is non-empty
- `inputSchema.properties.paths` is present

---

## Cleanup

```bash
hmn vault terminate test-vault
hmn vault terminate test-vault-2
rm -rf /tmp/test-vault /tmp/test-vault-2
```
