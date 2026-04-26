# ADR-0006: Outbox (and All Daemon State) Lives Outside the Watched Directory

**Status**: accepted
**Date**: 2026-04-23
**Decision-Makers**: Beau Simensen

---

## Context

Hypomnema emits change events to an append-only JSONL log so that consumers can subscribe without polling. The natural question is where that log lives on disk.

Two broad options:
1. **Inside the watched directory** — e.g., `<vault>/.hypomnema/outbox.jsonl`. Keeps everything about a given vault "together." Matches the `.git/`, `.obsidian/` pattern.
2. **In the daemon's own data directory** — e.g., `~/.local/share/hypomnema/outbox.jsonl`. Separates user-owned vault content from daemon-owned state.

The same question applies to the SQLite index file, the daemon logs, and the configuration — all of them are mutable state maintained by the daemon, not user content.

A vault of Markdown notes is the kind of directory users very commonly sync via Syncthing, Dropbox, iCloud Drive, or Obsidian Sync. A constantly-growing file (the outbox) or a frequently-mutated binary (the SQLite index) inside a synced directory produces pathological behavior: sync conflicts, wasted bandwidth, spurious change notifications fanning out to other devices, and in the worst cases corruption when two devices write the same bytes concurrently.

## Decision

All state Hypomnema maintains — the SQLite index (`index.sqlite`), the outbox (`outbox.jsonl`), the daemon logs, and the configuration file — lives in the daemon's own data directory (`~/.local/share/hypomnema/` on Linux and macOS, `%APPDATA%\hypomnema\` on Windows), or in XDG-standard config/log directories. **Nothing mutable is written under the watched directory.**

macOS deliberately uses XDG paths rather than `~/Library/Application Support/` — keeping one layout across Unix avoids branching in daemon code and in docs.

The daemon reads from the watched directory; it never writes there.

## Consequences

### Positive

- Safe to run on a synced vault (Syncthing / Dropbox / Obsidian Sync / iCloud Drive) without sync pathologies
- Clean ownership boundary: "if it's in the vault, it's user content" / "if it's in the data dir, it's daemon state"
- Multiple vault directories could share a daemon data-dir scheme without collision (though v0 is single-vault; see the handoff for the deferred multi-directory work)

### Negative

- Vault is not self-contained in the Git-style sense; cloning a vault does not clone its index, and re-indexing is required after a fresh checkout on another machine
- Users who want a portable "the vault *and* its index" bundle have to zip both directories together explicitly

### Neutral

- The principle generalizes to any future state: "if it mutates frequently or is device-specific, it stays out of the synced path." This is the rule, not a coincidence of one file.

---

## Notes

- This decision rules out the Git-style `<vault>/.hypomnema/` pattern even if it's aesthetically tempting
- Related to [ADR-0005](./0005-local-everything.md) — local-first cannot imply state-inside-vault; the two are independent
- The exact data-dir resolution rules (respect `XDG_DATA_HOME`, fall back to `~/.local/share/hypomnema`) are captured in `docs/reference/configuration.md`

## Amendments

- **2026-04-23** — Tightened the data-directory rule: macOS uses the same XDG-style path as Linux (`~/.local/share/hypomnema/`) rather than `~/Library/Application Support/hypomnema/`. The original decision was silent between the two on macOS; this amendment picks the Unix-uniform layout so daemon code and docs do not branch per-OS. No behavioral change for Linux or Windows; no reversal of the "state lives outside the watched vault" rule.

- **2026-04-26** — Multi-vault adopted in [ADR-0009](./0009-multi-vault-per-daemon.md) and the predicted multi-vault data-dir layout is now formalized. The Consequences section's anticipatory note ("Multiple vault directories could share a daemon data-dir scheme without collision") is realized as: `<data_dir>/vaults/<vault_id>/{index.sqlite, outbox.jsonl, meta.toml}` per vault, with a top-level `<data_dir>/vaults.sqlite` registry. The "state lives outside the watched vault" rule is unchanged and now applies per-vault.
