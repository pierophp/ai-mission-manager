# The terminal runtime is an adapter, and tmux is the first implementation

AI Mission Manager needs persistent sessions, a stream of terminal output, input, stable Pane identity, and identical behaviour on a remote Machine. Several programs provide that shape, so the runtime sits behind an adapter rather than being assumed, and tmux is what v0 implements: its control mode pushes per-Pane output, its Pane identifiers are stable and never reused, it is present everywhere, and its interface has been stable for a decade.

## Considered Options

- **Herdr, with a patched build.** Rejected, but narrowly — its agent detection is the best available anywhere and is already wired up on this machine. Its public socket API exposes no terminal-output subscription (the server computes the event internally but keeps it off the public list), so adopting it meant running a patched build on every Machine: a load-bearing fork of a fast-moving six-month-old project, plus a Linux cross-build and a permanent rebase. The argument that overturned it: this product intends to *become* the interface the user works in, and once that happens the runtime is a backend rather than a place to live — the reason to prefer Herdr disappears while the fork's cost remains.
- **Owning the PTY directly.** Rejected: persistence across app restarts would have to be rebuilt from scratch, and sessions the user started outside the app would be invisible.

## Consequences

`tmux attach` from any terminal is an escape hatch that works even when the app does not — which matters more, not less, as the app becomes the primary interface.

A Herdr adapter can be written later. The interface should be shaped against tmux honestly, with the narrowest surface that works, and generalised only when a second implementation actually exists — designing for runtime-neutrality in advance would be guessing.
