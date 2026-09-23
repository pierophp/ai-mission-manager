# Each Context has one execution Machine

Runs and Worktrees in a Context use its single configured execution Machine, so Items cannot override the target and an unconfigured Context cannot start execution. Changing the configured Machine does not migrate or replace existing Worktrees; they remain on their original Machine until explicitly removed and recreated. Deleting a Machine removes its local Run and execution metadata after best-effort stopping its Runs, while leaving physical checkout and Worktree files on the Machine untouched.
