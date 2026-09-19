# AI Mission Manager

The first end-to-end slice of a personal control plane for work delegated to people and agents. It is a Tauri 2 desktop app with a React/TypeScript view, a Rust domain core, and SQLite persistence.

## Run locally

```sh
npm install
npm run tauri -- dev
```

The app creates a local `Personal` Context with a `Default` Project on first launch. Create additional Contexts and Projects as needed; each Project can provide the default state for new Items. Capture an Item with a title, Context, and Project; it receives a stable `MC-*` identifier and is stored in the app data directory.

Paste a GitHub Issue or pull request URL into an Item to create a Link. GitHub data is fetched through the authenticated `gh` CLI and cached once per External Object, so multiple Items share one snapshot while keeping separate Links. Other URLs are kept as generic Links, and every snapshot shows its age.

Linked GitHub Objects are polled when the app opens and while it remains open. Changed titles, states, and metadata are recorded as Activity; each Link has its own review watermark, so shared Objects produce separate Attention Entries. Use the refresh action for an immediate poll, configure attention defaults per Context and object type, and override them on an individual Link.

The home view separates Items needing attention, running, waiting, due for a reminder, and completed. Statuses can move in any order. Each Item can carry notes and a reminder, and can be linked to other Items as `blocks`, `blocked_by`, or `related_to`. Use the Context filter to focus the home view; search always spans every Context and labels each result with its Context.

## Verify

```sh
npm run typecheck
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

The domain tests exercise Context and Project ownership, Project defaults, Item creation, status transitions, notes, reminders, relationships, search, home projections, isolation rejection, and requested persistence effects in memory. The persistence test closes and reopens a real SQLite database to verify the organisation hierarchy, inherited Item defaults, notes, reminders, relationships, and global identifier sequence survive restart.
