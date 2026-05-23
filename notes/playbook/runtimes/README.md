# Runtime Providers

Files under `notes/playbook/runtimes/` are **runtime providers** for the
capability contract in [`../capabilities.md`](../capabilities.md). A provider
implements zero or more capabilities by mapping them to concrete tools of one
orchestration layer. Layer A (role files) and Layer B (capability contract)
do not change when a provider is added, swapped, or composed.

## Coverage

Every provider file declares at the top which capabilities it covers, using
the verbs from `capabilities.md`:

- **Complete provider** — covers all nine capabilities. Today: `solo.md`.
- **Partial provider** — covers a subset. Today: `duo.md` (covers
  `spawn-agent-at-tier` only).

A partial provider must also declare its **dependency**: the kind of base
provider it expects to sit on top of, because uncovered capabilities are
resolved through the base. (Example: `duo.md` requires a Solo base, because
its `spawn-agent-at-tier` implementation delegates the underlying process
creation to Solo primitives.)

## Profile (base + overlays)

A session uses one **runtime profile**:

- exactly one **base** provider (must be complete), and
- zero or more **overlays** (typically partial providers), in declared order.

The default profile is `base: solo, overlays: []`.

A session declares a different profile in its bootstrap prompt or session
notes, for example:

```
runtime profile:
  base: solo
  overlays: [duo]
```

## Resolution

To resolve a capability under a profile:

1. Walk `overlays` left-to-right. The **first** overlay whose Coverage list
   includes the capability wins; use its mapping.
2. If no overlay claims the capability, fall through to the **base**
   provider's mapping.
3. If the base does not cover the capability either, the profile is
   ill-formed — fail loudly. (Bases are required to be complete to make
   this case unreachable in practice.)

## Conflict rule

At most one overlay in a profile may claim a given capability. If two
overlays both claim the same capability, the profile is ill-formed; pick one
or drop the other.

## Dependency rule

If a partial provider declares a base dependency, the chosen base must
satisfy it. For example, `duo.md` declares `requires base: solo`; a profile
with `base: solo, overlays: [duo]` is valid; a hypothetical profile with a
non-Solo base + duo overlay is not.

## Worked example: `base: solo, overlays: [duo]`

| Capability | Resolves to |
|---|---|
| `identity` | `solo.md` (Duo does not claim it) |
| `spawn-agent-at-tier` | `duo.md` (overlay claims it; supersedes Solo's § Tier Resolver) |
| `message-agent` | `solo.md` |
| `coordination/todo` | `solo.md` |
| `coordination/scratchpad` | `solo.md` |
| `coordination/kv` | `solo.md` |
| `pause-until-signal` | `solo.md` |
| `process-liveness` | `solo.md` |
| `close-process` | `solo.md` |

## Adding a new provider

1. Create `runtimes/<name>.md`.
2. Add a **Coverage** line at the top listing the capabilities claimed.
3. If partial, add a **Requires base** line stating the base it depends on.
4. For each claimed capability, add a mapping (semantics + concrete tools).
5. If a new profile is being introduced, document it here under "Worked
   examples".
