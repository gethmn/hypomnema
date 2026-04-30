# Decisions (ADRs)

Layer 1 of the Layered Documentation System. Architectural Decision Records capture *why* significant technical and product decisions were made. ADRs are immutable once accepted; new information appears as Amendments or in a superseding ADR.

See [`_adr-policy.md`](./_adr-policy.md) for the amend / supersede / extend policy.

## Index

| # | Title | Status |
|---|-------|--------|
| [0001](./0001-adopt-layered-documentation-system.md) | Adopt the Layered Documentation System | accepted |
| [0002](./0002-rust-over-python.md) | Rust over Python | accepted |
| [0003](./0003-indexing-in-the-daemon.md) | Indexing in the Daemon, Not in the Consumer | accepted |
| [0004](./0004-three-search-modes-as-peers.md) | Three Search Modes as Peers | accepted |
| [0005](./0005-local-everything.md) | Local Everything — No Required Cloud Dependencies | accepted |
| [0006](./0006-outbox-outside-watched-directory.md) | Daemon State Lives Outside the Watched Directory | accepted |
| [0007](./0007-sqlite-vec-over-alternatives.md) | sqlite-vec over Lance, qdrant, and Other Vector Stores | accepted |
| [0008](./0008-two-binary-daemon-plus-cli.md) | Two Binaries (hmnd + hmn) in One Crate | accepted |
| [0009](./0009-multi-vault-per-daemon.md) | Multi-Vault per Daemon | proposed |
| [0010](./0010-vault-definitions-as-runtime-state.md) | Vault Definitions Are Runtime State, Not Configuration | proposed |
| [0011](./0011-vault-management-on-hmn.md) | Vault Management Lives on `hmn` | proposed |
| [0012](./0012-mcp-transport-stdio-v0.md) | MCP transport — stdio on `hmn` in v0; socket on `hmnd` deferred | accepted |

## Creating a new ADR

1. Copy [`0000-template.md`](./0000-template.md) to `NNNN-short-title.md` (next number in sequence)
2. Fill Context, Decision, Consequences
3. Set `Status` to `proposed` until accepted
4. Add the entry to the table above
5. Cross-link from specs/architecture/implementation where the decision is referenced
