# Patches

## Tracker from repository configuration

- **Change:** the opening paragraph reads the issue tracker and triage label vocabulary from the repository's `AGENTS.md` / `CLAUDE.md` and the docs they point to, and asks the user when none is configured, instead of pointing to `/setup-matt-pocock-skills`.
- **Why:** that setup skill does not reach the run.

## Breakdown settled without confirmation

- **Change:** `Process` step 4 ("Quiz the user") becomes "Settle the breakdown": the agent checks granularity, blocking edges, and merge/split itself, revises until every check passes, then publishes; step 5 publishes the settled tickets instead of the approved ones.
- **Why:** the run publishes tickets unattended, with no user approval gate.
