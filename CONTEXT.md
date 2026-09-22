# AI Mission Manager

A personal control plane for work that is done by the user, delegated to AI coding agents, or merely watched. It sits above trackers, forges, Git and the terminal runtime, and its job is to retain the relationships between them.

## Language

### Organising work

**Context**:
A top-level boundary isolating one area of work, along with the providers, repositories and machines that belong to it. An Item belongs to exactly one Context.
_Avoid_: workspace, tenant, account, area

**Project**:
A container for Items inside a Context. It owns its configured Repositories and supplies the defaults Items created in it inherit. Every Item can use all Repositories configured for its Project.
_Avoid_: folder, category, epic

**Item**:
Something the user has intentionally decided to do, delegate, or keep on their radar. Everything else in the model hangs off it.
_Avoid_: task, ticket, issue, card, mission

### External work

**External Object**:
A single thing owned by an external system — a GitHub Issue, a pull request, a Jira work item. It exists once no matter how many Items refer to it.
_Avoid_: ticket, remote item, upstream object

**Link**:
The pairing of one Item with one External Object, carrying the user's own state about it: which changes they care about, and how far they have reviewed.
_Avoid_: association, reference, join

**Repository**:
A Git repository's canonical identity, registered under a Project, making it available to every Item in that Project. Not any particular checkout of it.
_Avoid_: checkout, clone, worktree

**Worktree**:
One physical Git worktree for one Repository and one Item, associated with a Machine, path, branch, and base branch.
_Avoid_: workspace, checkout

### Execution

**Terminal Runtime**:
The external program that owns terminal processes, sessions and Panes and keeps them alive between app restarts. AI Mission Manager directs one through an adapter and never reimplements it.
_Avoid_: multiplexer, terminal, tmux

**Machine**:
An execution target reachable through the Terminal Runtime, whether the local Mac or a remote host.
_Avoid_: host, server, node

**Run**:
One attempt at doing work on an Item by a single agent, using a Repository checkout configured for the Item's Project, either directly or through one physical Worktree. Runs are historical records while they are retained, but finished Runs may be explicitly deleted as part of local cleanup; an active Run blocks deletion of its Item, Machine, Project, or Context.
_Avoid_: session, job, execution, attempt

**Pane**:
A single terminal owned by the Terminal Runtime. A Pane running an agent is what a Run points at; Panes may also hold shells, tests or servers with no Run attached.
_Avoid_: terminal, tab, window

**Execution Profile**:
The kind of instruction prepared for an agent when a Run starts — investigate, implement, review, or a custom prompt.
_Avoid_: mode, template, preset

### Attention

**Needs Attention**:
The cross-cutting inbox of everything currently asking for the user's involvement. It is not an Item state; an Item in any state may appear in it.
_Avoid_: inbox, alerts, notifications

**Attention Entry**:
One unit of demand inside Needs Attention, gathering every unreviewed change from a single source into one thing to act on.
_Avoid_: notification, alert, event

**Activity**:
The full record of observed change, whether or not any of it was worth the user's attention.
_Avoid_: feed, history, log

**Watch**:
The user's intent to keep something on their radar for a period, independent of whether anything happens to it.
_Avoid_: subscription, follow, monitor

**Reminder**:
A scheduled moment at which an Item should be put back in front of the user. Set on the Item itself.
_Avoid_: alarm, due date, deadline
