# AI Mission Manager

AI Mission Manager is a personal macOS control plane for work done by people and AI agents.

Work often spans a thought, a GitHub issue, several repositories, agent sessions, pull requests, and someone else's review. Each tool owns one part of that workflow, but none of them owns the relationships between the parts. AI Mission Manager keeps those relationships together so you can answer:

- Where is this work running, and on which Machine?
- Is the agent working, blocked, finished, or no longer reachable?
- What changed since I last reviewed it?
- What needs my attention now?

The project is in active v0 development and is intended for personal use on macOS.

## How it works

The central entity is an **Item**: something you have decided to do, delegate, or keep on your radar. An Item can remain a simple note, or grow into tracked external work and delegated execution:

~~~text
Item
├── Links to GitHub Issues, pull requests, or other URLs
├── Worksets containing one or more repositories
└── Runs of Claude Code or Codex inside a terminal Pane
~~~

The app also brings external changes, blocked Runs, due Reminders, and review dates into one **Needs Attention** view. An external signal is evidence for you to consider; it never silently completes work or changes your intent.

## Initial scope

The first vertical slice covers:

- **Capture and organisation** — Contexts, Projects, stable MC-* Item identifiers, Inbox/Active/Waiting/Done states, notes, relationships, and cross-Context search.
- **GitHub tracking** — paste a GitHub Issue or pull request URL, keep a cached snapshot, deduplicate External Objects, create an Issue from an editable preview, add comments, and refresh manually. Unrecognised URLs remain generic Links.
- **Attention management** — Activity history, per-Link review watermarks, Context and Link attention policies, Watch dates, Item Reminders, and grouped Attention Entries.
- **Worksets** — combine repositories under one working directory, configure branches, attach an existing directory without changing Git state, archive safely, and inspect uncommitted or unpushed work before removal.
- **Agent Runs** — launch Claude Code or Codex with an explicit Execution Profile and reviewed prompt, keep Run history, attach a manually started agent only after approval, and report unknown, working, blocked, or finished state.
- **Terminal access** — embed the real tmux Pane, send input and resize it, open the exact Pane in macOS Terminal, and recover Run associations after an app or connection restart.
- **Remote execution** — run the same terminal flow on a remote Machine over SSH, without silently falling back to local execution.

## Domain model

The project uses a small, deliberate vocabulary:

| Concept | Meaning |
| --- | --- |
| **Context** | A boundary that isolates an area of work, its providers, repositories, and Machines. |
| **Project** | A container for Items inside a Context and the source of their defaults. |
| **Item** | The user's durable unit of intent. |
| **External Object** | A provider-owned object such as a GitHub Issue or pull request. |
| **Link** | One Item's relationship with one External Object, including its own watch and review state. |
| **Workset** | A persistent working directory containing one checkout per selected Repository. |
| **Run** | One historical attempt by one agent inside a Workset. |
| **Pane** | A terminal owned by the Terminal Runtime and associated with a Run when an agent is running there. |
| **Needs Attention** | The unified view of changes and situations that currently require the user's involvement. |

See [CONTEXT.md](CONTEXT.md) for the complete glossary.

## Design principles

- **The user remains in control.** Only an explicit action completes an Item, starts or stops a Run, publishes work, or removes files.
- **Relationships are first-class.** The app connects intent, external work, repositories, execution context, and history without replacing the systems that own them.
- **No guessing.** Missing connectivity, failed fetches, and unsupported agent states preserve the last known truth or show unknown; they do not become false state changes.
- **Isolation is a domain rule.** Context boundaries apply to repositories, Machines, provider configuration, actions, and prompt content—not just to the UI.
- **The runtime is an adapter.** tmux owns sessions, Panes, PTYs, and terminal persistence. AI Mission Manager directs it rather than reimplementing it.
- **Archiving is not deletion.** Cleanup is a separate, explicit action with a safety report.

## Architecture

~~~text
React + TypeScript UI
          │ Tauri commands
          ▼
Rust application shell ───── SQLite persistence
          │
          ├── pure domain core: (state, event) → (state, effects)
          ├── GitHub adapter through the authenticated gh CLI
          └── Terminal Runtime adapter
                    └── tmux control mode → local or SSH Machine
~~~

The v0 application is one Tauri process. There is no separate daemon, custom IPC protocol, scheduler, retry loop, or supervising agent. Agent state comes from hooks installed by the app for Runs it starts; state is persisted and recovered when a terminal connection returns.

The initial provider is GitHub. The initial Terminal Runtime is tmux. Both are behind boundaries so additional providers or runtimes can be added when there is a concrete need.

## Requirements

- macOS
- Node.js and npm
- Rust and Cargo
- tmux
- GitHub CLI (gh), authenticated with gh auth login for GitHub integration
- Claude Code and/or Codex for agent Runs
- SSH access to a remote Machine for remote execution

Claude Code, Codex, and SSH are optional depending on which parts of the application you want to use; gh is needed for GitHub integration. Tauri development also requires the usual macOS developer tools; install them with xcode-select --install if they are not already present.

Check the local integrations before launching:

~~~sh
tmux -V
gh auth status
~~~

AI Mission Manager does not install or authenticate third-party tools on your behalf.

## Run locally

~~~sh
npm install
npm run tauri -- dev
~~~

On first launch, the app creates a Personal Context with a Default Project. From there, capture an Item, add external work or a Workset when needed, and start a Run explicitly.

npm run dev starts only the Vite front end. Use the Tauri command above for the complete desktop application and its Rust backend.

## Verify changes

~~~sh
npm run typecheck
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
~~~

The test suite focuses on observable behaviour in the pure domain core, with targeted persistence, provider, agent-hook, and live tmux coverage. The tmux and remote execution tests require the corresponding local tools.

## Deliberately out of scope for v0

The initial release does not include Jira, Azure DevOps, or Bitbucket integrations; a runtime other than tmux; webhooks; an autonomous scheduler, retry, or dispatch loop; mobile or multi-user collaboration; an MCP server; an embedded diff viewer; automatic branch publishing or pull-request creation; or automatic destructive cleanup.

There is no signed distribution, updater, migration guarantee, or backwards-compatibility promise yet. This is personal software optimized for a fast local development cycle.

## Project documentation

- [Initial product specification](https://github.com/pierophp/ai-mission-manager/issues/1) — problem statement, user stories, implementation decisions, and v0 boundaries.
- [CONTEXT.md](CONTEXT.md) — domain vocabulary and modelling guidance.
- [docs/adr/](docs/adr/) — architecture decisions and their consequences.
- [spikes/tmux-control-mode/](spikes/tmux-control-mode/) — disposable proofs for local/remote tmux control mode and agent lifecycle reporting.
