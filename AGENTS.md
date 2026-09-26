## Agent skills

### Issue tracker

Issues live in GitHub Issues via the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

Uses the default five canonical triage labels. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context layout: root `CONTEXT.md` and `docs/adr/`. See `docs/agents/domain.md`.

## UI conventions

**No stacked forms.** Forms and multi-step flows never open in a Dialog over another Dialog or Sheet — they render inline in the surface that owns the data. A launch confirmation with configuration controls stays inline with its owning surface. Only a simple confirmation dialog for one confirm/cancel action, optionally showing what it affects, may stack.
