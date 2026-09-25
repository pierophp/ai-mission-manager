## Agent skills

### Issue tracker

Issues live in GitHub Issues via the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

Uses the default five canonical triage labels. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context layout: root `CONTEXT.md` and `docs/adr/`. See `docs/agents/domain.md`.

## UI conventions

**No stacked dialogs.** Forms and multi-step flows never open in a Dialog over another Dialog or Sheet — they render inline in the surface that owns the data. Only confirmation dialogs (confirm/cancel a single action, optionally showing what it affects) may stack.
