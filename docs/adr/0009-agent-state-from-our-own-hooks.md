# Agent state comes from hooks we own, not from the terminal runtime

Claude Code and Codex can invoke a command on lifecycle events, so the agent reports its own state — working, blocked, finished — directly to AI Mission Manager, carrying the Run identity injected into its environment when the Run started. We neither infer state by parsing terminal output, which breaks whenever an agent changes its interface, nor read it from the terminal runtime.

## How the report travels

The hook writes a state file on the machine where the agent runs, then best-effort notifies the app. **The file is the truth; the notification is only latency.** Agent state is edge-triggered, so a dropped connection would otherwise lose the transition itself rather than merely delay it — with a state file, reconnecting recovers everything missed by reading the current Pane option after reconnecting.

The notification uses tmux control mode's format subscription: the hook sets the `@ai_mission_manager_run_state` user option on the exact Pane, and the app subscribes with `refresh-client -B`. tmux 3.7c emits `%subscription-changed` when that option changes, at most once per second, so the report travels over the SSH connection the app already holds. The option contains the provider, Run identity, state and timestamp. No reverse-forwarded socket is needed, and no daemon runs on the remote Machine: a hook script is a file, and each invocation is short-lived.

The spike's executable proof is in `spikes/tmux-control-mode/src/agent-state.ts`. Claude Code maps `SessionStart`/`UserPromptSubmit` to working, permission or elicitation notifications to blocked, and `Stop`/`SessionEnd` to finished. Codex maps `SessionStart`/`UserPromptSubmit` to working, `PermissionRequest` to blocked, and `Stop`/`SessionEnd` to finished. Codex has no equivalent general notification signal in this mapping, so other waiting states remain unknown. The hook input contracts are documented by [Claude Code](https://code.claude.com/docs/en/hooks) and [Codex](https://developers.openai.com/es-419/docs/hooks).

The app provisions the hook on a Machine the first time it is used. This is our own tooling, not a third-party dependency, so it falls under the same exception that lets the app install its own CLI.

The production hook entrypoint is the application binary itself (`--agent-state-hook <claude|codex>`), so a Run does not depend on a separate Node installation or a long-running helper. Run state is stored in the domain and SQLite as `unknown`, `working`, `blocked`, or `finished`; the app reconciles the durable state file when it opens and whenever it reconnects to a Run Pane. A blocked Run is projected into Needs Attention, while all Run-state transitions leave Item state unchanged.

## Consequences

This is what keeps the terminal runtime swappable (ADR-0001). Agent awareness is the one capability that differs sharply between runtimes, and sourcing it ourselves reduces the runtime to a small, portable surface: create a session, spawn a process, stream a Pane, send input, stable identity. The mechanism is identical locally and remotely because the hook talks to the Machine's tmux server and the app receives the option change through its existing runtime connection.

It creates a real asymmetry: only Runs the app starts get state, because only then does the app control the agent's environment and configuration. An agent started by hand and attached later stays unknown. Agents that cannot report state are shown as unknown rather than guessed at.
