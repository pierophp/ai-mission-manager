# AI Mission Manager

The first end-to-end slice of a personal control plane for work delegated to people and agents. It is a Tauri 2 desktop app with a React/TypeScript view, a Rust domain core, and SQLite persistence.

## Run locally

```sh
npm install
npm run tauri -- dev
```

The app creates a local `Personal` Context on first launch. Capture an Item with a title and Context; it receives a stable `MC-*` identifier, appears in Inbox, and is stored in the app data directory.

## Verify

```sh
npm run typecheck
npm run build
cargo test --manifest-path src-tauri/Cargo.toml
```

The domain tests exercise Item creation and requested persistence effects in memory. The persistence test closes and reopens a real SQLite database to verify the Item and global identifier sequence survive restart.
