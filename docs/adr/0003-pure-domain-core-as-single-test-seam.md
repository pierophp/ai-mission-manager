# The domain core is pure, modular, and the only primary test seam

The core takes `(state, event)` and returns `(state, effects)`, touching no
socket, disk, filesystem, provider, terminal runtime, Tauri state, or clock.
All I/O lives in application and infrastructure adapters around it. Item
state, Needs Attention, reconciliation, Run association, grilling, and
destructive safety decisions remain exercisable in memory.

The physical core is split into cohesive modules under `src-tauri/src/domain/`:

- `model` contains the shared domain vocabulary and `DomainState`.
- `events` contains `Event`, `Effect`, and `Decision`.
- `reducer` contains the state-transition implementation.
- `error` contains domain validation failures.
- `deletion`, `grilling`, and `projections` contain pure feature rules and
  user-facing projections.

Those modules are one conceptual core, not competing state stores or test
seams. `domain::state_transition` is the narrow primary interface exported to
feature code, while `crate::domain::*` remains a compatibility facade for
existing Rust callers. The module layout can evolve without changing the
state-transition contract.

The application layer mirrors the product vocabulary: setup, structure,
activity, and work. Work is split internally for Items and Needs Attention,
Project Repository execution and Worktree management, Run and grilling,
External Object and Link, and deletion. `app` retains only the stable Tauri
command facade and one shared `Runtime`; it does not duplicate feature
implementations. `persistence` is
similarly a facade over schema/migration, loading, effects, audit, settings,
and codec modules.

## Consequences

Persistence, provider adapters, Git/filesystem adapters, and the terminal
runtime adapter are deliberately not alternate domain seams. They are tested
at their existing adapter boundaries, with persistence round trips and the
live terminal-runtime coverage that already protects the real risks. The
primary behavioural seam remains the pure `(state, event)` transition.

The legacy Workset migration in SQLite is retained only as data compatibility
for existing personal databases; it is not an application or domain
implementation shim. This remains compatible with ADR-0008: no general schema
compatibility guarantee is introduced.

This organization does not change the surrounding decisions: the Terminal
Runtime remains an adapter with tmux as the v0 implementation (ADR-0001), v0
still has no separate daemon (ADR-0004), provider integration still uses the
official CLI boundary (ADR-0005), and agent state still comes from our hooks
and durable state files rather than terminal-output inference (ADR-0009).
