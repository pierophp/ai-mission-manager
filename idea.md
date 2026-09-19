# AI Mission Manager — v0 Technical Product Spec

## 1. Summary

**AI Mission Manager** is a macOS-first personal control plane for managing work that is performed by humans, AI coding agents, or external collaborators.

It unifies:

* local tasks;
* Jira Cloud work items;
* Azure DevOps work items;
* GitHub Issues;
* GitHub pull requests;
* Azure Repos pull requests;
* Bitbucket Cloud pull requests;
* reminders and temporary watches;
* Claude Code and Codex executions;
* local and remote Herdr environments;
* multi-repository working directories;
* branches, commits, pushes, and PR publication.

The central abstraction is not a ticket and not an agent.

It is an **Item**: something the user has intentionally decided to do, delegate, or keep on their radar.

An Item may start as a local note with no external ticket, later acquire a Jira/Azure/GitHub link, spawn several agent Runs, produce multiple PRs across multiple repositories, and remain tracked until the user manually decides it is complete.

AI Mission Manager does not replace Jira, Azure DevOps, GitHub, Bitbucket, Git, or Herdr.

It sits above them.

```text
                     AI Mission Manager
                            │
              ┌─────────────┼─────────────┐
              │             │             │
           Work         Execution       Follow-up
              │             │             │
      Jira / Azure /    Herdr + AI    PRs / reminders
      GitHub / local      agents        / watches
```

The first version is a **single-user macOS application**.

---

# 2. Product principle

The product answers one primary question:

> What am I currently doing or waiting on, and what needs my attention now?

The application is not intended to be another full backlog manager.

Jira, Azure DevOps, GitHub, and Bitbucket remain the authoritative external systems for the objects they own.

AI Mission Manager maintains a personal operational view across all of them.

The desired workflow is:

```text
capture
   ↓
organize
   ↓
delegate / work / watch
   ↓
observe
   ↓
intervene when necessary
   ↓
publish / follow up
   ↓
manually complete
```

A major product requirement is that the user no longer needs to remember:

> Which terminal, machine, branch, worktree, agent, ticket, or PR was I working in?

That association belongs to AI Mission Manager.

---

# 3. Testing seams

The v0 should be testable primarily at a small number of high-level seams.

## 3.1 Core domain seam

Given persisted domain state and an incoming event/action, the core determines:

* Item state;
* Needs Attention state;
* reminder state;
* Run association;
* Workset association;
* external-object association;
* audit events;
* actions that should be offered or rejected.

The domain must not depend directly on Tauri, xterm.js, specific CLIs, or Herdr transport details.

---

## 3.2 Provider adapter seam

Each external provider is accessed through a normalized adapter.

Given a provider configuration and request such as:

```text
get object
list suggestions
create issue
add comment
transition status
list PR state
create PR
```

the adapter:

1. calls the configured locally installed CLI;
2. parses its output;
3. returns normalized domain data or a typed operational error.

Provider adapters must be independently testable without the UI.

---

## 3.3 Herdr execution seam

The Mission domain interacts with Herdr through one execution interface covering:

* list machines;
* list workspaces;
* list panes;
* detect agents;
* create/focus workspace;
* start Claude/Codex;
* stream a terminal pane;
* send terminal input;
* reconcile existing panes;
* associate existing sessions.

Whether the target is local or remote must not leak into the Item/Run domain.

---

## 3.4 Persistence seam

SQLite persistence should be testable with real migrations and temporary databases.

Important cases include:

* restart and reconciliation;
* duplicate external links;
* multiple Items linking the same external object;
* multiple Worksets per Item;
* several Runs per Workset;
* audit history;
* reminders;
* stale external snapshots.

---

## 3.5 End-to-end acceptance seam

The highest-value acceptance seam is:

> From a tracked Item, the user can get to the exact execution context, interact with its real Herdr terminal, restart AI Mission Manager, and return to the same tracked relationship without creating duplicate work.

The local and remote versions of this flow are both required.

---

# 4. Goals

The v0 must make the following workflows possible.

## 4.1 Capture work without requiring a ticket

The user can create an Item using only:

* title;
* Context.

Everything else is optional initially.

Example:

```text
Investigate slow invoice import
Context: Work A
```

The Item may remain local forever.

---

## 4.2 Convert local work into externally tracked work

A local Item can later:

* create a Jira Cloud ticket;
* create an Azure DevOps work item;
* create a GitHub Issue;
* link an already existing external object.

The Item remains the same Item after that operation.

Its history is preserved.

---

## 4.3 Track external objects without performing implementation work

The user can track:

* PRs;
* tickets;
* issues;
* other supported external objects;

without starting an agent or cloning a repository.

Example:

> Track this Bitbucket PR for two weeks and put it back in front of me on Friday.

---

## 4.4 Delegate implementation or investigation to an AI agent

From an Item, the user can start:

* Claude Code;
* Codex;

on:

* the local Mac;
* a saved remote Herdr machine.

The execution environment may contain several repositories.

---

## 4.5 See everything currently requiring attention

The home view must surface:

* agents needing intervention;
* review requests;
* reminders;
* watched external objects with important changes;
* items requiring manual reevaluation.

---

## 4.6 Preserve execution context

The system must retain associations between:

```text
Item
→ Workset
→ Herdr Workspace
→ Run
→ Herdr Pane
→ Agent
```

across Mission Control restarts and Mac reboots whenever the underlying Herdr resources still exist.

---

# 5. Non-goals for v0

The following are explicitly out of scope.

## 5.1 No autonomous scheduler

AI Mission Manager must not automatically:

* start the next task;
* retry a failed Run;
* switch from Claude to Codex;
* dispatch work because capacity became available.

Every new Run begins through an explicit user action.

---

## 5.2 No manager agent

There is no supervisory AI that autonomously assigns work to other agents.

---

## 5.3 No MCP integration

MCP may be added later.

The v0 exposes its own CLI but no MCP server.

---

## 5.4 No mobile client

The architecture should keep transport separate enough to allow a mobile client later.

The v0 exposes no network API for this purpose.

---

## 5.5 No multi-user collaboration

The v0 is one user per installation.

There are no:

* organizations;
* shared queues;
* permissions;
* team assignments;
* Mission Control accounts.

---

## 5.6 No webhooks

External providers are polled.

No public callback infrastructure is required.

---

## 5.7 No automatic Markdown task synchronization

The v0 does not:

* watch Markdown directories;
* import generated Markdown ticket files automatically;
* treat Markdown files as live task sources;
* extract DAG relationships from Markdown.

Existing spec/ticket-generation skills remain external to Mission Control.

---

## 5.8 No dedicated Spec entity

A spec is a link/reference attached to an Item.

There is no separate `Spec` aggregate in v0.

---

## 5.9 No embedded diff viewer

PR metadata and status are visible.

Detailed code review happens in the provider or existing development tools.

---

## 5.10 No reimplementation of Herdr

AI Mission Manager does not implement:

* PTY ownership;
* terminal persistence;
* terminal multiplexing;
* splits;
* full workspace/tab management.

Herdr remains the terminal runtime.

---

## 5.11 No automatic destructive cleanup

Completing or archiving work never automatically:

* kills agents;
* deletes branches;
* removes worktrees;
* deletes checkouts;
* closes Herdr panes;
* discards uncommitted work.

---

# 6. Core terminology

## 6.1 Context

A top-level isolation boundary.

Examples:

```text
Work A
Work B
Personal
```

A Context determines which:

* providers;
* projects;
* Herdr machines;
* repositories;
* execution profiles;

belong to that area of work.

An Item belongs to exactly one Context.

Cross-context agent operations are prohibited.

Search may cross Contexts.

---

## 6.2 Project

A lightweight organizational container inside a Context.

```text
Context
└── Project
    └── Item
```

Every Item belongs to exactly one Project.

If the Context does not need project subdivision, it receives a `Default` Project.

Projects may provide defaults for:

* repositories;
* external tracker;
* execution machine;
* agent profile.

---

## 6.3 Item

The primary Mission Control entity.

An Item is something intentionally being:

* done;
* delegated;
* investigated;
* reviewed;
* waited on;
* watched.

Example:

```text
MC-42
Investigate webhook retry failures
```

An Item may have:

* no external links;
* one external link;
* several external links;
* no Workset;
* multiple Worksets;
* zero or many Runs;
* reminders;
* dependencies;
* notes;
* watch configuration.

---

## 6.4 External Object

A normalized reference to something owned by an external system.

Supported v0 object classes include:

```text
Jira issue
Azure DevOps work item
GitHub Issue
GitHub PR
Azure Repos PR
Bitbucket Cloud PR
generic URL
```

An External Object is unique in the database but may be linked to several Items.

Example:

```text
GitHub PR #381
     ├── MC-42
     └── MC-58
```

Polling occurs once per External Object.

---

## 6.5 Repository

A Git repository registered under a Project.

The repository record represents the canonical repository identity/configuration, not a specific checkout.

---

## 6.6 Workset

A persistent multi-repository execution environment representing one logical line of work.

Example:

```text
~/worktrees/work-b/PLAT-847/
├── service-a/
├── service-b/
└── frontend/
```

Each directory is an independent Git checkout/worktree.

The Workset groups them.

A Workset has:

* Context;
* Project;
* logical branch;
* filesystem root;
* selected repositories;
* optional per-repository branch overrides;
* optional per-repository base branch overrides;
* preferred Herdr Workspace association.

Workset lifecycle is independent from Run lifecycle.

---

## 6.7 Run

A Run is one execution attempt/session associated with a Workset.

Examples:

```text
Run #1 — Claude — investigation
Run #2 — Codex — implementation
Run #3 — Claude — review
```

A Run records:

* Workset;
* machine;
* Herdr Workspace;
* Herdr pane;
* agent kind;
* execution profile;
* timestamps;
* observed status.

Runs are historical.

Changing agents does not require creating a new Workset.

---

## 6.8 Pane

A Herdr terminal pane.

A Pane may contain:

* Claude;
* Codex;
* shell;
* dev server;
* tests;
* log tail;
* another non-agent command.

Agent panes have additional Run semantics.

Non-agent panes can still appear in the embedded terminal tabs.

---

## 6.9 Reminder

A scheduled local reminder attached to an Item.

An Item may have multiple reminders.

---

## 6.10 Watch

A policy controlling how an Item or linked External Object remains on the user's radar.

Important independent timestamps:

```text
watch_until
review_at
```

`watch_until` defines the intended monitoring period.

`review_at` means the user wants the Item surfaced on that date regardless of external activity.

Neither automatically completes an Item.

---

# 7. Item state model

Mission Control has its own deliberately simple Item state.

```text
Inbox
Active
Waiting
Done
```

These states are manually controlled.

There is no requirement that they progress linearly.

External tracker states do not automatically change Item state.

Agent state does not automatically change Item state.

---

# 8. Operational signals

Operational state is separate from Item state.

Examples include:

```text
Agent working
Agent blocked
Agent finished
Run failed
Machine offline
PR updated
Review requested
CI failed
Reminder due
Watch expired
Provider unavailable
```

An Item may be:

```text
Waiting
```

while simultaneously having:

```text
PR updated
Needs Attention
```

---

# 9. Needs Attention

`Needs Attention` is a cross-cutting inbox, not an Item state.

Examples that may generate attention:

* agent requires user intervention;
* direct mention;
* review requested;
* reminder reached;
* watch review date reached;
* relevant CI failure;
* important PR transition;
* manual review of an agent result is required.

All completion remains manual.

---

# 10. Activity versus attention

Every supported external change may be recorded in Activity.

Only selected changes generate Needs Attention.

Example:

```text
PR #381
7 changes since last review

- 4 commits
- 2 comments
- CI is now passing
```

This is presented as one attention unit rather than seven separate notifications.

Opening an Item does not automatically mark these updates as reviewed.

The user explicitly chooses:

```text
Mark reviewed
```

---

# 11. Watch policy

An Item or monitored External Object may have a simple configurable policy.

Example:

```text
Notify me about:

[x] direct mentions
[x] review requests
[x] comments
[ ] commits
[x] CI failures
[x] merged / closed
```

Defaults may be configured per Context and object type.

A general rules DSL is out of scope.

---

# 12. Snooze

Attention units can be snoozed independently from Item state.

Quick options:

```text
1 hour
Tomorrow
Next week
Pick date
```

Snoozing:

* does not modify the external object;
* does not change an agent's actual state;
* does not change Item state.

It only determines when the attention unit is surfaced again.

---

# 13. Local dependencies

Items may express local relationships:

```text
blocks
blocked_by
related_to
```

These relationships are maintained by Mission Control.

They are particularly useful when the external tracker does not represent the user's internal execution breakdown.

External dependency synchronization is not automatic.

---

# 14. External providers

The v0 supports:

## Tracker / issue providers

* Jira Cloud;
* Azure DevOps Cloud;
* GitHub.com Issues.

## Pull request providers

* GitHub.com;
* Azure Repos;
* Bitbucket Cloud.

Self-hosted Jira, Azure DevOps Server, GitHub Enterprise Server, and Bitbucket Data Center are not required for v0.

---

# 15. Provider integration strategy

Mission Control does not implement authentication flows in v0.

It uses locally installed and already authenticated provider CLIs.

Example:

```text
GitHub → gh
Azure → configured Azure CLI tooling
Jira → configured Jira CLI
Bitbucket → configured Bitbucket CLI
```

The exact executable path is configurable.

Mission Control:

1. detects whether the required executable exists;
2. checks supported version/capabilities;
3. checks whether the CLI can perform the required action;
4. instructs the user how to fix missing setup;
5. never installs or logs in to third-party CLIs automatically.

---

# 16. Provider adapter interface

The implementation should expose a provider-neutral internal interface.

Conceptually:

```ts
interface ProviderAdapter {
  detect(): Promise<DetectionResult>;
  version(): Promise<VersionResult>;
  healthcheck(): Promise<HealthResult>;
  capabilities(): Promise<CapabilitySet>;

  fetchExternalObject(ref: ExternalRef): Promise<ExternalObjectSnapshot>;
  listSuggestions(scope: SuggestionScope): Promise<Suggestion[]>;
  createObject(input: CreateExternalObjectInput): Promise<ExternalObject>;
  addComment(input: AddCommentInput): Promise<void>;
}
```

Tracker adapters may additionally support:

```ts
listTransitions()
transition()
```

PR-capable adapters may additionally support:

```ts
findPullRequestForBranch()
createPullRequest()
```

The interface names are illustrative; behavior is the requirement.

---

# 17. Provider CLI environment

Because `missiond` runs through macOS `launchd`, it must not assume the same interactive shell environment as a terminal.

Mission Control should discover and persist absolute executable paths.

Example:

```text
/opt/homebrew/bin/gh
```

Provider configuration may specify required environment values explicitly.

A command working from Ghostty but failing from `missiond` due solely to a missing interactive `PATH` is considered an integration defect.

---

# 18. Provider failure behavior

Provider unavailability must degrade gracefully.

Example:

```text
GitHub
✓ healthy

Jira
⚠ authentication required

Bitbucket
⚠ CLI unavailable
```

Other providers continue operating.

A parse or provider failure must not cause the last known external object state to be replaced with an invalid empty state.

Offline snapshots display age:

```text
Last sync: 18 minutes ago
Provider unavailable
```

---

# 19. Polling

The v0 uses polling.

No public webhook receiver is required.

Conceptual sources:

```text
Herdr local       event-driven where practical
Herdr remote      bridge / reconciliation
Jira              polling
Azure DevOps      polling
GitHub            polling
Bitbucket         polling
```

Polling frequency may be adaptive.

Active/recent Items may be checked more often than long-term Watches.

A manual refresh must be available.

---

# 20. Suggestions

Mission Control should not automatically import the entire user's Ji35;112;17Mra/Azure/GitHub workload.

Instead it exposes Suggestions such as:

```text
Assigned to me
Review requested
Recently updated
```

The user explicitly chooses:

```text
Track
```

before the object becomes part of the main operational radar.

---

# 21. Generic URL ingestion

The user can paste a URL into a universal link field.

Supported URLs should be classified when possible as:

* GitHub PR;
* GitHub Issue;
* Jira ticket;
* Azure work item;
* Azure PR;
* Bitbucket PR.

Recognized objects are fetched through their provider adapter.

Unknown URLs are stored as generic links rather than rejected.

---

# 22. External object deduplication

The same provider object must be represented once internally.

Identity should be based on provider-native stable identity rather than title.

Several Items may link to the same External Object.

Polling and event normalization occur once.

Global notifications for the same external event should not duplicate simply because several Items reference the object.

---

# 23. Creating external tickets

An Item may create:

* Jira issue;
* Azure DevOps work item;
* GitHub Issue.

Before publication, the user receives an editable preview containing appropriate fields.

Creation is explicit.

After creation, the returned External Object is linked to the existing Item.

The Item is not replaced.

---

# 24. Editing external systems

The v0 supports selected explicit writes.

Supported behaviors should include:

* create issue/work item;
* add comment;
* list tracker transitions;
* exp35;111;17Mlicitly apply a Jira/Azure state transition;
* push Git branches;
* create PRs.

The v0 is not a full editor for all Jira/Azure/GitHub/Bitbucket metadata.

Complex edits open the original provider.

---

# 25. Worksets

## 25.1 Multi-repository model

One Workset may contain several repositories simultaneously.

Example:

```text
~/worktrees/work-b/PLAT-847/
├── api/
├── worker/
└── frontend/
```

The Workset root is a first-class path.

Agents normally start at this root so they can navigate all included repositories.

---

## 25.2 Default Workset path

Default structure:

```text
~/worktrees/<context>/<branch-slug>/
```

A Project component may be inserted when useful.

Example:

```text
~/worktrees/work-b/platform/PLAT-847/
```

The physical directory nam35;110;17M35;109;17Me and logical Git branch are separate concepts.

A branch such as:

```text
feature/PLAT-847-retry
```

does not have to become a nested filesystem path.

---

## 25.3 Branch model

A Workset has a default logical branch.

Individual repositories may override:

* branch name;
* base branch.

Example:

```text
Workset branch:
feature/PLAT-847

service-a:
branch feature/PLAT-847
base main

service-b:
branch feature/PLAT-847
base develop
```

---

## 25.4 Repository selection

Projects know their available repositories.

Creating a Workset allows selecting the subset required.

Example:

```text
[x] service-a
[x] service-b
[ ] admin
[ ] mobile
```

Repositories may be added later.

---

## 25.5 Existing Worksets

The v0 must support attaching an existing direct35;108;18Mory.

Example:

```text
Attach Existing Workset
~/worktrees/PLAT-847
```

Mission Control inspects child Git repositories and their current state.

Attaching must not:

* switch branches;
* reset repositories;
* move directories;
* modify Git configuration.

---

# 26. Workset lifecycle

Worksets persist across multiple Runs.

Example:

```text
Item
└── Workset
    ├── Run #1 Claude investigation
    ├── Run #2 Codex implementation
    ├── Run #3 Claude review
    └── Run #4 Codex fix
```

An Item may have several Worksets for alternative implementations.

---

# 27. Concurrent Runs

Mission Control does not enforce repository write locks.

Several agents may operate within the same Workset, including against overlapping repositories.

The application may di35;106;18Msplay that multiple active Runs share a Workset, but the user remains responsible for managing concurrency.

Mission Control must not refuse a Run solely because another Run is working in the same repository.

---

# 28. Git behavior

Agents may:

* inspect Git;
* edit files;
* run tests;
* create commits.

Push and PR creation remain explicit user-triggered operations in v0.

---

# 29. Publish Workset

Mission Control may provide a publication workflow.

Example:

```text
Publish Workset

service-a
3 commits ahead
[x] push
[x] create PR

service-b
1 commit ahead
[x] push
[x] create PR

frontend
clean
```

The user previews and confirms the operation.

Publication uses installed provider/Git tooling.

If Mission Control creates a PR itself, it may immediate35;105;18M35;104;18Mly link the returned PR External Object because provenance is unambiguous.

For PRs merely discovered from matching branch names, the system offers:

```text
Link PR
```

rather than silently linking.

---

# 30. Workset archive and cleanup

`Archive Workset` only changes Mission Control visibility/state.

It does not remove checkouts.

Physical removal is a separate explicit operation.

Before removal, Mission Control must report relevant safety conditions such as:

```text
service-a   clean
service-b   2 unpushed commits
frontend    uncommitted changes
```

Destructive cleanup requires confirmation.

---

# 31. Herdr role

Herdr is the execution runtime.

Herdr owns:

* terminal processes;
* PTYs;
* sessions;
* panes;
* workspaces;
* terminal persistence;
35;102;18M* agent detection/state where available.

AI Mission Manager owns:

* Items;
* Contexts;
* Projects;
* Worksets;
* Runs;
* tracker associations;
* PR associations;
* reminders;
* watches;
* attention;
* audit history.

---

# 32. Herdr machines

The source of execution machines is Herdr.

Mission Control does not maintain an independent SSH host registry.

The available execution targets come from Herdr's machine model/list.

A Context may restrict which Herdr machines are appropriate for that work.

---

# 33. Local and remote execution

Example configuration:

```text
Work A
Azure DevOps
Execution → local Mac

Work B
Jira
Execution → Linux VM via Herdr machine
```

The domain treats both as execution targets.

No silent fallback is permitted.

If the inten35;101;18Mded remote machine is unavailable:

```text
Run cannot start
Remote machine unavailable
```

Mission Control must not execute the work locally instead.

---

# 34. Workset ↔ Herdr Workspace

A Workset should preferably correspond to one Herdr Workspace.

```text
Mission Workset
       ↕
Herdr Workspace
```

Inside it may exist:

```text
Claude
Codex
Tests
Server
Logs
Shell
```

The relationship should use stable Herdr identifiers, not just visible names.

---

# 35. Run ↔ Herdr Pane

A Run stores the stable identity required to find its actual Herdr execution.

Conceptually:

```text
Run
├── herdr_machine_id
├── herdr_workspace_id
├── herdr_pane_id
└── agent_kind
```

Human-readable names remain UI metadata.

When possible, Mission Control should also store 35;100;18M35;99;18Mits Item/Run identifiers in Herdr metadata to improve reconciliation.

---

# 36. Reconciliation

On startup or reconnection, `missiond` must:

1. load persisted Mission state;
2. inspect known Herdr machines;
3. inspect relevant workspaces/panes;
4. reconnect known Run associations;
5. mark missing or changed resources;
6. refresh external providers.

The system must never create a replacement Run automatically merely because a persisted pane cannot be found.

Example:

```text
Run #4
⚠ Herdr pane missing
```

The user chooses what happens next.

---

# 37. Untracked Herdr sessions

If Mission Control detects an agent started manually under a known Workset, it may suggest association.

Example:

```text
Untracked agent detected

Claude
~/worktrees/work-b/PL35;98;18MAT-847

Likely Workset: PLAT-847

[Attach]
[Ignore]
```

It does not attach automatically.

---

# 38. Agent support

The v0 provides semantic support for:

* Claude Code;
* Codex.

Other Herdr panes/processes may still be displayed as terminals but are not guaranteed to receive agent-specific status interpretation or automation.

---

# 39. Execution profiles

At minimum, v0 supports configurable conceptual profiles:

```text
Investigate
Implement
Review
Custom prompt
```

The profile determines what kind of instruction is prepared.

Agent installation/authentication remains on the execution machine.

Mission Control does not move credentials between machines.

---

# 40. Delegation

Delegating an Item requires explicit user confirmation.

The delegation UI35;97;18M surfaces:

* Context;
* Project;
* Workset;
* machine;
* selected agent;
* execution profile;
* working directory;
* included context/prompt preview.

No execution starts automatically from dependencies becoming ready.

---

# 41. Prompt/context composition

Before starting an agent, Mission Control prepares an editable preview.

Potential sources include:

* Item objective;
* local notes explicitly selected;
* external tickets explicitly selected;
* external PR references explicitly selected;
* profile-specific instructions.

Content is not included merely because it is linked.

No context from another Context is automatically included.

The user can modify the prompt before launch.

---

# 42. Embedded terminal

The Mission Control UI includes an interactive terminal attached to the real Herdr pane.

It does not create a separate PTY.

Conceptual flow:

```text
Herdr pane
   ↓
terminal stream
   ↓
missiond / native bridge
   ↓
Tauri
   ↓
xterm.js
```

Keyboard input flows back to the same pane.

The terminal must support at least the interaction necessary for:

* normal agent conversation;
* interactive prompts;
* Ctrl-key operations;
* terminal resizing.

---

# 43. Embedded terminal UI

An Item/Workset may expose several panes as simple terminal tabs.

Example:

```text
[Claude] [Codex] [Server] [Tests]
```

Only one pane needs to be rendered actively at a time in v0.

Mission Control does not recreate Herdr splits or full workspace layouts.

---

# 44. Remote terminal streaming

A remote Workset must support the same embedded terminal experience.

The implementation may transport the Herdr terminal session through SSH or another Herdr-compatible bridge.

This is a required technical spike.

The acceptance criterion is behavior, not a predetermined transport implementation:

> A pane running on the configured Linux Herdr machine can be interacted with from the embedded xterm terminal on the Mac without creating a second agent/session.

---

# 45. Open Full Herdr

Every tracked Run should offer:

```text
Open Full Herdr
```

The intent is to reach the exact relevant execution context, not merely open the Herdr application generically.

The target resolution uses:

```text
Run
→ machine
→ workspace
→ pane
```

This local and remote deep-link/focus behavior is a required integration test.

If the exact target no longer exists, the application must report that instead of focusing a vaguely similar pane.

---

# 46. Architecture

The v0 architecture is:

```text
                 AI Mission Manager.app
                 Tauri 2 + React + TS
                          │
                   Unix domain socket
                          │
                      missiond
                        Rust
                          │
       ┌──────────────────┼──────────────────┐
       │                  │                  │
     Herdr             Providers           SQLite
       │                  │
   local/remote       local CLIs
```

---

# 47. Desktop application

Recommended stack:

```text
Tauri 2
React
TypeScript
xterm.js
```

The desktop application is a client of `missiond`.

Business logic should not be duplicated in the UI.

---

# 48. missiond

`missiond` is a Rust daemon.

Responsibilities include:

* domain state;
* persistence;
* provider polling;
* Herdr reconciliation;
* reminders;
* notification decisions;
* CLI requests;
* terminal bridging;
* audit events;
* health checks.

It runs independently of the desktop UI.

---

# 49. Daemon lifecycle

`missiond` runs as a user-level macOS `LaunchAgent`.

Expected lifecycle:

```text
macOS login
   ↓
missiond
   ↓
polling / reminders / reconciliation

Mission Control.app
   ↓
connects to missiond
```

Restarting macOS must not cause automatic agent execution.

---

# 50. Local transport

The v0 uses a Unix domain socket accessible only to the current user.

Conceptually:

```text
~/Library/Application Support/AI Mission Manager/mission.sock
```

Both:

```text
AI Mission Manager.app
mission CLI
```

connect to this endpoint.

No HTTP port is exposed by default.

The domain/API boundary should remain transport-independent enough to permit another transport later.

---

# 51. mission CLI

The v0 ships its own CLI:

```text
mission
```

The app can install/link it for the user.

It connects to the same `missiond`.

Representative capabilities:

```bash
mission item create "Investigate timeout"
mission item open MC-42
mission link add MC-42 <url>
mission run attach MC-42 <herdr-target>
mission watch MC-42 --until <date>
```

Exact command syntax may evolve, but CLI access to core Item/link/watch/Run workflows is part of v0.

MCP is not.

---

# 52. Persistence

The v0 uses one SQLite database.

All Context-owned records include a required Context identity.

Reasons for one database:

* global search;
* unified Today view;
* unified Needs Attention;
* shared external object deduplication;
* simpler local persistence.

Cross-context agent actions are prevented at the domain layer.

---

# 53. Suggested persistence aggregates

The exact migration/table design may be chosen during implementation, but the persistent model must represent at least:

```text
contexts
projects
items
item_relationships

external_providers
external_objects
item_external_links
external_snapshots

repositories
worksets
workset_repositories

runs
herdr_targets
pane_associations

reminders
watch_policies
attention_entries

activity_events
audit_events

adapter_health
settings
```

Implementation may combine or split these tables.

Behavioral semantics are more important than matching these exact table names.

---

# 54. Audit log

The v0 maintains a lightweight append-only audit history.

Examples:

```text
Started Run #4
Attached existing Workset
Linked PR #381
Moved Item to Waiting
Created Jira ticket
Published service-a branch
Associated Herdr pane
```

Audit history does not automatically record:

* terminal keystrokes;
* full terminal output;
* prompts;
* source code.

---

# 55. Technical logs

Technical operational logs are separate from the Item audit history.

They may contain:

* adapter invocation metadata;
* provider errors;
* parsing failures;
* Herdr connection failures;
* SSH failures;
* reconciliation diagnostics.

Technical logs:

* rotate;
* avoid sensitive full payloads by default;
* support a temporary debug mode when needed.

---

# 56. Health and diagnostics

The v0 includes a System/Health screen.

Example:

```text
missiond          ✓
Herdr local       ✓
Linux VM          ✓
GitHub CLI        ✓
Jira CLI          ✓
Azure CLI         ✓
Bitbucket CLI     ⚠ authentication required
```

Actions include:

```text
Retry
Copy diagnostics
Open logs
```

Adapters expose enough detection/version/capability information to support this view.

---

# 57. Offline behavior

The UI remains useful when external systems are temporarily unavailable.

Last known snapshots remain visible with timestamps.

Example:

```text
PLAT-847
In Progress

Last sync: 18m ago
Jira unavailable
```

Lack of provider response never implies deletion, closure, or completion.

Remote Runs may show:

```text
Machine offline
Last observed: working
```

The system does not assume that the agent failed or finished.

---

# 58. Home view

The default app screen is the unified operational dashboard.

Primary sections:

```text
Needs Attention
Running
Waiting
Due / Follow up
```

Quick Context filters:

```text
All
Work A
Work B
Personal
```

Secondary navigation exposes:

* Contexts;
* Projects;
* Items;
* Activity;
* System/Health.

---

# 59. Item quick capture

`⌘N` opens minimal Item capture.

Required:

```text
Title
Context
```

Project defaults to the current/default Project.

Other fields can be filled later.

---

# 60. Global Quick Capture

The v0 includes a global macOS Quick Capture mechanism that can be invoked even if the main app window is closed.

Because `missiond` already runs independently, capture can persist immediately.

The UI should remain deliberately minimal.

---

# 61. Command Palette

`⌘K` is a first-class navigation/action surface.

Representative queries/actions:

```text
PLAT-847
needs attention
create task
delegate
open terminal
open full herdr
mark waiting
remind tomorrow
add github pr
```

Keyboard access is a core product characteristic, not an optional accessibility enhancement.

---

# 62. Search

Global search may cross Contexts.

Results must display Context clearly.

An action that invokes an agent remains scoped to a single Context.

Search never causes content from multiple Contexts to be combined automatically into an AI prompt.

---

# 63. Menu bar

The macOS app exposes a lightweight menu bar presence.

Example:

```text
AI Mission Manager

2 need attention
3 agents running

Open AI Mission Manager
Quick Capture
Pause notifications
```

The full product does not need to be reproduced in the menu bar.

---

# 64. Notifications

Native macOS notifications are used primarily for Needs Attention.

They are configurable per Context.

Default notification content should be discreet and avoid displaying sensitive source code or terminal content.

Repeated notifications for the same unresolved condition should be deduplicated.

---

# 65. Item completion

Item completion is always manual.

No external signal automatically marks an Item Done.

This includes:

* PR merged;
* ticket closed;
* CI passing;
* agent finished;
* agent reports success.

These are evidence that may inform the user's decision.

They are not completion decisions.

---

# 66. Run completion versus Item completion

A Run may become:

```text
finished
failed
missing
stopped
```

without changing Item state.

A successful Run normally results in something that needs user review.

---

# 67. Completing while processes are active

If the user marks an Item Done while associated Runs remain active, the UI warns the user.

The completion action itself still does not terminate them.

Process termination is a separate explicit command.

---

# 68. Bulk actions

The v0 supports bulk local operations such as:

* snooze;
* move Item state;
* move Project;
* add reminder;
* archive.

Potentially destructive or external operations require stronger preview/confirmation.

Examples:

* start several agents;
* create several tickets;
* push several repos;
* create several PRs;
* apply several tracker transitions.

---

# 69. Setup

First-run setup uses a simple wizard.

Example:

```text
Create Context
→ Work A

Tracker
→ Azure DevOps

Execution
→ Local Herdr

Project
→ Default

Dependencies
✓ herdr
✓ provider CLI
✓ gh
```

Configuration remains editable later.

SQLite is the source of truth for Mission Control configuration in v0; YAML configuration is not required.

---

# 70. Third-party tooling installation

AI Mission Manager does not automatically install:

* Herdr;
* GitHub CLI;
* Azure tooling;
* Jira CLI;
* Bitbucket CLI;
* Claude Code;
* Codex.

It detects missing requirements and provides instructions.

It may install/link its own `mission` CLI.

---

# 71. Platform

The Mission Control UI and `missiond` are macOS-only in v0.

Linux remote machines only require the execution-side tools already used for work, such as:

* Herdr;
* Git;
* Claude Code;
* Codex;
* repository dependencies.

There is no requirement to run the full Mission server on the Linux VM.

---

# 72. Updates and distribution

Automatic application updating is not required for the initial local v0.

The first target is:

* locally buildable app;
* usable `.app` bundle;
* reliable LaunchAgent;
* working CLI;
* stable persistence.

Signing/distribution/updater work can follow when the product is used beyond the initial development environment.

---

# 73. User stories

## 73.1 Local task without ticket

As the user, I can create:

```text
Investigate timeout in invoice processing
```

without creating anything in Jira/Azure/GitHub.

I can later delegate it to Codex, track its Workset and eventually decide to create an Azure DevOps work item without losing its prior history.

---

## 73.2 Remote Jira implementation

As the user, I can track `PLAT-847` from Jira Cloud, create/select a Workset on the remote Linux Herdr machine, launch Claude, interact with its actual terminal from AI Mission Manager, close/restart the app, and later return to that same execution context.

---

## 73.3 Multi-repository implementation

As the user, I can create:

```text
~/worktrees/work-b/PLAT-847/
├ service-a
├ service-b
└ frontend
```

and launch an agent from the Workset root so it can modify several services as part of the same logical task.

---

## 73.4 Multiple agent attempts

As the user, I can run Claude, then Codex, then Claude again against the same Workset while keeping all Runs in the Item history.

---

## 73.5 Parallel agents

As the user, I can start multiple agents inside the same Workset, including agents that may touch the same repository, without Mission Control enforcing locks.

---

## 73.6 Existing Herdr session

As the user, if I manually start Claude in a known Workset, Mission Control can detect it and offer to attach that session to the appropriate Item/Workset.

It does not associate it without my approval.

---

## 73.7 Watch a PR without an agent

As the user, I can paste a GitHub, Azure Repos, or Bitbucket PR and say effectively:

```text
Watch until October 2
Bring it back to me Friday
```

without creating a Workset or Run.

---

## 73.8 Review requested

As the user, a tracked PR receiving a relevant review request can appear in Needs Attention.

I can inspect its metadata and open the original provider for detailed review.

---

## 73.9 Publish multi-repo work

As the user, I can review which repositories contain commits and explicitly choose which ones to push/create PRs for.

Mission Control links the PRs it creates back to the Item and Workset.

---

## 73.10 Restart recovery

As the user, after rebooting the Mac, I see my persisted Items immediately and Mission Control reconciles them against Herdr/providers without spawning duplicate agents.

---

# 74. Acceptance criteria

The v0 is considered usable when all of the following end-to-end flows work.

## AC1 — local Item creation

Given the app is running, the user can create an Item from Quick Capture using only title and Context.

The Item appears in Inbox and survives restart.

---

## AC2 — external linking

The user can link supported Jira/Azure/GitHub/Bitbucket objects by URL.

Mission Control retrieves recognizable metadata through the configured CLI.

Unknown URLs remain valid generic links.

---

## AC3 — local Workset

The user can create or attach a local multi-repo Workset.

Mission Control does not alter attached existing repositories without explicit instruction.

---

## AC4 — local agent Run

The user can start Claude Code or Codex inside a local Workset through Herdr.

A Run is persisted with the correct Herdr associations.

---

## AC5 — embedded local terminal

The user can interact with the actual local Herdr pane through the embedded xterm terminal.

The application does not start a duplicate process for this interaction.

---

## AC6 — remote agent Run

The user can start Claude Code or Codex in a Workset on a saved remote Herdr machine.

The Run remains associated with that machine.

---

## AC7 — embedded remote terminal

The user can interact with the remote Herdr pane through the embedded terminal.

Closing/reopening the Mission Control view does not create a new agent.

---

## AC8 — Open Full Herdr

From a Run, `Open Full Herdr` lands on the exact intended Herdr execution context or reports that the exact target is unavailable.

This must work for both local and remote Runs.

---

## AC9 — reconciliation

After restarting `missiond` or macOS, existing Herdr resources are reconciled with persisted Runs.

Missing resources are marked missing rather than recreated.

---

## AC10 — untracked agent detection

An agent manually started within a known Workset can be detected and surfaced as a suggested attachment.

No automatic attachment occurs.

---

## AC11 — provider offline behavior

If one provider becomes unavailable, other providers and local functionality continue operating.

Last known state remains visible with a stale timestamp.

---

## AC12 — Watch workflow

The user can configure `watch_until`, `review_at`, and reminders.

Expired review/reminder conditions enter Needs Attention without marking the Item Done.

---

## AC13 — snooze

A Needs Attention entry can be snoozed and returns at the selected time without changing the underlying Item/external state.

---

## AC14 — external event grouping

Multiple external updates for one tracked object are grouped into one attention summary while remaining individually represented in Activity.

---

## AC15 — manual completion

No provider or agent event automatically mark35;96;18Ms an Item Done.

Only explicit user action does so.

---

## AC16 — safe publication

The user can preview and explicitly push/create PRs for selected repositories in a Workset.

No automatic push occurs merely because an agent committed.

---

## AC17 — ticket creation

A local Item can explicitly create and link a Jira, Azure DevOps, or GitHub Issue object through a preview workflow.

The original Item identity and history remain intact.

---

## AC18 — Context isolation

An agent Run started from an Item cannot silently use another Context's machine, repo, provider configuration, or prompt content.

Global search remains allowed.

---

## AC19 — Health visibility

Missing CLIs, auth failures, unavailable Herdr machines, and parsing failures are visible in System/Health without destroying cached application state.

---

## AC20 — no destructive completion

Marking an Item Done or archiving a Workset never deletes repositories, branches, worktrees, or terminal processes automatically.

---

# 75. Technical spikes required before broad implementation

A few implementation assumptions must be proven early.

These are validation tasks, not open product decisions.

## Spike A — Local Herdr terminal stream

Prove:

```text
Herdr local pane
→ Rust/Tauri bridge
→ xterm.js
→ interactive input/resize
```

Success means the real existing pane is controllable without creating a new PTY.

---

## Spike B — Remote Herdr terminal stream

Prove:

```text
Remote Herdr pane
→ SSH/Herdr bridge
→ missiond
→ xterm.js
```

Success35;95;18M means the user can interact with an existing remote pane without creating a duplicate session.

---

## Spike C — Open Full Herdr targeting

Prove that a persisted:

```text
machine + workspace + pane
```

association can be used to reach the correct visible Herdr context from the macOS app.

Local and remote targets must both be tested.

---

## Spike D — LaunchAgent CLI environment

Prove that `missiond`, launched outside an interactive shell, can reliably execute configured provider CLIs using absolute paths and intended credentials/configuration.

---

# 76. Implementation invariants

The following are architectural invariants.

1. **Item is the primary work entity.**
   External tickets do not replace it.

2. **Context is an isolation boundary.**
   Gl35;94;18Mobal visibility does not imply cross-context execution.

3. **Workset is not Run.**
   Worksets persist across many agent attempts.

4. **Workset is not Herdr Workspace.**
   They are associated but remain separate concepts owned by different systems.

5. **Run is not Item state.**
   Agent success/failure does not automatically complete work.

6. **External state is not Item state.**
   Tracker transitions remain independent.

7. **Herdr owns terminal runtime.**
   Mission Control does not become a second multiplexer.

8. **Provider adapters own CLI peculiarities.**
   The core domain does not know `gh` flags or Jira CLI output formats.

9. **Writes to external systems are explicit.**
   Tracking an object is not permission to modify it.

10. **Destructive filesystem operations are explicit.**
    Archiving is never deletion.

11. **Missing data is not state change.**
    Offline/parse errors retain the previous known state.

12. **Completion is human-controlled.**
    Only the user marks an Item Done.

---

# 77. Product flow summary

The intended complete flow is:

```text
                      Quick Capture
                            │
                            ▼
                           Item
                            │
           ┌────────────────┼─────────────────┐
           │                │                 │
      External links     Worksets          Watches
           │                │                 │
 Jira/Azure/GitHub          │            reminders
 GitHub/Azure/BB PRs        │          35;93;18M35;92;18M  activity
                            │
                      Herdr Workspace
                            │
                 ┌──────────┼──────────┐
                 │          │          │
               Claude      Codex      shell/tests
                 │          │
                 └──── Runs ┘
                            │
                       Git commits
                            │
                     explicit publish
                            │
                    PR(s) linked back
                            │
                       manual Done
```

---

# 78. v0 completion definition

The v0 stops expanding when these ten product flows are complete:

1. Quickly create a local Item.
2. Link Jira, Azure DevOps, GitHub, or Bitbucket objects.
3.35;91;18M Track changes and reminders over time.
4. Create or attach a multi-repository Workset.
5. Start Claude/Codex locally or remotely through Herdr.
6. Interact with the Herdr terminal inside Mission Control.
7. Recover tracked state after restart/reconnection.
8. Publish branches and create/link PRs explicitly.
9. Reach the exact full Herdr execution context.
10. Complete/archive work manually without destroying its execution environment.

Anything beyond those flows belongs to a later release unless it is necessary to make one of these flows reliable.

---

# 79. Explicit post-v0 candidates

The following ideas are intentionally deferred:

* Markdown task/spec import;
* Markdown directory watching;
* automatic dependency extraction from Markdown;
* tighter int35;89;17Megration with `to-spec` / `to-tickets`;
* MCP server;
* mobile app;
* authenticated network API;
* webhooks;
* autonomous agent manager;
* automatic dispatch;
* automatic retries;
* capacity-based scheduler;
* multi-user/team mode;
* complete embedded PR diff/review UX;
* self-hosted provider editions;
* Linux desktop client;
* automatic updater/distribution infrastructure.

---

# 80. Final product definition

**AI Mission Manager v0 is a personal macOS control plane for AI-assisted engineering work.**

It gives the user one operational view across local tasks, trackers, PRs, reminders, machines, repositories, Herdr sessions, Claude Code, and Codex.

It deliberately does not replace those systems.

Its job is to retain the relationships between them and mak35;88;17M35;87;17Me the user's active workload observable and actionable:

```text
What am I doing?
What is running?
What am I waiting for?
What changed?
What needs me?
Where is the exact execution?
What can I safely do next?
```

When AI Mission Manager can answer those questions reliably across both local and remote work, the v0 has succeeded.

