# Projects own Repositories used by all their Items

A Project already groups Items and supplies their defaults, so Repository selection does not need a separate per-Item Workspace configuration. Register each Repository under a Project in Settings; every Item in that Project can use the complete configured set. Worktrees remain isolated by Item, Repository, Machine, and path, with branch defaults derived from the Item and the Repository's configured base branch.

Existing persisted Workspace rows remain hidden execution references for Run and Worktree records. The application creates them automatically, mirrors the Project's Repository set into them, and exposes no user flow for creating or removing them. This preserves existing local execution history while Project is the only user-configured Repository boundary. If a future need requires selecting only part of a Project's Repositories per Item, that policy should be introduced explicitly at the Item level.
