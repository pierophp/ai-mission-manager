# Local application copy

Upstream source: `mattpocock/skills`, `skills/engineering/implement/SKILL.md`.

This copy is maintained locally for the application's implementation prompts,
including direct Runs and Implementation Queue Runs. It is not synchronized
automatically; bring upstream changes over manually when desired.

## Local divergences

- Inlines the test-first loop (from upstream `tdd`) and the two-axis review
  (from upstream `code-review`) instead of invoking those skills, since only
  this `SKILL.md` reaches the run. The review fixes agreed findings instead of
  only reporting them.
