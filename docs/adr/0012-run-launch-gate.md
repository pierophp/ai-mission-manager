# Gate Run launches until the Run is recorded

An agent must not execute its prompt until Mission Manager has persisted the Run and its real tmux Pane identity. Each launch therefore follows this sequence:

1. Validate the Item, Context, execution Machine, Workspace, Repository or Worktree, and the permissions approved in the preview.
2. Inspect the checkout once before launch. Direct and Grill compare branch and dirty state with the approved preview. Worktree validates the selected Worktree branch and Repository remote. Worktree has no run-specific dirty preview; its stored `is_dirty` value can be stale after user edits, so a dirty checkout does not block launch.
3. Run Machine preflight and create a tmux session whose shell waits on a channel tied to the Run id and session name.
4. Record the Run through the domain decision and persistence seam using the real Pane id. The recorded state starts as `Unknown`.
5. Release the shell with `tmux wait-for -S` only after the commit succeeds.

The checkout inspection, Machine preflight, session creation, Pane lookup, and release run outside the shared Runtime mutex. A separate async launch mutex serializes starts so concurrent commands cannot reserve the same next Run id or session name. Recording revalidates the captured application identities and approved Direct/Grill permissions against current state.

If Run recording fails, Mission Manager kills the exact gated session it created and never releases it. If release fails after recording, the caller receives an error that identifies the persisted but unreleased Run; its state remains `Unknown` and it can be stopped. If the app exits before recording, the waiting shell remains inert. Its Pane title identifies the agent so the existing untracked-agent suggestions can expose it for deletion.

This gate protects all launch paths: Direct, Grill, and Worktree. Launch errors do not change Item state, in keeping with ADR-0007.
