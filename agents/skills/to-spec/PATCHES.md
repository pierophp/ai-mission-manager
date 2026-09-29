# Patches

## Tracker from repository configuration

- **Change:** the opening paragraph reads the issue tracker and triage label vocabulary from the repository's `AGENTS.md` / `CLAUDE.md` and the docs they point to, and asks the user when none is configured, instead of pointing to `/setup-matt-pocock-skills`.
- **Why:** that setup skill does not reach the run.

## Seams decided without confirmation

- **Change:** in `Process` step 2, the agent decides the test seams itself instead of checking them with the user, and records each seam with its reason in the spec's Testing Decisions (the template's Testing Decisions list gains a seams item).
- **Why:** the run synthesizes the spec unattended; recording the seams keeps the decision visible on the published spec.
