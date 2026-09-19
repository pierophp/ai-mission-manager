# Agent state comes from hooks we own, not from the terminal runtime

Claude Code and Codex can invoke a command on lifecycle events, so the agent reports its own state — working, blocked, finished — directly to AI Mission Manager, carrying the Run identity injected into its environment when the Run started. We neither infer state by parsing terminal output, which breaks whenever an agent changes its interface, nor read it from the terminal runtime.

## How the report travels

The hook writes a state file on the machine where the agent runs, then best-effort notifies the app. **The file is the truth; the notification is only latency.** Agent state is edge-triggered, so a dropped connection would otherwise lose the transition itself rather than merely delay it — with a state file, reconnecting recovers everything missed by reading it.

The notification reuses the SSH connection the app already holds for the terminal runtime, as a reverse-forwarded Unix socket whose path is injected into the Pane's environment alongside the Run identity. No daemon runs on the remote machine: a hook script is a file, not a process. Setting a tmux user option on the Pane instead, and reading it over the existing control-mode connection, would remove the tunnel entirely and is worth preferring if control mode notifies on option changes.

The app provisions the hook on a Machine the first time it is used. This is our own tooling, not a third-party dependency, so it falls under the same exception that lets the app install its own CLI.

## Consequences

This is what keeps the terminal runtime swappable (ADR-0001). Agent awareness is the one capability that differs sharply between runtimes, and sourcing it ourselves reduces the runtime to a small, portable surface: create a session, spawn a process, stream a Pane, send input, stable identity. The mechanism is also identical locally and remotely — only the socket path differs.

It creates a real asymmetry: only Runs the app starts get state, because only then does the app control the agent's environment and configuration. An agent started by hand and attached later stays unknown. Agents that cannot report state are shown as unknown rather than guessed at.
