# The domain core is a pure function, and the only primary test seam

The core takes `(state, event)` and returns `(state, effects)`, touching no socket, disk or clock; all I/O lives in a thin shell around it. Most of the Rust here will be written by an AI rather than read line by line, so the core is shaped to be verified by behaviour instead of by reading — Item state, Attention Entries, reconciliation and Run association are all exercisable in memory.

## Consequences

Persistence, provider adapters and the terminal runtime adapter are deliberately *not* first-class test seams: they are covered by fakes at the core plus one round-trip test each, alongside a small suite against a live terminal runtime, which is where the real risk sits. This replaces the five test seams the original design proposed with two.
