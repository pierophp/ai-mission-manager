# AI Mission Manager

The first end-to-end slice of a personal control plane for work delegated to people and agents. It is a Tauri 2 desktop app with a React/TypeScript view, a Rust domain core, and SQLite persistence.

## Run locally

```sh
npm install
npm run tauri -- dev
```

The app creates a local `Personal` Context with a `Default` Project on first launch. Create additional Contexts and Projects as needed; each Project can provide the default state for new Items. Capture an Item with a title, Context, and Project; it receives a stable `MC-*` identifier, appears in Inbox when its Project default is `Inbox`, and is stored in the app data directory.

## Verify

```sh
npm run typecheck
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```

The domain tests exercise Context and Project ownership, Project defaults, Item creation, isolation rejection, and requested persistence effects in memory. The persistence test closes and reopens a real SQLite database to verify the organisation hierarchy, inherited Item defaults, and global identifier sequence survive restart.
