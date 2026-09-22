use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Context {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectDefaults {
    pub item_status: ItemStatus,
    pub execution_mode: ExecutionMode,
}

impl Default for ProjectDefaults {
    fn default() -> Self {
        Self {
            item_status: ItemStatus::Inbox,
            execution_mode: ExecutionMode::Worktree,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionMode {
    Direct,
    Worktree,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: i64,
    pub context_id: i64,
    pub name: String,
    pub defaults: ProjectDefaults,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Repository {
    pub id: i64,
    pub project_id: i64,
    pub name: String,
    pub remote_url: String,
    pub base_branch: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryLocation {
    pub repository_id: i64,
    pub machine_id: i64,
    pub checkout_path: String,
    pub worktree_root: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRepositoryInput {
    pub repository_id: i64,
    pub branch: String,
    pub base_branch: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRepository {
    pub repository_id: i64,
    pub branch: String,
    pub base_branch: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkspacePreparationState {
    Pending,
    Resumable,
    Ready,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: i64,
    pub item_id: i64,
    pub repositories: Vec<WorkspaceRepository>,
    pub preparation_state: WorkspacePreparationState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Worktree {
    pub id: i64,
    pub workspace_id: i64,
    pub repository_id: i64,
    pub machine_id: i64,
    pub path: String,
    pub branch: String,
    pub base_branch: String,
    pub is_dirty: bool,
}

pub fn sanitize_worktree_branch(branch: &str) -> String {
    let mut sanitized = String::new();
    let mut last_was_separator = false;
    for character in branch.trim().chars() {
        let allowed = character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.');
        if allowed {
            sanitized.push(character);
            last_was_separator = false;
        } else if !last_was_separator {
            sanitized.push('-');
            last_was_separator = true;
        }
    }
    let sanitized = sanitized.trim_matches(['-', '.']).to_owned();
    if sanitized.is_empty() || sanitized == "." || sanitized == ".." {
        "branch".into()
    } else {
        sanitized
    }
}

pub fn worktree_path(
    worktree_root: &Path,
    workspace_id: i64,
    branch: &str,
    repository_name: &str,
) -> PathBuf {
    worktree_root
        .join(format!("workspace-{workspace_id}"))
        .join(sanitize_worktree_branch(branch))
        .join(repository_name)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunCheckout {
    pub repository_id: i64,
    pub path: String,
    pub branch: String,
    pub is_dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MachineTransport {
    #[serde(rename = "local")]
    Local,
    #[serde(rename = "ssh")]
    Ssh {
        host: String,
        user: Option<String>,
        port: Option<u16>,
        identity_file: Option<String>,
        known_hosts_file: Option<String>,
        strict_host_key_checking: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MachineObservation {
    #[serde(rename = "unknown")]
    Unknown,
    #[serde(rename = "available")]
    Available,
    #[serde(rename = "offline")]
    Offline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Machine {
    pub id: i64,
    pub context_id: i64,
    pub name: String,
    pub socket_name: String,
    pub transport: MachineTransport,
    pub last_observed: MachineObservation,
    pub last_observed_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentKind {
    #[serde(rename = "claude")]
    Claude,
    #[serde(rename = "codex")]
    Codex,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionProfile {
    #[serde(rename = "investigate")]
    Investigate,
    #[serde(rename = "implement")]
    Implement,
    #[serde(rename = "review")]
    Review,
    #[serde(rename = "custom")]
    CustomPrompt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunState {
    #[serde(rename = "unknown")]
    Unknown,
    #[serde(rename = "working")]
    Working,
    #[serde(rename = "blocked")]
    Blocked,
    #[serde(rename = "finished")]
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RunPaneStatus {
    #[serde(rename = "unknown")]
    Unknown,
    #[serde(rename = "available")]
    Available,
    #[serde(rename = "missing")]
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunPromptSelection {
    pub include_objective: bool,
    pub include_notes: bool,
    pub external_object_ids: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    pub id: i64,
    pub item_id: i64,
    pub workspace_id: Option<i64>,
    pub repository_id: Option<i64>,
    pub worktree_id: Option<i64>,
    pub machine_id: i64,
    pub agent: AgentKind,
    pub execution_profile: ExecutionProfile,
    pub prompt: String,
    pub working_directory: String,
    pub session_name: String,
    pub pane_id: String,
    pub started_at: i64,
    pub state: RunState,
    pub pane_status: RunPaneStatus,
    pub direct_checkouts: Vec<RunCheckout>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentPaneObservation {
    pub machine_id: i64,
    pub agent: AgentKind,
    pub session_name: String,
    pub pane_id: String,
    pub current_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunSuggestion {
    pub machine_id: i64,
    pub machine_name: String,
    pub agent: AgentKind,
    pub session_name: String,
    pub pane_id: String,
    pub current_path: String,
    pub item_id: i64,
    pub item_identifier: String,
    pub item_title: String,
    pub context_id: i64,
    pub context_name: String,
    #[serde(default)]
    pub workspace_id: Option<i64>,
    #[serde(default)]
    pub repository_id: Option<i64>,
    #[serde(default)]
    pub worktree_id: Option<i64>,
    #[serde(default)]
    pub location_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reminder {
    pub id: i64,
    pub remind_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub id: i64,
    pub human_identifier: String,
    pub title: String,
    pub project_id: i64,
    pub status: ItemStatus,
    pub notes: String,
    pub reminders: Vec<Reminder>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExternalProvider {
    #[serde(rename = "github")]
    GitHub,
    #[serde(rename = "generic")]
    Generic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExternalObjectKind {
    #[serde(rename = "issue")]
    Issue,
    #[serde(rename = "pull_request")]
    PullRequest,
    #[serde(rename = "generic")]
    Generic,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalObjectInput {
    pub provider: ExternalProvider,
    pub kind: ExternalObjectKind,
    pub external_key: String,
    pub canonical_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalObject {
    pub id: i64,
    pub provider: ExternalProvider,
    pub kind: ExternalObjectKind,
    pub external_key: String,
    pub canonical_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalMetadata {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalSnapshotData {
    pub title: String,
    pub state: String,
    pub metadata: Vec<ExternalMetadata>,
    pub fetched_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalSnapshot {
    pub external_object_id: i64,
    pub title: String,
    pub state: String,
    pub metadata: Vec<ExternalMetadata>,
    pub fetched_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExternalChangeKind {
    #[serde(rename = "title")]
    Title,
    #[serde(rename = "state")]
    State,
    #[serde(rename = "metadata")]
    Metadata,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalChange {
    pub kind: ExternalChangeKind,
    pub key: Option<String>,
    pub previous: Option<String>,
    pub current: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Activity {
    pub id: i64,
    pub external_object_id: i64,
    pub observed_at: i64,
    pub changes: Vec<ExternalChange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalChangePolicy {
    pub title: bool,
    pub state: bool,
    pub metadata: bool,
}

impl ExternalChangePolicy {
    pub const fn all() -> Self {
        Self {
            title: true,
            state: true,
            metadata: true,
        }
    }

    fn allows(self, kind: ExternalChangeKind) -> bool {
        match kind {
            ExternalChangeKind::Title => self.title,
            ExternalChangeKind::State => self.state,
            ExternalChangeKind::Metadata => self.metadata,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextAttentionDefault {
    pub context_id: i64,
    pub object_kind: ExternalObjectKind,
    pub policy: ExternalChangePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Link {
    pub id: i64,
    pub item_id: i64,
    pub external_object_id: i64,
    pub reviewed_activity_id: i64,
    pub attention_policy: Option<ExternalChangePolicy>,
    pub watch_until: Option<String>,
    pub review_at: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AttentionEntryKind {
    #[serde(rename = "external_change")]
    ExternalChange,
    #[serde(rename = "review")]
    Review,
    #[serde(rename = "reminder")]
    Reminder,
    #[serde(rename = "blocked_run")]
    BlockedRun,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttentionEntry {
    pub kind: AttentionEntryKind,
    pub link_id: i64,
    pub reminder_id: Option<i64>,
    pub run_id: Option<i64>,
    pub item_id: i64,
    pub external_object_id: i64,
    pub source_title: String,
    pub source_url: String,
    pub activities: Vec<Activity>,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalLinkView {
    pub link: Link,
    pub object: ExternalObject,
    pub snapshot: Option<ExternalSnapshot>,
    pub attention_policy: ExternalChangePolicy,
    pub attention_entry: Option<AttentionEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemStatus {
    Inbox,
    Active,
    Waiting,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemRelationKind {
    Blocks,
    BlockedBy,
    RelatedTo,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemRelation {
    pub from_item_id: i64,
    pub to_item_id: i64,
    pub kind: ItemRelationKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemView {
    pub item: Item,
    pub context_id: i64,
    pub context_name: String,
    pub project_name: String,
    pub relationships: Vec<ItemRelation>,
    pub workspaces: Vec<Workspace>,
    pub worktrees: Vec<Worktree>,
    pub runs: Vec<Run>,
    pub links: Vec<ExternalLinkView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDeletionWorkspace {
    pub id: i64,
    pub item_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDeletionPlan {
    pub item_id: i64,
    pub human_identifier: String,
    pub title: String,
    pub reminder_count: usize,
    pub relationship_count: usize,
    pub workspaces: Vec<ItemDeletionWorkspace>,
    pub run_ids: Vec<i64>,
    pub active_run_ids: Vec<i64>,
    pub link_ids: Vec<i64>,
    pub orphaned_external_object_ids: Vec<i64>,
    pub orphaned_snapshot_count: usize,
    pub orphaned_activity_count: usize,
    #[serde(skip)]
    pub state_fingerprint: String,
}

impl ItemDeletionPlan {
    pub fn summary(&self) -> ItemDeletionSummary {
        ItemDeletionSummary {
            item_id: self.item_id,
            reminder_count: self.reminder_count,
            relationship_count: self.relationship_count,
            workspace_count: self.workspaces.len(),
            run_count: self.run_ids.len(),
            link_count: self.link_ids.len(),
            external_object_count: self.orphaned_external_object_ids.len(),
            snapshot_count: self.orphaned_snapshot_count,
            activity_count: self.orphaned_activity_count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemDeletionSummary {
    pub item_id: i64,
    pub reminder_count: usize,
    pub relationship_count: usize,
    pub workspace_count: usize,
    pub run_count: usize,
    pub link_count: usize,
    pub external_object_count: usize,
    pub snapshot_count: usize,
    pub activity_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalObjectDeletionPlan {
    pub external_object_id: i64,
    pub provider: ExternalProvider,
    pub kind: ExternalObjectKind,
    pub external_key: String,
    pub canonical_url: String,
    pub link_ids: Vec<i64>,
    pub snapshot_count: usize,
    pub activity_count: usize,
    #[serde(skip)]
    pub state_fingerprint: String,
}

impl ExternalObjectDeletionPlan {
    pub fn summary(&self) -> ExternalObjectDeletionSummary {
        ExternalObjectDeletionSummary {
            external_object_id: self.external_object_id,
            link_count: self.link_ids.len(),
            snapshot_count: self.snapshot_count,
            activity_count: self.activity_count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalObjectDeletionSummary {
    pub external_object_id: i64,
    pub link_count: usize,
    pub snapshot_count: usize,
    pub activity_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryDeletionWorkspace {
    pub id: i64,
    pub item_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryDeletionPlan {
    pub repository_id: i64,
    pub name: String,
    pub remote_url: String,
    pub workspaces: Vec<RepositoryDeletionWorkspace>,
    #[serde(skip)]
    pub state_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineDeletionRun {
    pub id: i64,
    pub item_id: i64,
    pub item_identifier: String,
    pub item_title: String,
    pub workspace_id: Option<i64>,
    pub worktree_id: Option<i64>,
    pub state: RunState,
    pub pane_status: RunPaneStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineDeletionPlan {
    pub machine_id: i64,
    pub name: String,
    pub runs: Vec<MachineDeletionRun>,
    pub active_run_ids: Vec<i64>,
    #[serde(skip)]
    pub state_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParentDeletionProject {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParentDeletionItem {
    pub id: i64,
    pub human_identifier: String,
    pub title: String,
    pub project_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParentDeletionRepository {
    pub id: i64,
    pub name: String,
    pub remote_url: String,
    pub project_id: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParentDeletionMachine {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParentDeletionRun {
    pub id: i64,
    pub item_id: i64,
    pub item_identifier: String,
    pub item_title: String,
    pub workspace_id: Option<i64>,
    pub worktree_id: Option<i64>,
    pub machine_id: i64,
    pub state: RunState,
    pub pane_status: RunPaneStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParentDeletionSummary {
    pub context_id: Option<i64>,
    pub project_id: Option<i64>,
    pub project_count: usize,
    pub item_count: usize,
    pub repository_count: usize,
    pub machine_count: usize,
    pub workspace_count: usize,
    pub run_count: usize,
    pub reminder_count: usize,
    pub relationship_count: usize,
    pub link_count: usize,
    pub attention_default_count: usize,
    pub external_object_count: usize,
    pub snapshot_count: usize,
    pub activity_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetLocalDataSummary {
    pub context_count: usize,
    pub project_count: usize,
    pub repository_count: usize,
    pub item_count: usize,
    pub workspace_count: usize,
    pub machine_count: usize,
    pub run_count: usize,
    pub reminder_count: usize,
    pub relationship_count: usize,
    pub link_count: usize,
    pub external_object_count: usize,
    pub snapshot_count: usize,
    pub activity_count: usize,
    pub attention_default_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetLocalDataRecord {
    pub kind: String,
    pub id: i64,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetLocalDataPlan {
    pub summary: ResetLocalDataSummary,
    pub affected_records: Vec<ResetLocalDataRecord>,
    pub workspaces: Vec<ItemDeletionWorkspace>,
    #[serde(skip)]
    pub state_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ParentDeletionPlan {
    pub context_id: Option<i64>,
    pub project_id: Option<i64>,
    pub name: String,
    pub projects: Vec<ParentDeletionProject>,
    pub items: Vec<ParentDeletionItem>,
    pub repositories: Vec<ParentDeletionRepository>,
    pub machines: Vec<ParentDeletionMachine>,
    pub workspaces: Vec<ItemDeletionWorkspace>,
    pub runs: Vec<ParentDeletionRun>,
    pub active_run_ids: Vec<i64>,
    pub reminder_count: usize,
    pub relationship_count: usize,
    pub link_ids: Vec<i64>,
    pub attention_defaults: Vec<ContextAttentionDefault>,
    pub orphaned_external_object_ids: Vec<i64>,
    pub orphaned_snapshot_count: usize,
    pub orphaned_activity_count: usize,
    #[serde(skip)]
    pub state_fingerprint: String,
}

impl ParentDeletionPlan {
    pub fn summary(&self) -> ParentDeletionSummary {
        ParentDeletionSummary {
            context_id: self.context_id,
            project_id: self.project_id,
            project_count: self.projects.len(),
            item_count: self.items.len(),
            repository_count: self.repositories.len(),
            machine_count: self.machines.len(),
            workspace_count: self.workspaces.len(),
            run_count: self.runs.len(),
            reminder_count: self.reminder_count,
            relationship_count: self.relationship_count,
            link_count: self.link_ids.len(),
            attention_default_count: self.attention_defaults.len(),
            external_object_count: self.orphaned_external_object_ids.len(),
            snapshot_count: self.orphaned_snapshot_count,
            activity_count: self.orphaned_activity_count,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HomeView {
    pub needs_attention: Vec<ItemView>,
    pub attention_entries: Vec<AttentionEntry>,
    pub running: Vec<ItemView>,
    pub waiting: Vec<ItemView>,
    pub due: Vec<ItemView>,
    pub completed: Vec<ItemView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "camelCase")]
pub enum AuditAction {
    ContextCreated {
        context_id: i64,
    },
    ProjectCreated {
        project_id: i64,
    },
    RepositoryRegistered {
        repository_id: i64,
    },
    ItemCreated {
        item_id: i64,
    },
    ItemStatusChanged {
        item_id: i64,
        from: ItemStatus,
        to: ItemStatus,
    },
    ItemTitleChanged {
        item_id: i64,
    },
    ItemNotesChanged {
        item_id: i64,
    },
    ItemRemindersChanged {
        item_id: i64,
    },
    ItemRelationChanged {
        from_item_id: i64,
        to_item_id: i64,
        kind: ItemRelationKind,
    },
    WorkspaceCreated {
        workspace_id: i64,
    },
    WorkspaceUpdated {
        workspace_id: i64,
    },
    WorkspaceRemoved {
        workspace_id: i64,
        #[serde(default)]
        repository_count: Option<usize>,
    },
    MachineRegistered {
        machine_id: i64,
    },
    MachineObserved {
        machine_id: i64,
        observation: MachineObservation,
    },
    MachineDeleted {
        machine_id: i64,
        #[serde(default)]
        run_count: Option<usize>,
    },
    RunCreated {
        run_id: i64,
    },
    RunStopped {
        run_id: i64,
    },
    RunStateChanged {
        run_id: i64,
        from: RunState,
        to: RunState,
    },
    RunDeleted {
        run_id: i64,
    },
    RunPaneStatusChanged {
        run_id: i64,
        from: RunPaneStatus,
        to: RunPaneStatus,
    },
    ExternalObjectCreated {
        external_object_id: i64,
    },
    ExternalObjectRefreshed {
        external_object_id: i64,
    },
    LinkCreated {
        link_id: i64,
    },
    LinkUpdated {
        link_id: i64,
    },
    LinkDeleted {
        link_id: i64,
        #[serde(default)]
        external_object_id: Option<i64>,
        #[serde(default)]
        external_object_deleted: Option<bool>,
    },
    ExternalObjectDeleted {
        external_object_id: i64,
        #[serde(default)]
        link_count: Option<usize>,
        #[serde(default)]
        snapshot_count: Option<usize>,
        #[serde(default)]
        activity_count: Option<usize>,
    },
    ContextAttentionDefaultChanged {
        context_id: i64,
        object_kind: ExternalObjectKind,
    },
    ItemDeleted {
        summary: ItemDeletionSummary,
    },
    RepositoryDeleted {
        repository_id: i64,
        #[serde(default)]
        workspace_count: Option<usize>,
    },
    ProjectDeleted {
        summary: ParentDeletionSummary,
    },
    ContextDeleted {
        summary: ParentDeletionSummary,
    },
    ResetBoundary {
        context_id: i64,
        project_id: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: i64,
    pub recorded_at: i64,
    pub action: AuditAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedActivity {
    pub activity: Activity,
    pub object: ExternalObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivityTabView {
    pub audit_entries: Vec<AuditEntry>,
    pub activities: Vec<ObservedActivity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainState {
    pub next_context_id: i64,
    pub next_project_id: i64,
    pub next_item_id: i64,
    pub next_item_number: i64,
    pub next_repository_id: i64,
    pub next_workspace_id: i64,
    pub next_worktree_id: i64,
    pub next_machine_id: i64,
    pub next_run_id: i64,
    pub next_external_object_id: i64,
    pub next_link_id: i64,
    pub next_activity_id: i64,
    pub next_reminder_id: i64,
    pub contexts: Vec<Context>,
    pub projects: Vec<Project>,
    pub repositories: Vec<Repository>,
    pub repository_locations: Vec<RepositoryLocation>,
    pub items: Vec<Item>,
    pub workspaces: Vec<Workspace>,
    pub worktrees: Vec<Worktree>,
    pub machines: Vec<Machine>,
    pub runs: Vec<Run>,
    pub relationships: Vec<ItemRelation>,
    pub external_objects: Vec<ExternalObject>,
    pub links: Vec<Link>,
    pub snapshots: Vec<ExternalSnapshot>,
    pub activities: Vec<Activity>,
    pub attention_defaults: Vec<ContextAttentionDefault>,
}

pub fn activity_tab_view(state: &DomainState, audit_entries: Vec<AuditEntry>) -> ActivityTabView {
    let activities = state
        .activities
        .iter()
        .rev()
        .filter_map(|activity| {
            let object = state
                .external_objects
                .iter()
                .find(|object| object.id == activity.external_object_id)?;
            Some(ObservedActivity {
                activity: activity.clone(),
                object: object.clone(),
            })
        })
        .collect();

    ActivityTabView {
        audit_entries,
        activities,
    }
}

pub fn plan_reset_local_data(state: &DomainState) -> ResetLocalDataPlan {
    let mut affected_records = Vec::new();
    affected_records.extend(state.contexts.iter().map(|context| ResetLocalDataRecord {
        kind: "Context".into(),
        id: context.id,
        label: context.name.clone(),
    }));
    affected_records.extend(state.projects.iter().map(|project| ResetLocalDataRecord {
        kind: "Project".into(),
        id: project.id,
        label: project.name.clone(),
    }));
    affected_records.extend(
        state
            .repositories
            .iter()
            .map(|repository| ResetLocalDataRecord {
                kind: "Repository".into(),
                id: repository.id,
                label: repository.name.clone(),
            }),
    );
    affected_records.extend(state.items.iter().map(|item| ResetLocalDataRecord {
        kind: "Item".into(),
        id: item.id,
        label: format!("{} · {}", item.human_identifier, item.title),
    }));
    affected_records.extend(
        state
            .workspaces
            .iter()
            .map(|workspace| ResetLocalDataRecord {
                kind: "Workspace".into(),
                id: workspace.id,
                label: format!("Item {}", workspace.item_id),
            }),
    );
    affected_records.extend(state.machines.iter().map(|machine| ResetLocalDataRecord {
        kind: "Machine".into(),
        id: machine.id,
        label: machine.name.clone(),
    }));
    affected_records.extend(state.runs.iter().map(|run| ResetLocalDataRecord {
        kind: "Run".into(),
        id: run.id,
        label: format!(
            "{:?} · Item {} · Machine {}",
            run.state,
            state
                .items
                .iter()
                .find(|item| item.id == run.item_id)
                .map(|item| item.human_identifier.as_str())
                .unwrap_or("unknown"),
            state
                .machines
                .iter()
                .find(|machine| machine.id == run.machine_id)
                .map(|machine| machine.name.as_str())
                .unwrap_or("unknown")
        ),
    }));
    affected_records.extend(state.links.iter().map(|link| ResetLocalDataRecord {
        kind: "Link".into(),
        id: link.id,
        label: format!(
            "Item {} → {}",
            state
                .items
                .iter()
                .find(|item| item.id == link.item_id)
                .map(|item| item.human_identifier.as_str())
                .unwrap_or("unknown"),
            state
                .external_objects
                .iter()
                .find(|object| object.id == link.external_object_id)
                .map(|object| object.external_key.as_str())
                .unwrap_or("unknown")
        ),
    }));
    affected_records.extend(state.external_objects.iter().map(|external_object| {
        ResetLocalDataRecord {
            kind: "External Object".into(),
            id: external_object.id,
            label: external_object.external_key.clone(),
        }
    }));

    ResetLocalDataPlan {
        summary: ResetLocalDataSummary {
            context_count: state.contexts.len(),
            project_count: state.projects.len(),
            repository_count: state.repositories.len(),
            item_count: state.items.len(),
            workspace_count: state.workspaces.len(),
            machine_count: state.machines.len(),
            run_count: state.runs.len(),
            reminder_count: state.items.iter().map(|item| item.reminders.len()).sum(),
            relationship_count: state.relationships.len(),
            link_count: state.links.len(),
            external_object_count: state.external_objects.len(),
            snapshot_count: state.snapshots.len(),
            activity_count: state.activities.len(),
            attention_default_count: state.attention_defaults.len(),
        },
        affected_records,
        workspaces: state
            .workspaces
            .iter()
            .map(|workspace| ItemDeletionWorkspace {
                id: workspace.id,
                item_id: workspace.item_id,
            })
            .collect(),
        state_fingerprint: serde_json::to_string(state)
            .expect("DomainState should always be serializable"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    CreateContext {
        name: String,
    },
    CreateProject {
        context_id: i64,
        name: String,
        defaults: ProjectDefaults,
    },
    RegisterRepository {
        project_id: i64,
        name: String,
        remote_url: String,
    },
    RegisterRepositoryAtLocation {
        project_id: i64,
        name: String,
        remote_url: String,
        base_branch: String,
        machine_id: i64,
        checkout_path: String,
        worktree_root: String,
    },
    ConfigureRepositoryLocation {
        repository_id: i64,
        machine_id: i64,
        checkout_path: String,
        worktree_root: String,
    },
    ResetLocalData,
    DeleteRepository {
        repository_id: i64,
        workspace_ids: Vec<i64>,
    },
    DeleteProject {
        project_id: i64,
        item_ids: Vec<i64>,
        repository_ids: Vec<i64>,
        workspace_ids: Vec<i64>,
    },
    DeleteContext {
        context_id: i64,
        project_ids: Vec<i64>,
        item_ids: Vec<i64>,
        repository_ids: Vec<i64>,
        workspace_ids: Vec<i64>,
        machine_ids: Vec<i64>,
    },
    DeleteMachine {
        machine_id: i64,
        run_ids: Vec<i64>,
    },
    CreateItem {
        title: String,
        context_id: i64,
        project_id: i64,
    },
    CreateWorkspace {
        item_id: i64,
        repositories: Vec<WorkspaceRepositoryInput>,
    },
    CreateWorktree {
        workspace_id: i64,
        repository_id: i64,
        machine_id: i64,
        path: String,
        branch: String,
        base_branch: String,
        is_dirty: bool,
    },
    MarkWorkspaceResumable {
        workspace_id: i64,
    },
    RemoveWorktree {
        worktree_id: i64,
    },
    RemoveWorkspace {
        workspace_id: i64,
    },
    RegisterMachine {
        context_id: i64,
        name: String,
        socket_name: String,
        transport: MachineTransport,
    },
    ObserveMachine {
        machine_id: i64,
        observation: MachineObservation,
        observed_at: i64,
    },
    DeleteItem {
        item_id: i64,
    },
    DeleteLink {
        link_id: i64,
    },
    DeleteExternalObject {
        external_object_id: i64,
    },
    DeleteRun {
        run_id: i64,
    },
    StartDirectRun {
        item_id: i64,
        workspace_id: i64,
        machine_id: i64,
        agent: AgentKind,
        execution_profile: ExecutionProfile,
        prompt: String,
        working_directory: String,
        session_name: String,
        pane_id: String,
        started_at: i64,
        prompt_selection: RunPromptSelection,
        checkouts: Vec<RunCheckout>,
        repository_id: i64,
        allow_dirty: bool,
        allow_shared_checkouts: bool,
    },
    StartWorktreeRun {
        item_id: i64,
        workspace_id: i64,
        worktree_id: i64,
        machine_id: i64,
        agent: AgentKind,
        execution_profile: ExecutionProfile,
        prompt: String,
        working_directory: String,
        session_name: String,
        pane_id: String,
        started_at: i64,
        prompt_selection: RunPromptSelection,
    },
    AttachRun {
        item_id: i64,
        workspace_id: i64,
        worktree_id: Option<i64>,
        repository_id: i64,
        machine_id: i64,
        agent: AgentKind,
        working_directory: String,
        session_name: String,
        pane_id: String,
        attached_at: i64,
    },
    UpdateRunState {
        run_id: i64,
        state: RunState,
    },
    SetRunPaneStatus {
        run_id: i64,
        status: RunPaneStatus,
    },
    SetItemStatus {
        item_id: i64,
        status: ItemStatus,
    },
    SetItemTitle {
        item_id: i64,
        title: String,
    },
    SetItemNotes {
        item_id: i64,
        notes: String,
    },
    SetItemRelation {
        from_item_id: i64,
        to_item_id: i64,
        kind: ItemRelationKind,
    },
    AddItemReminder {
        item_id: i64,
        remind_at: String,
    },
    RemoveItemReminder {
        item_id: i64,
        reminder_id: i64,
    },
    SetLinkWatchUntil {
        link_id: i64,
        watch_until: Option<String>,
    },
    SetLinkReviewAt {
        link_id: i64,
        review_at: Option<String>,
    },
    ClearLinkReviewAt {
        link_id: i64,
    },
    LinkExternalObject {
        item_id: i64,
        object: ExternalObjectInput,
        snapshot: Option<ExternalSnapshotData>,
    },
    RefreshExternalObject {
        external_object_id: i64,
        snapshot: ExternalSnapshotData,
    },
    SetLinkAttentionPolicy {
        link_id: i64,
        policy: Option<ExternalChangePolicy>,
    },
    SetContextAttentionDefault {
        context_id: i64,
        object_kind: ExternalObjectKind,
        policy: ExternalChangePolicy,
    },
    MarkLinkReviewed {
        link_id: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    PersistContext {
        context: Context,
        next_context_id: i64,
    },
    PersistProject {
        project: Project,
        next_project_id: i64,
    },
    PersistRepository {
        repository: Repository,
        next_repository_id: i64,
    },
    UpdateRepository {
        repository: Repository,
    },
    PersistRepositoryLocation {
        location: RepositoryLocation,
    },
    ResetLocalData {
        context: Context,
        project: Project,
        next_context_id: i64,
        next_project_id: i64,
    },
    PersistItem {
        item: Item,
        next_item_number: i64,
        next_item_id: i64,
    },
    PersistWorkspace {
        workspace: Workspace,
        next_workspace_id: i64,
    },
    PersistWorkspaceUpdate {
        workspace: Workspace,
    },
    PersistWorktree {
        worktree: Worktree,
        next_worktree_id: i64,
    },
    RemoveWorktree {
        worktree_id: i64,
    },
    RemoveWorkspace {
        workspace_id: i64,
    },
    RemoveRepository {
        repository_id: i64,
    },
    RemoveMachine {
        machine_id: i64,
    },
    PersistMachine {
        machine: Machine,
        next_machine_id: i64,
    },
    PersistMachineObservation {
        machine: Machine,
    },
    PersistRun {
        run: Run,
        next_run_id: i64,
    },
    PersistRunState {
        run: Run,
    },
    PersistRunPaneStatus {
        run: Run,
    },
    RemoveRun {
        run_id: i64,
    },
    PersistItemUpdate {
        item: Item,
    },
    PersistItemReminders {
        item: Item,
        next_reminder_id: i64,
    },
    PersistItemRelation {
        relation: ItemRelation,
    },
    PersistExternalObject {
        object: ExternalObject,
        next_external_object_id: i64,
    },
    PersistLink {
        link: Link,
        next_link_id: i64,
    },
    PersistLinkState {
        link: Link,
    },
    PersistExternalSnapshot {
        snapshot: ExternalSnapshot,
    },
    PersistActivity {
        activity: Activity,
        next_activity_id: i64,
    },
    PersistContextAttentionDefault {
        attention_default: ContextAttentionDefault,
    },
    RemoveItemCascade {
        item_id: i64,
        orphaned_external_object_ids: Vec<i64>,
        summary: ItemDeletionSummary,
    },
    RemoveProjectCascade {
        project_id: i64,
        item_ids: Vec<i64>,
        repository_ids: Vec<i64>,
        workspace_ids: Vec<i64>,
        orphaned_external_object_ids: Vec<i64>,
        summary: ParentDeletionSummary,
    },
    RemoveContextCascade {
        context_id: i64,
        project_ids: Vec<i64>,
        item_ids: Vec<i64>,
        repository_ids: Vec<i64>,
        workspace_ids: Vec<i64>,
        machine_ids: Vec<i64>,
        orphaned_external_object_ids: Vec<i64>,
        summary: ParentDeletionSummary,
    },
    RemoveLink {
        link_id: i64,
        external_object_id: i64,
    },
    RemoveExternalObject {
        external_object_id: i64,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub state: DomainState,
    pub effects: Vec<Effect>,
}

pub fn plan_item_deletion(
    state: &DomainState,
    item_id: i64,
) -> Result<ItemDeletionPlan, DomainError> {
    let item = state
        .items
        .iter()
        .find(|item| item.id == item_id)
        .ok_or(DomainError::ItemNotFound { item_id })?;
    let workspaces = state
        .workspaces
        .iter()
        .filter(|workspace| workspace.item_id == item_id)
        .map(|workspace| ItemDeletionWorkspace {
            id: workspace.id,
            item_id: workspace.item_id,
        })
        .collect::<Vec<_>>();
    let run_ids = state
        .runs
        .iter()
        .filter(|run| run.item_id == item_id)
        .map(|run| run.id)
        .collect::<Vec<_>>();
    let active_run_ids = state
        .runs
        .iter()
        .filter(|run| run.item_id == item_id && run.state != RunState::Finished)
        .map(|run| run.id)
        .collect::<Vec<_>>();
    let link_ids = state
        .links
        .iter()
        .filter(|link| link.item_id == item_id)
        .map(|link| link.id)
        .collect::<Vec<_>>();
    let linked_external_object_ids = state
        .links
        .iter()
        .filter(|link| link.item_id == item_id)
        .map(|link| link.external_object_id)
        .collect::<Vec<_>>();
    let orphaned_external_object_ids = linked_external_object_ids
        .iter()
        .copied()
        .filter(|external_object_id| {
            !state.links.iter().any(|link| {
                link.external_object_id == *external_object_id && link.item_id != item_id
            })
        })
        .collect::<Vec<_>>();

    Ok(ItemDeletionPlan {
        item_id,
        human_identifier: item.human_identifier.clone(),
        title: item.title.clone(),
        reminder_count: item.reminders.len(),
        relationship_count: state
            .relationships
            .iter()
            .filter(|relation| relation.from_item_id == item_id || relation.to_item_id == item_id)
            .count(),
        workspaces,
        run_ids,
        active_run_ids,
        link_ids,
        orphaned_snapshot_count: state
            .snapshots
            .iter()
            .filter(|snapshot| orphaned_external_object_ids.contains(&snapshot.external_object_id))
            .count(),
        orphaned_activity_count: state
            .activities
            .iter()
            .filter(|activity| orphaned_external_object_ids.contains(&activity.external_object_id))
            .count(),
        orphaned_external_object_ids,
        state_fingerprint: serde_json::to_string(state)
            .expect("DomainState should always be serializable"),
    })
}

pub fn plan_external_object_deletion(
    state: &DomainState,
    external_object_id: i64,
) -> Result<ExternalObjectDeletionPlan, DomainError> {
    let object = state
        .external_objects
        .iter()
        .find(|object| object.id == external_object_id)
        .ok_or(DomainError::ExternalObjectNotFound { external_object_id })?;
    let link_ids = state
        .links
        .iter()
        .filter(|link| link.external_object_id == external_object_id)
        .map(|link| link.id)
        .collect::<Vec<_>>();

    Ok(ExternalObjectDeletionPlan {
        external_object_id,
        provider: object.provider,
        kind: object.kind,
        external_key: object.external_key.clone(),
        canonical_url: object.canonical_url.clone(),
        link_ids,
        snapshot_count: state
            .snapshots
            .iter()
            .filter(|snapshot| snapshot.external_object_id == external_object_id)
            .count(),
        activity_count: state
            .activities
            .iter()
            .filter(|activity| activity.external_object_id == external_object_id)
            .count(),
        state_fingerprint: serde_json::to_string(state)
            .expect("DomainState should always be serializable"),
    })
}

pub fn plan_repository_deletion(
    state: &DomainState,
    repository_id: i64,
) -> Result<RepositoryDeletionPlan, DomainError> {
    let repository = state
        .repositories
        .iter()
        .find(|repository| repository.id == repository_id)
        .ok_or(DomainError::RepositoryNotFound { repository_id })?;
    let workspaces = state
        .workspaces
        .iter()
        .filter(|workspace| {
            workspace
                .repositories
                .iter()
                .any(|selected| selected.repository_id == repository_id)
        })
        .map(|workspace| RepositoryDeletionWorkspace {
            id: workspace.id,
            item_id: workspace.item_id,
        })
        .collect::<Vec<_>>();

    Ok(RepositoryDeletionPlan {
        repository_id,
        name: repository.name.clone(),
        remote_url: repository.remote_url.clone(),
        workspaces,
        state_fingerprint: serde_json::to_string(state)
            .expect("DomainState should always be serializable"),
    })
}

pub fn plan_machine_deletion(
    state: &DomainState,
    machine_id: i64,
) -> Result<MachineDeletionPlan, DomainError> {
    let machine = state
        .machines
        .iter()
        .find(|machine| machine.id == machine_id)
        .ok_or(DomainError::MachineNotFound { machine_id })?;
    let runs = state
        .runs
        .iter()
        .filter(|run| run.machine_id == machine_id)
        .map(|run| {
            let item = state
                .items
                .iter()
                .find(|item| item.id == run.item_id)
                .ok_or(DomainError::ItemNotFound {
                    item_id: run.item_id,
                })?;
            Ok(MachineDeletionRun {
                id: run.id,
                item_id: run.item_id,
                item_identifier: item.human_identifier.clone(),
                item_title: item.title.clone(),
                workspace_id: run.workspace_id,
                worktree_id: run.worktree_id,
                state: run.state,
                pane_status: run.pane_status,
            })
        })
        .collect::<Result<Vec<_>, DomainError>>()?;
    let active_run_ids = runs
        .iter()
        .filter(|run| run.state != RunState::Finished)
        .map(|run| run.id)
        .collect();

    Ok(MachineDeletionPlan {
        machine_id,
        name: machine.name.clone(),
        runs,
        active_run_ids,
        state_fingerprint: serde_json::to_string(state)
            .expect("DomainState should always be serializable"),
    })
}

pub fn plan_project_deletion(
    state: &DomainState,
    project_id: i64,
) -> Result<ParentDeletionPlan, DomainError> {
    let project = state
        .projects
        .iter()
        .find(|project| project.id == project_id)
        .ok_or(DomainError::ProjectNotFound { project_id })?;
    plan_parent_deletion(
        state,
        None,
        Some(project_id),
        vec![project_id],
        project.name.clone(),
    )
}

pub fn plan_context_deletion(
    state: &DomainState,
    context_id: i64,
) -> Result<ParentDeletionPlan, DomainError> {
    let context = state
        .contexts
        .iter()
        .find(|context| context.id == context_id)
        .ok_or(DomainError::ContextNotFound { context_id })?;
    let project_ids = state
        .projects
        .iter()
        .filter(|project| project.context_id == context_id)
        .map(|project| project.id)
        .collect();
    plan_parent_deletion(
        state,
        Some(context_id),
        None,
        project_ids,
        context.name.clone(),
    )
}

fn plan_parent_deletion(
    state: &DomainState,
    context_id: Option<i64>,
    project_id: Option<i64>,
    project_ids: Vec<i64>,
    name: String,
) -> Result<ParentDeletionPlan, DomainError> {
    let projects = state
        .projects
        .iter()
        .filter(|project| project_ids.contains(&project.id))
        .map(|project| ParentDeletionProject {
            id: project.id,
            name: project.name.clone(),
        })
        .collect::<Vec<_>>();
    let items = state
        .items
        .iter()
        .filter(|item| project_ids.contains(&item.project_id))
        .map(|item| ParentDeletionItem {
            id: item.id,
            human_identifier: item.human_identifier.clone(),
            title: item.title.clone(),
            project_id: item.project_id,
        })
        .collect::<Vec<_>>();
    let item_ids = items.iter().map(|item| item.id).collect::<Vec<_>>();
    let repositories = state
        .repositories
        .iter()
        .filter(|repository| project_ids.contains(&repository.project_id))
        .map(|repository| ParentDeletionRepository {
            id: repository.id,
            name: repository.name.clone(),
            remote_url: repository.remote_url.clone(),
            project_id: repository.project_id,
        })
        .collect::<Vec<_>>();
    let workspaces = state
        .workspaces
        .iter()
        .filter(|workspace| item_ids.contains(&workspace.item_id))
        .map(|workspace| ItemDeletionWorkspace {
            id: workspace.id,
            item_id: workspace.item_id,
        })
        .collect::<Vec<_>>();
    let machines = context_id
        .map(|context_id| {
            state
                .machines
                .iter()
                .filter(|machine| machine.context_id == context_id)
                .map(|machine| ParentDeletionMachine {
                    id: machine.id,
                    name: machine.name.clone(),
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let machine_ids = machines
        .iter()
        .map(|machine| machine.id)
        .collect::<Vec<_>>();
    let runs = state
        .runs
        .iter()
        .filter(|run| item_ids.contains(&run.item_id) || machine_ids.contains(&run.machine_id))
        .map(|run| {
            let item = state
                .items
                .iter()
                .find(|item| item.id == run.item_id)
                .ok_or(DomainError::ItemNotFound {
                    item_id: run.item_id,
                })?;
            Ok(ParentDeletionRun {
                id: run.id,
                item_id: run.item_id,
                item_identifier: item.human_identifier.clone(),
                item_title: item.title.clone(),
                workspace_id: run.workspace_id,
                worktree_id: run.worktree_id,
                machine_id: run.machine_id,
                state: run.state,
                pane_status: run.pane_status,
            })
        })
        .collect::<Result<Vec<_>, DomainError>>()?;
    let active_run_ids = runs
        .iter()
        .filter(|run| run.state != RunState::Finished)
        .map(|run| run.id)
        .collect::<Vec<_>>();
    let link_ids = state
        .links
        .iter()
        .filter(|link| item_ids.contains(&link.item_id))
        .map(|link| link.id)
        .collect::<Vec<_>>();
    let linked_external_object_ids = state
        .links
        .iter()
        .filter(|link| item_ids.contains(&link.item_id))
        .map(|link| link.external_object_id)
        .collect::<Vec<_>>();
    let orphaned_external_object_ids = linked_external_object_ids
        .iter()
        .copied()
        .filter(|external_object_id| {
            !state.links.iter().any(|link| {
                link.external_object_id == *external_object_id && !item_ids.contains(&link.item_id)
            })
        })
        .fold(Vec::new(), |mut ids, external_object_id| {
            if !ids.contains(&external_object_id) {
                ids.push(external_object_id);
            }
            ids
        });
    let attention_defaults = context_id
        .map(|context_id| {
            state
                .attention_defaults
                .iter()
                .filter(|attention_default| attention_default.context_id == context_id)
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(ParentDeletionPlan {
        context_id,
        project_id,
        name,
        projects,
        items: items.clone(),
        repositories,
        machines,
        workspaces,
        runs,
        active_run_ids,
        reminder_count: items
            .iter()
            .filter_map(|parent_item| state.items.iter().find(|item| item.id == parent_item.id))
            .map(|item| item.reminders.len())
            .sum(),
        relationship_count: state
            .relationships
            .iter()
            .filter(|relation| {
                item_ids.contains(&relation.from_item_id) || item_ids.contains(&relation.to_item_id)
            })
            .count(),
        link_ids,
        attention_defaults,
        orphaned_snapshot_count: state
            .snapshots
            .iter()
            .filter(|snapshot| orphaned_external_object_ids.contains(&snapshot.external_object_id))
            .count(),
        orphaned_activity_count: state
            .activities
            .iter()
            .filter(|activity| orphaned_external_object_ids.contains(&activity.external_object_id))
            .count(),
        orphaned_external_object_ids,
        state_fingerprint: serde_json::to_string(state)
            .expect("DomainState should always be serializable"),
    })
}

fn sorted_ids(mut ids: Vec<i64>) -> Vec<i64> {
    ids.sort_unstable();
    ids
}

fn parent_selection_matches(expected: &[i64], provided: Vec<i64>) -> bool {
    sorted_ids(expected.to_vec()) == sorted_ids(provided)
}

fn workspace_preparation_state(
    state: &DomainState,
    workspace_id: i64,
    previous: WorkspacePreparationState,
) -> WorkspacePreparationState {
    let Some(workspace) = state
        .workspaces
        .iter()
        .find(|workspace| workspace.id == workspace_id)
    else {
        return previous;
    };
    let complete = workspace.repositories.iter().all(|repository| {
        state.worktrees.iter().any(|worktree| {
            worktree.workspace_id == workspace_id
                && worktree.repository_id == repository.repository_id
        })
    });
    if complete {
        WorkspacePreparationState::Ready
    } else if previous == WorkspacePreparationState::Resumable {
        WorkspacePreparationState::Resumable
    } else {
        WorkspacePreparationState::Pending
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DomainError {
    #[error("a Context name cannot be blank")]
    EmptyContextName,
    #[error("a Project name cannot be blank")]
    EmptyProjectName,
    #[error("an Item title cannot be blank")]
    EmptyTitle,
    #[error("a Reminder date cannot be blank")]
    EmptyReminderAt,
    #[error("Context name already exists: {name}")]
    ContextNameTaken { name: String },
    #[error("Project name already exists in Context {context_id}: {name}")]
    ProjectNameTaken { context_id: i64, name: String },
    #[error("Context {context_id} does not exist")]
    ContextNotFound { context_id: i64 },
    #[error("Project {project_id} does not exist")]
    ProjectNotFound { project_id: i64 },
    #[error("Project {project_id} deletion selection changed; review the deletion preview again")]
    ProjectDeletionPlanMismatch { project_id: i64 },
    #[error("Project {project_id} has active Runs: {run_ids:?}")]
    ProjectHasActiveRuns { project_id: i64, run_ids: Vec<i64> },
    #[error("Context {context_id} deletion selection changed; review the deletion preview again")]
    ContextDeletionPlanMismatch { context_id: i64 },
    #[error("Context {context_id} has active Runs: {run_ids:?}")]
    ContextHasActiveRuns { context_id: i64, run_ids: Vec<i64> },
    #[error("local-data reset has active Runs: {run_ids:?}")]
    ResetHasActiveRuns { run_ids: Vec<i64> },
    #[error("the last Context cannot be deleted")]
    CannotDeleteLastContext,
    #[error("Project {project_id} belongs to another Context")]
    ProjectContextMismatch { project_id: i64, context_id: i64 },
    #[error("a Repository name cannot be blank")]
    EmptyRepositoryName,
    #[error("a Repository name must be a single directory name")]
    InvalidRepositoryName,
    #[error("a Repository remote URL cannot be blank")]
    EmptyRepositoryRemoteUrl,
    #[error("a Repository base branch cannot be blank")]
    EmptyRepositoryBaseBranch,
    #[error("a Repository checkout path cannot be blank")]
    EmptyRepositoryCheckoutPath,
    #[error("a Repository Worktree root cannot be blank")]
    EmptyRepositoryWorktreeRoot,
    #[error("Repository name already exists in Project {project_id}: {name}")]
    RepositoryNameTaken { project_id: i64, name: String },
    #[error("Repository {name} in Project {project_id} has a different remote URL")]
    RepositoryRemoteMismatch { project_id: i64, name: String },
    #[error("Repository {repository_id} does not exist")]
    RepositoryNotFound { repository_id: i64 },
    #[error("Repository {repository_id} belongs to another Project than {project_id}")]
    RepositoryProjectMismatch { repository_id: i64, project_id: i64 },
    #[error("Repository {repository_id} has already been configured on Machine {machine_id}")]
    RepositoryLocationAlreadyExists { repository_id: i64, machine_id: i64 },
    #[error(
        "Repository {repository_id} deletion must include Workspaces {expected_workspace_ids:?}; received {provided_workspace_ids:?}"
    )]
    RepositoryWorkspacesMismatch {
        repository_id: i64,
        expected_workspace_ids: Vec<i64>,
        provided_workspace_ids: Vec<i64>,
    },
    #[error(
        "Machine {machine_id} deletion must include Runs {expected_run_ids:?}; received {provided_run_ids:?}"
    )]
    MachineRunsMismatch {
        machine_id: i64,
        expected_run_ids: Vec<i64>,
        provided_run_ids: Vec<i64>,
    },
    #[error("a branch cannot be blank")]
    EmptyBranch,
    #[error("a Workspace must include at least one Repository")]
    EmptyWorkspaceRepositories,
    #[error("Workspace {workspace_id} does not exist")]
    WorkspaceNotFound { workspace_id: i64 },
    #[error("Workspace {workspace_id} has Run history and cannot be removed")]
    WorkspaceHasRuns { workspace_id: i64 },
    #[error("Workspace {workspace_id} belongs to another Item")]
    WorkspaceItemMismatch { workspace_id: i64, item_id: i64 },
    #[error("Repository {repository_id} is not selected in Workspace {workspace_id}")]
    RunRepositoryNotSelected {
        repository_id: i64,
        workspace_id: i64,
    },
    #[error("Worktree {worktree_id} does not belong to Workspace {workspace_id}")]
    RunWorktreeWorkspaceMismatch { worktree_id: i64, workspace_id: i64 },
    #[error("Run working directory does not match Repository {repository_id}")]
    RunWorkingDirectoryRepositoryMismatch { repository_id: i64 },
    #[error("Repository {repository_id} is already in Workspace {workspace_id}")]
    RepositoryAlreadyInWorkspace {
        repository_id: i64,
        workspace_id: i64,
    },
    #[error("Worktree {worktree_id} does not exist")]
    WorktreeNotFound { worktree_id: i64 },
    #[error("Repository {repository_id} already has a Worktree in Workspace {workspace_id}")]
    WorktreeAlreadyExists {
        repository_id: i64,
        workspace_id: i64,
    },
    #[error("Worktree Repository {repository_id} is not selected in Workspace {workspace_id}")]
    WorktreeRepositoryNotSelected {
        repository_id: i64,
        workspace_id: i64,
    },
    #[error("a Worktree path cannot be blank")]
    EmptyWorktreePath,
    #[error("a Machine name cannot be blank")]
    EmptyMachineName,
    #[error("a Machine socket name cannot be blank")]
    EmptyMachineSocketName,
    #[error("Machine name already exists in Context {context_id}: {name}")]
    MachineNameTaken { context_id: i64, name: String },
    #[error("Machine {machine_id} does not exist")]
    MachineNotFound { machine_id: i64 },
    #[error("Machine {machine_id} belongs to another Context")]
    MachineContextMismatch { machine_id: i64, context_id: i64 },
    #[error("Machine {machine_id} has active Runs: {run_ids:?}")]
    MachineHasActiveRuns { machine_id: i64, run_ids: Vec<i64> },
    #[error("a remote Machine host cannot be blank")]
    EmptyMachineHost,
    #[error("a remote Machine host contains unsupported characters")]
    InvalidMachineHost,
    #[error("a remote Machine user cannot be blank")]
    EmptyMachineUser,
    #[error("a remote Machine user contains unsupported characters")]
    InvalidMachineUser,
    #[error("a remote Machine SSH port must be positive")]
    InvalidMachinePort,
    #[error("a remote Machine host-key checking mode is unsupported")]
    InvalidMachineHostKeyChecking,
    #[error("a Run prompt cannot be blank")]
    EmptyRunPrompt,
    #[error("a Run working directory cannot be blank")]
    EmptyRunWorkingDirectory,
    #[error("Direct Run must include at least one Repository")]
    EmptyDirectRunCheckouts,
    #[error("Direct Run checkout for Repository {repository_id} is invalid")]
    InvalidDirectRunCheckout { repository_id: i64 },
    #[error("Direct Run must choose a primary Repository")]
    MissingDirectRunRepository,
    #[error("Direct Run has dirty checkouts for Repositories {repository_ids:?}; confirm the warning before starting")]
    DirectRunDirtyCheckouts { repository_ids: Vec<i64> },
    #[error("Direct Run shares checkout paths with active Runs {run_ids:?}: {paths:?}; confirm the shared checkout warning before starting")]
    DirectRunSharedCheckouts {
        run_ids: Vec<i64>,
        paths: Vec<String>,
    },
    #[error("a Run session name cannot be blank")]
    EmptyRunSessionName,
    #[error("a Run Pane identity cannot be blank")]
    EmptyRunPaneId,
    #[error("Run {run_id} does not exist")]
    RunNotFound { run_id: i64 },
    #[error("Run {run_id} is {state:?}; stop it and wait for Finished state before deleting")]
    RunNotFinished { run_id: i64, state: RunState },
    #[error(
        "a Run is already attached to Machine {machine_id}, session {session_name}, Pane {pane_id}"
    )]
    RunAlreadyAttached {
        machine_id: i64,
        session_name: String,
        pane_id: String,
    },
    #[error("External Object {external_object_id} is not linked to Item {item_id}")]
    RunPromptSourceNotLinked {
        external_object_id: i64,
        item_id: i64,
    },
    #[error("Repository {repository_id} was selected more than once")]
    DuplicateRepositorySelection { repository_id: i64 },
    #[error("Item {item_id} does not exist")]
    ItemNotFound { item_id: i64 },
    #[error("Item {item_id} has active Runs: {run_ids:?}")]
    ItemHasActiveRuns { item_id: i64, run_ids: Vec<i64> },
    #[error("Items {from_item_id} and {to_item_id} belong to different Contexts")]
    ItemContextMismatch { from_item_id: i64, to_item_id: i64 },
    #[error("an Item cannot relate to itself: {item_id}")]
    SelfRelation { item_id: i64 },
    #[error("the relationship already exists")]
    RelationAlreadyExists,
    #[error("the Item identifier sequence is exhausted")]
    SequenceExhausted,
    #[error("an external URL cannot be blank")]
    EmptyExternalUrl,
    #[error("an external object key cannot be blank")]
    EmptyExternalObjectKey,
    #[error("External Object {external_object_id} does not exist")]
    ExternalObjectNotFound { external_object_id: i64 },
    #[error("the Link already exists")]
    LinkAlreadyExists,
    #[error("Link {link_id} does not exist")]
    LinkNotFound { link_id: i64 },
    #[error("Reminder {reminder_id} does not exist on Item {item_id}")]
    ReminderNotFound { item_id: i64, reminder_id: i64 },
}

/// Stores machine paths in a stable form while keeping paths under the machine home readable.
pub fn normalize_machine_path(input: &str, machine_home: &str) -> Result<String, DomainError> {
    let input = input.trim().trim_end_matches('/');
    if input.is_empty() {
        return Err(DomainError::EmptyRepositoryCheckoutPath);
    }
    if input == "~" || input.starts_with("~/") {
        return Ok(input.to_owned());
    }

    let home = machine_home.trim().trim_end_matches('/');
    if !home.is_empty() && (input == home || input.starts_with(&format!("{home}/"))) {
        let relative = input[home.len()..].trim_start_matches('/');
        return Ok(if relative.is_empty() {
            "~".into()
        } else {
            format!("~/{relative}")
        });
    }

    if input.starts_with('/') {
        Ok(input.to_owned())
    } else {
        Ok(format!("~/{input}"))
    }
}

pub fn decide(mut state: DomainState, event: Event) -> Result<Decision, DomainError> {
    match event {
        Event::CreateContext { name } => {
            let name = clean_name(name, DomainError::EmptyContextName)?;
            if state.contexts.iter().any(|context| context.name == name) {
                return Err(DomainError::ContextNameTaken { name });
            }

            let id = state.next_context_id;
            let next_context_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let project_id = state.next_project_id;
            let next_project_id = project_id
                .checked_add(1)
                .ok_or(DomainError::SequenceExhausted)?;
            let context = Context { id, name };
            let project = Project {
                id: project_id,
                context_id: id,
                name: "Default".into(),
                defaults: ProjectDefaults::default(),
            };

            state.next_context_id = next_context_id;
            state.next_project_id = next_project_id;
            state.contexts.push(context.clone());
            state.projects.push(project.clone());

            Ok(Decision {
                state,
                effects: vec![
                    Effect::PersistContext {
                        context,
                        next_context_id,
                    },
                    Effect::PersistProject {
                        project,
                        next_project_id,
                    },
                ],
            })
        }
        Event::CreateProject {
            context_id,
            name,
            defaults,
        } => {
            let name = clean_name(name, DomainError::EmptyProjectName)?;
            ensure_context(&state, context_id)?;
            if state
                .projects
                .iter()
                .any(|project| project.context_id == context_id && project.name == name)
            {
                return Err(DomainError::ProjectNameTaken { context_id, name });
            }

            let id = state.next_project_id;
            let next_project_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let project = Project {
                id,
                context_id,
                name,
                defaults,
            };

            state.next_project_id = next_project_id;
            state.projects.push(project.clone());

            Ok(Decision {
                state,
                effects: vec![Effect::PersistProject {
                    project,
                    next_project_id,
                }],
            })
        }
        Event::RegisterRepository {
            project_id,
            name,
            remote_url,
        } => {
            let name = clean_name(name, DomainError::EmptyRepositoryName)?;
            if name == "." || name == ".." || name.contains('/') || name.contains('\\') {
                return Err(DomainError::InvalidRepositoryName);
            }
            let remote_url = clean_name(remote_url, DomainError::EmptyRepositoryRemoteUrl)?;
            let project = state
                .projects
                .iter()
                .find(|project| project.id == project_id)
                .ok_or(DomainError::ProjectNotFound { project_id })?;
            if state
                .repositories
                .iter()
                .any(|repository| repository.project_id == project.id && repository.name == name)
            {
                return Err(DomainError::RepositoryNameTaken { project_id, name });
            }

            let id = state.next_repository_id;
            let next_repository_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let repository = Repository {
                id,
                project_id,
                name,
                remote_url,
                base_branch: "main".into(),
            };
            state.next_repository_id = next_repository_id;
            state.repositories.push(repository.clone());

            Ok(Decision {
                state,
                effects: vec![Effect::PersistRepository {
                    repository,
                    next_repository_id,
                }],
            })
        }
        Event::RegisterRepositoryAtLocation {
            project_id,
            name,
            remote_url,
            base_branch,
            machine_id,
            checkout_path,
            worktree_root,
        } => {
            let name = clean_repository_name(name)?;
            let remote_url = clean_name(remote_url, DomainError::EmptyRepositoryRemoteUrl)?;
            let base_branch = clean_name(base_branch, DomainError::EmptyRepositoryBaseBranch)?;
            let project = state
                .projects
                .iter()
                .find(|project| project.id == project_id)
                .ok_or(DomainError::ProjectNotFound { project_id })?;
            let machine = state
                .machines
                .iter()
                .find(|machine| machine.id == machine_id)
                .ok_or(DomainError::MachineNotFound { machine_id })?;
            if machine.context_id != project.context_id {
                return Err(DomainError::MachineContextMismatch {
                    machine_id,
                    context_id: project.context_id,
                });
            }
            let checkout_path =
                clean_name(checkout_path, DomainError::EmptyRepositoryCheckoutPath)?;
            let worktree_root =
                clean_name(worktree_root, DomainError::EmptyRepositoryWorktreeRoot)?;
            let location = RepositoryLocation {
                repository_id: 0,
                machine_id,
                checkout_path,
                worktree_root,
            };

            if let Some(repository) = state
                .repositories
                .iter_mut()
                .find(|repository| repository.project_id == project_id && repository.name == name)
            {
                if repository.remote_url != remote_url {
                    return Err(DomainError::RepositoryRemoteMismatch { project_id, name });
                }
                if state.repository_locations.iter().any(|candidate| {
                    candidate.repository_id == repository.id && candidate.machine_id == machine_id
                }) {
                    return Err(DomainError::RepositoryLocationAlreadyExists {
                        repository_id: repository.id,
                        machine_id,
                    });
                }
                repository.base_branch = base_branch;
                let repository = repository.clone();
                let location = RepositoryLocation {
                    repository_id: repository.id,
                    ..location
                };
                state.repository_locations.push(location.clone());
                return Ok(Decision {
                    state,
                    effects: vec![
                        Effect::UpdateRepository { repository },
                        Effect::PersistRepositoryLocation { location },
                    ],
                });
            }

            let id = state.next_repository_id;
            let next_repository_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let repository = Repository {
                id,
                project_id,
                name,
                remote_url,
                base_branch,
            };
            let location = RepositoryLocation {
                repository_id: id,
                ..location
            };
            state.next_repository_id = next_repository_id;
            state.repositories.push(repository.clone());
            state.repository_locations.push(location.clone());

            Ok(Decision {
                state,
                effects: vec![
                    Effect::PersistRepository {
                        repository,
                        next_repository_id,
                    },
                    Effect::PersistRepositoryLocation { location },
                ],
            })
        }
        Event::ConfigureRepositoryLocation {
            repository_id,
            machine_id,
            checkout_path,
            worktree_root,
        } => {
            let repository = state
                .repositories
                .iter()
                .find(|repository| repository.id == repository_id)
                .ok_or(DomainError::RepositoryNotFound { repository_id })?;
            let project = state
                .projects
                .iter()
                .find(|project| project.id == repository.project_id)
                .ok_or(DomainError::ProjectNotFound {
                    project_id: repository.project_id,
                })?;
            let machine = state
                .machines
                .iter()
                .find(|machine| machine.id == machine_id)
                .ok_or(DomainError::MachineNotFound { machine_id })?;
            if machine.context_id != project.context_id {
                return Err(DomainError::MachineContextMismatch {
                    machine_id,
                    context_id: project.context_id,
                });
            }
            if state.repository_locations.iter().any(|candidate| {
                candidate.repository_id == repository_id && candidate.machine_id == machine_id
            }) {
                return Err(DomainError::RepositoryLocationAlreadyExists {
                    repository_id,
                    machine_id,
                });
            }
            let location = RepositoryLocation {
                repository_id,
                machine_id,
                checkout_path: clean_name(checkout_path, DomainError::EmptyRepositoryCheckoutPath)?,
                worktree_root: clean_name(worktree_root, DomainError::EmptyRepositoryWorktreeRoot)?,
            };
            state.repository_locations.push(location.clone());
            Ok(Decision {
                state,
                effects: vec![Effect::PersistRepositoryLocation { location }],
            })
        }
        Event::ResetLocalData => {
            let active_run_ids = state
                .runs
                .iter()
                .filter(|run| run.state != RunState::Finished)
                .map(|run| run.id)
                .collect::<Vec<_>>();
            if !active_run_ids.is_empty() {
                return Err(DomainError::ResetHasActiveRuns {
                    run_ids: active_run_ids,
                });
            }
            let context_id = state.next_context_id;
            let next_context_id = context_id
                .checked_add(1)
                .ok_or(DomainError::SequenceExhausted)?;
            let project_id = state.next_project_id;
            let next_project_id = project_id
                .checked_add(1)
                .ok_or(DomainError::SequenceExhausted)?;
            let context = Context {
                id: context_id,
                name: "Personal".into(),
            };
            let project = Project {
                id: project_id,
                context_id,
                name: "Default".into(),
                defaults: ProjectDefaults::default(),
            };

            state.next_context_id = next_context_id;
            state.next_project_id = next_project_id;
            state.contexts = vec![context.clone()];
            state.projects = vec![project.clone()];
            state.repositories.clear();
            state.repository_locations.clear();
            state.items.clear();
            state.workspaces.clear();
            state.worktrees.clear();
            state.machines.clear();
            state.runs.clear();
            state.relationships.clear();
            state.external_objects.clear();
            state.links.clear();
            state.snapshots.clear();
            state.activities.clear();
            state.attention_defaults.clear();

            Ok(Decision {
                state,
                effects: vec![Effect::ResetLocalData {
                    context,
                    project,
                    next_context_id,
                    next_project_id,
                }],
            })
        }
        Event::DeleteRepository {
            repository_id,
            workspace_ids,
        } => {
            let plan = plan_repository_deletion(&state, repository_id)?;
            let mut expected_workspace_ids = plan
                .workspaces
                .iter()
                .map(|workspace| workspace.id)
                .collect::<Vec<_>>();
            let mut provided_workspace_ids = workspace_ids;
            expected_workspace_ids.sort_unstable();
            provided_workspace_ids.sort_unstable();
            if expected_workspace_ids != provided_workspace_ids {
                return Err(DomainError::RepositoryWorkspacesMismatch {
                    repository_id,
                    expected_workspace_ids,
                    provided_workspace_ids,
                });
            }
            if let Some(workspace_id) = plan.workspaces.iter().find_map(|workspace| {
                state
                    .runs
                    .iter()
                    .any(|run| run.workspace_id == Some(workspace.id))
                    .then_some(workspace.id)
            }) {
                return Err(DomainError::WorkspaceHasRuns { workspace_id });
            }

            for workspace in &plan.workspaces {
                state
                    .worktrees
                    .retain(|worktree| worktree.workspace_id != workspace.id);
                state
                    .workspaces
                    .retain(|candidate| candidate.id != workspace.id);
            }
            state
                .repositories
                .retain(|repository| repository.id != repository_id);
            state
                .repository_locations
                .retain(|location| location.repository_id != repository_id);

            let mut effects = plan
                .workspaces
                .iter()
                .map(|workspace| Effect::RemoveWorkspace {
                    workspace_id: workspace.id,
                })
                .collect::<Vec<_>>();
            effects.push(Effect::RemoveRepository { repository_id });

            Ok(Decision { state, effects })
        }
        Event::DeleteProject {
            project_id,
            item_ids,
            repository_ids,
            workspace_ids,
        } => {
            let plan = plan_project_deletion(&state, project_id)?;
            if !parent_selection_matches(
                &plan.items.iter().map(|item| item.id).collect::<Vec<_>>(),
                item_ids,
            ) || !parent_selection_matches(
                &plan
                    .repositories
                    .iter()
                    .map(|repository| repository.id)
                    .collect::<Vec<_>>(),
                repository_ids,
            ) || !parent_selection_matches(
                &plan
                    .workspaces
                    .iter()
                    .map(|workspace| workspace.id)
                    .collect::<Vec<_>>(),
                workspace_ids,
            ) {
                return Err(DomainError::ProjectDeletionPlanMismatch { project_id });
            }
            if !plan.active_run_ids.is_empty() {
                return Err(DomainError::ProjectHasActiveRuns {
                    project_id,
                    run_ids: plan.active_run_ids.clone(),
                });
            }

            let summary = plan.summary();
            let orphaned_external_object_ids = plan.orphaned_external_object_ids.clone();
            let item_ids = plan.items.iter().map(|item| item.id).collect::<Vec<_>>();
            state.items.retain(|item| !item_ids.contains(&item.id));
            state.worktrees.retain(|worktree| {
                !plan
                    .workspaces
                    .iter()
                    .any(|workspace| workspace.id == worktree.workspace_id)
            });
            state.workspaces.retain(|workspace| {
                !plan
                    .workspaces
                    .iter()
                    .any(|candidate| candidate.id == workspace.id)
            });
            state
                .runs
                .retain(|run| !plan.runs.iter().any(|candidate| candidate.id == run.id));
            state.relationships.retain(|relation| {
                !item_ids.contains(&relation.from_item_id)
                    && !item_ids.contains(&relation.to_item_id)
            });
            state.links.retain(|link| !item_ids.contains(&link.item_id));
            state
                .external_objects
                .retain(|object| !plan.orphaned_external_object_ids.contains(&object.id));
            state.snapshots.retain(|snapshot| {
                !plan
                    .orphaned_external_object_ids
                    .contains(&snapshot.external_object_id)
            });
            state.activities.retain(|activity| {
                !plan
                    .orphaned_external_object_ids
                    .contains(&activity.external_object_id)
            });
            state.repositories.retain(|repository| {
                !plan
                    .repositories
                    .iter()
                    .any(|candidate| candidate.id == repository.id)
            });
            let repository_ids = plan
                .repositories
                .iter()
                .map(|repository| repository.id)
                .collect::<Vec<_>>();
            state
                .repository_locations
                .retain(|location| !repository_ids.contains(&location.repository_id));
            state.projects.retain(|project| project.id != project_id);

            Ok(Decision {
                state,
                effects: vec![Effect::RemoveProjectCascade {
                    project_id,
                    item_ids,
                    repository_ids: plan
                        .repositories
                        .iter()
                        .map(|repository| repository.id)
                        .collect(),
                    workspace_ids: plan
                        .workspaces
                        .iter()
                        .map(|workspace| workspace.id)
                        .collect(),
                    orphaned_external_object_ids,
                    summary,
                }],
            })
        }
        Event::DeleteContext {
            context_id,
            project_ids,
            item_ids,
            repository_ids,
            workspace_ids,
            machine_ids,
        } => {
            let plan = plan_context_deletion(&state, context_id)?;
            if state.contexts.len() == 1 {
                return Err(DomainError::CannotDeleteLastContext);
            }
            if !parent_selection_matches(
                &plan
                    .projects
                    .iter()
                    .map(|project| project.id)
                    .collect::<Vec<_>>(),
                project_ids,
            ) || !parent_selection_matches(
                &plan.items.iter().map(|item| item.id).collect::<Vec<_>>(),
                item_ids,
            ) || !parent_selection_matches(
                &plan
                    .repositories
                    .iter()
                    .map(|repository| repository.id)
                    .collect::<Vec<_>>(),
                repository_ids,
            ) || !parent_selection_matches(
                &plan
                    .workspaces
                    .iter()
                    .map(|workspace| workspace.id)
                    .collect::<Vec<_>>(),
                workspace_ids,
            ) || !parent_selection_matches(
                &plan
                    .machines
                    .iter()
                    .map(|machine| machine.id)
                    .collect::<Vec<_>>(),
                machine_ids,
            ) {
                return Err(DomainError::ContextDeletionPlanMismatch { context_id });
            }
            if !plan.active_run_ids.is_empty() {
                return Err(DomainError::ContextHasActiveRuns {
                    context_id,
                    run_ids: plan.active_run_ids.clone(),
                });
            }

            let summary = plan.summary();
            let orphaned_external_object_ids = plan.orphaned_external_object_ids.clone();
            let item_ids = plan.items.iter().map(|item| item.id).collect::<Vec<_>>();
            let project_ids = plan
                .projects
                .iter()
                .map(|project| project.id)
                .collect::<Vec<_>>();
            let machine_ids = plan
                .machines
                .iter()
                .map(|machine| machine.id)
                .collect::<Vec<_>>();
            state.contexts.retain(|context| context.id != context_id);
            state
                .projects
                .retain(|project| !project_ids.contains(&project.id));
            state.items.retain(|item| !item_ids.contains(&item.id));
            state.repositories.retain(|repository| {
                !plan
                    .repositories
                    .iter()
                    .any(|candidate| candidate.id == repository.id)
            });
            let repository_ids = plan
                .repositories
                .iter()
                .map(|repository| repository.id)
                .collect::<Vec<_>>();
            state
                .repository_locations
                .retain(|location| !repository_ids.contains(&location.repository_id));
            state.worktrees.retain(|worktree| {
                !plan
                    .workspaces
                    .iter()
                    .any(|workspace| workspace.id == worktree.workspace_id)
            });
            state.workspaces.retain(|workspace| {
                !plan
                    .workspaces
                    .iter()
                    .any(|candidate| candidate.id == workspace.id)
            });
            state
                .runs
                .retain(|run| !plan.runs.iter().any(|candidate| candidate.id == run.id));
            state
                .machines
                .retain(|machine| !machine_ids.contains(&machine.id));
            state.relationships.retain(|relation| {
                !item_ids.contains(&relation.from_item_id)
                    && !item_ids.contains(&relation.to_item_id)
            });
            state.links.retain(|link| !item_ids.contains(&link.item_id));
            state
                .external_objects
                .retain(|object| !plan.orphaned_external_object_ids.contains(&object.id));
            state.snapshots.retain(|snapshot| {
                !plan
                    .orphaned_external_object_ids
                    .contains(&snapshot.external_object_id)
            });
            state.activities.retain(|activity| {
                !plan
                    .orphaned_external_object_ids
                    .contains(&activity.external_object_id)
            });
            state
                .attention_defaults
                .retain(|attention_default| attention_default.context_id != context_id);

            Ok(Decision {
                state,
                effects: vec![Effect::RemoveContextCascade {
                    context_id,
                    project_ids,
                    item_ids,
                    repository_ids: plan
                        .repositories
                        .iter()
                        .map(|repository| repository.id)
                        .collect(),
                    workspace_ids: plan
                        .workspaces
                        .iter()
                        .map(|workspace| workspace.id)
                        .collect(),
                    machine_ids,
                    orphaned_external_object_ids,
                    summary,
                }],
            })
        }
        Event::DeleteMachine {
            machine_id,
            run_ids,
        } => {
            let plan = plan_machine_deletion(&state, machine_id)?;
            let mut expected_run_ids = plan.runs.iter().map(|run| run.id).collect::<Vec<_>>();
            let mut provided_run_ids = run_ids;
            expected_run_ids.sort_unstable();
            provided_run_ids.sort_unstable();
            if expected_run_ids != provided_run_ids {
                return Err(DomainError::MachineRunsMismatch {
                    machine_id,
                    expected_run_ids,
                    provided_run_ids,
                });
            }
            if !plan.active_run_ids.is_empty() {
                return Err(DomainError::MachineHasActiveRuns {
                    machine_id,
                    run_ids: plan.active_run_ids,
                });
            }

            state.runs.retain(|run| run.machine_id != machine_id);
            state.machines.retain(|machine| machine.id != machine_id);
            state
                .repository_locations
                .retain(|location| location.machine_id != machine_id);
            let mut effects = plan
                .runs
                .iter()
                .map(|run| Effect::RemoveRun { run_id: run.id })
                .collect::<Vec<_>>();
            effects.push(Effect::RemoveMachine { machine_id });

            Ok(Decision { state, effects })
        }
        Event::CreateItem {
            title,
            context_id,
            project_id,
        } => {
            if title.trim().is_empty() {
                return Err(DomainError::EmptyTitle);
            }
            ensure_context(&state, context_id)?;
            let project = state
                .projects
                .iter()
                .find(|project| project.id == project_id)
                .ok_or(DomainError::ProjectNotFound { project_id })?;
            if project.context_id != context_id {
                return Err(DomainError::ProjectContextMismatch {
                    project_id,
                    context_id,
                });
            }

            let id = state.next_item_id;
            let number = state.next_item_number;
            let next_item_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let next_item_number = number
                .checked_add(1)
                .ok_or(DomainError::SequenceExhausted)?;
            let item = Item {
                id,
                human_identifier: format!("MC-{number}"),
                title: title.trim().to_owned(),
                project_id,
                status: project.defaults.item_status,
                notes: String::new(),
                reminders: Vec::new(),
            };

            state.next_item_id = next_item_id;
            state.next_item_number = next_item_number;
            state.items.push(item.clone());

            Ok(Decision {
                state,
                effects: vec![Effect::PersistItem {
                    item,
                    next_item_number,
                    next_item_id,
                }],
            })
        }
        Event::CreateWorkspace {
            item_id,
            repositories,
        } => {
            let project_id = item_project_id(&state, item_id)?;
            if repositories.is_empty() {
                return Err(DomainError::EmptyWorkspaceRepositories);
            }
            let repositories = normalize_workspace_repositories(&state, project_id, repositories)?;
            let id = state.next_workspace_id;
            let next_workspace_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let workspace = Workspace {
                id,
                item_id,
                repositories,
                preparation_state: WorkspacePreparationState::Pending,
            };
            state.next_workspace_id = next_workspace_id;
            state.workspaces.push(workspace.clone());

            Ok(Decision {
                state,
                effects: vec![Effect::PersistWorkspace {
                    workspace,
                    next_workspace_id,
                }],
            })
        }
        Event::CreateWorktree {
            workspace_id,
            repository_id,
            machine_id,
            path,
            branch,
            base_branch,
            is_dirty,
        } => {
            let workspace = state
                .workspaces
                .iter()
                .find(|workspace| workspace.id == workspace_id)
                .ok_or(DomainError::WorkspaceNotFound { workspace_id })?;
            if !workspace
                .repositories
                .iter()
                .any(|repository| repository.repository_id == repository_id)
            {
                return Err(DomainError::WorktreeRepositoryNotSelected {
                    repository_id,
                    workspace_id,
                });
            }
            if state.worktrees.iter().any(|worktree| {
                worktree.workspace_id == workspace_id && worktree.repository_id == repository_id
            }) {
                return Err(DomainError::WorktreeAlreadyExists {
                    repository_id,
                    workspace_id,
                });
            }
            let item_context_id = item_context_id(&state, workspace.item_id)?;
            let machine = state
                .machines
                .iter()
                .find(|machine| machine.id == machine_id)
                .ok_or(DomainError::MachineNotFound { machine_id })?;
            if machine.context_id != item_context_id {
                return Err(DomainError::MachineContextMismatch {
                    machine_id,
                    context_id: item_context_id,
                });
            }
            let path = clean_name(path, DomainError::EmptyWorktreePath)?;
            let branch = clean_name(branch, DomainError::EmptyBranch)?;
            let base_branch = clean_name(base_branch, DomainError::EmptyBranch)?;
            let id = state.next_worktree_id;
            let next_worktree_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let worktree = Worktree {
                id,
                workspace_id,
                repository_id,
                machine_id,
                path,
                branch,
                base_branch,
                is_dirty,
            };
            state.next_worktree_id = next_worktree_id;
            state.worktrees.push(worktree.clone());

            let preparation_state =
                workspace_preparation_state(&state, workspace_id, workspace.preparation_state);
            let workspace = state
                .workspaces
                .iter_mut()
                .find(|workspace| workspace.id == workspace_id)
                .expect("the Workspace was checked above");
            let workspace_changed = workspace.preparation_state != preparation_state;
            workspace.preparation_state = preparation_state;
            let workspace = workspace.clone();
            let mut effects = vec![Effect::PersistWorktree {
                worktree,
                next_worktree_id,
            }];
            if workspace_changed {
                effects.push(Effect::PersistWorkspaceUpdate { workspace });
            }

            Ok(Decision { state, effects })
        }
        Event::MarkWorkspaceResumable { workspace_id } => {
            let workspace = state
                .workspaces
                .iter_mut()
                .find(|workspace| workspace.id == workspace_id)
                .ok_or(DomainError::WorkspaceNotFound { workspace_id })?;
            workspace.preparation_state = WorkspacePreparationState::Resumable;
            let workspace = workspace.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistWorkspaceUpdate { workspace }],
            })
        }
        Event::RemoveWorktree { worktree_id } => {
            let position = state
                .worktrees
                .iter()
                .position(|worktree| worktree.id == worktree_id)
                .ok_or(DomainError::WorktreeNotFound { worktree_id })?;
            let workspace_id = state.worktrees[position].workspace_id;
            state.worktrees.remove(position);
            let previous_workspace = state
                .workspaces
                .iter()
                .find(|workspace| workspace.id == workspace_id)
                .cloned()
                .ok_or(DomainError::WorkspaceNotFound { workspace_id })?;
            let preparation_state = workspace_preparation_state(
                &state,
                workspace_id,
                previous_workspace.preparation_state,
            );
            let workspace = state
                .workspaces
                .iter_mut()
                .find(|workspace| workspace.id == workspace_id)
                .expect("the Workspace was checked above");
            workspace.preparation_state = preparation_state;
            let workspace = workspace.clone();

            Ok(Decision {
                state,
                effects: vec![
                    Effect::RemoveWorktree { worktree_id },
                    Effect::PersistWorkspaceUpdate { workspace },
                ],
            })
        }
        Event::RemoveWorkspace { workspace_id } => {
            if !state
                .workspaces
                .iter()
                .any(|workspace| workspace.id == workspace_id)
            {
                return Err(DomainError::WorkspaceNotFound { workspace_id });
            }
            if state
                .runs
                .iter()
                .any(|run| run.workspace_id == Some(workspace_id))
            {
                return Err(DomainError::WorkspaceHasRuns { workspace_id });
            }
            state
                .worktrees
                .retain(|worktree| worktree.workspace_id != workspace_id);
            state
                .workspaces
                .retain(|workspace| workspace.id != workspace_id);

            Ok(Decision {
                state,
                effects: vec![Effect::RemoveWorkspace { workspace_id }],
            })
        }
        Event::RegisterMachine {
            context_id,
            name,
            socket_name,
            transport,
        } => {
            ensure_context(&state, context_id)?;
            let name = clean_name(name, DomainError::EmptyMachineName)?;
            let socket_name = clean_name(socket_name, DomainError::EmptyMachineSocketName)?;
            let transport = clean_machine_transport(transport)?;
            if state
                .machines
                .iter()
                .any(|machine| machine.context_id == context_id && machine.name == name)
            {
                return Err(DomainError::MachineNameTaken { context_id, name });
            }
            let id = state.next_machine_id;
            let next_machine_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let machine = Machine {
                id,
                context_id,
                name,
                socket_name,
                transport,
                last_observed: MachineObservation::Unknown,
                last_observed_at: None,
            };
            state.next_machine_id = next_machine_id;
            state.machines.push(machine.clone());
            Ok(Decision {
                state,
                effects: vec![Effect::PersistMachine {
                    machine,
                    next_machine_id,
                }],
            })
        }
        Event::ObserveMachine {
            machine_id,
            observation,
            observed_at,
        } => {
            let machine = state
                .machines
                .iter_mut()
                .find(|machine| machine.id == machine_id)
                .ok_or(DomainError::MachineNotFound { machine_id })?;
            machine.last_observed = observation;
            machine.last_observed_at = Some(observed_at);
            let machine = machine.clone();
            Ok(Decision {
                state,
                effects: vec![Effect::PersistMachineObservation { machine }],
            })
        }
        Event::DeleteItem { item_id } => {
            let plan = plan_item_deletion(&state, item_id)?;
            if !plan.active_run_ids.is_empty() {
                return Err(DomainError::ItemHasActiveRuns {
                    item_id,
                    run_ids: plan.active_run_ids,
                });
            }
            let summary = plan.summary();
            let orphaned_external_object_ids = plan.orphaned_external_object_ids.clone();
            state.items.retain(|item| item.id != item_id);
            state.worktrees.retain(|worktree| {
                !state.workspaces.iter().any(|workspace| {
                    workspace.id == worktree.workspace_id && workspace.item_id == item_id
                })
            });
            state
                .workspaces
                .retain(|workspace| workspace.item_id != item_id);
            state.runs.retain(|run| run.item_id != item_id);
            state.relationships.retain(|relation| {
                relation.from_item_id != item_id && relation.to_item_id != item_id
            });
            state.links.retain(|link| link.item_id != item_id);
            state
                .external_objects
                .retain(|object| !orphaned_external_object_ids.contains(&object.id));
            state.snapshots.retain(|snapshot| {
                !orphaned_external_object_ids.contains(&snapshot.external_object_id)
            });
            state.activities.retain(|activity| {
                !orphaned_external_object_ids.contains(&activity.external_object_id)
            });

            Ok(Decision {
                state,
                effects: vec![Effect::RemoveItemCascade {
                    item_id,
                    orphaned_external_object_ids,
                    summary,
                }],
            })
        }
        Event::DeleteLink { link_id } => {
            let link = state
                .links
                .iter()
                .find(|link| link.id == link_id)
                .cloned()
                .ok_or(DomainError::LinkNotFound { link_id })?;
            let external_object_id = link.external_object_id;
            state.links.retain(|candidate| candidate.id != link_id);
            let orphaned = !state
                .links
                .iter()
                .any(|candidate| candidate.external_object_id == external_object_id);
            if orphaned {
                state
                    .external_objects
                    .retain(|object| object.id != external_object_id);
                state
                    .snapshots
                    .retain(|snapshot| snapshot.external_object_id != external_object_id);
                state
                    .activities
                    .retain(|activity| activity.external_object_id != external_object_id);
            }

            let mut effects = vec![Effect::RemoveLink {
                link_id,
                external_object_id,
            }];
            if orphaned {
                effects.push(Effect::RemoveExternalObject { external_object_id });
            }
            Ok(Decision { state, effects })
        }
        Event::DeleteExternalObject { external_object_id } => {
            plan_external_object_deletion(&state, external_object_id)?;
            state
                .links
                .retain(|link| link.external_object_id != external_object_id);
            state
                .external_objects
                .retain(|object| object.id != external_object_id);
            state
                .snapshots
                .retain(|snapshot| snapshot.external_object_id != external_object_id);
            state
                .activities
                .retain(|activity| activity.external_object_id != external_object_id);

            Ok(Decision {
                state,
                effects: vec![Effect::RemoveExternalObject { external_object_id }],
            })
        }
        Event::DeleteRun { run_id } => {
            let position = state
                .runs
                .iter()
                .position(|run| run.id == run_id)
                .ok_or(DomainError::RunNotFound { run_id })?;
            let run_state = state.runs[position].state;
            if run_state != RunState::Finished {
                return Err(DomainError::RunNotFinished {
                    run_id,
                    state: run_state,
                });
            }
            state.runs.remove(position);

            Ok(Decision {
                state,
                effects: vec![Effect::RemoveRun { run_id }],
            })
        }
        Event::StartDirectRun {
            item_id,
            workspace_id,
            machine_id,
            agent,
            execution_profile,
            prompt,
            working_directory,
            session_name,
            pane_id,
            started_at,
            prompt_selection,
            checkouts,
            repository_id,
            allow_dirty,
            allow_shared_checkouts,
        } => {
            let context_id = item_context_id(&state, item_id)?;
            let workspace = state
                .workspaces
                .iter()
                .find(|workspace| workspace.id == workspace_id)
                .ok_or(DomainError::WorkspaceNotFound { workspace_id })?;
            if workspace.item_id != item_id {
                return Err(DomainError::WorkspaceItemMismatch {
                    workspace_id,
                    item_id,
                });
            }
            let machine = state
                .machines
                .iter()
                .find(|machine| machine.id == machine_id)
                .ok_or(DomainError::MachineNotFound { machine_id })?;
            if machine.context_id != context_id {
                return Err(DomainError::MachineContextMismatch {
                    machine_id,
                    context_id,
                });
            }
            if checkouts.is_empty() {
                return Err(DomainError::EmptyDirectRunCheckouts);
            }
            if !workspace
                .repositories
                .iter()
                .any(|selected| selected.repository_id == repository_id)
            {
                return Err(DomainError::RunRepositoryNotSelected {
                    repository_id,
                    workspace_id,
                });
            }
            let selected_repository_ids = workspace
                .repositories
                .iter()
                .map(|repository| repository.repository_id)
                .collect::<Vec<_>>();
            let mut checkout_repository_ids = Vec::new();
            for checkout in &checkouts {
                if checkout.path.trim().is_empty()
                    || checkout.branch.trim().is_empty()
                    || !selected_repository_ids.contains(&checkout.repository_id)
                    || checkout_repository_ids.contains(&checkout.repository_id)
                {
                    return Err(DomainError::InvalidDirectRunCheckout {
                        repository_id: checkout.repository_id,
                    });
                }
                checkout_repository_ids.push(checkout.repository_id);
            }
            if checkout_repository_ids.len() != selected_repository_ids.len() {
                return Err(DomainError::InvalidDirectRunCheckout {
                    repository_id: selected_repository_ids
                        .into_iter()
                        .find(|repository_id| !checkout_repository_ids.contains(repository_id))
                        .unwrap_or_default(),
                });
            }
            for external_object_id in prompt_selection.external_object_ids {
                if !state.links.iter().any(|link| {
                    link.item_id == item_id && link.external_object_id == external_object_id
                }) {
                    return Err(DomainError::RunPromptSourceNotLinked {
                        external_object_id,
                        item_id,
                    });
                }
            }
            let dirty_repository_ids = checkouts
                .iter()
                .filter(|checkout| checkout.is_dirty)
                .map(|checkout| checkout.repository_id)
                .collect::<Vec<_>>();
            if !allow_dirty && !dirty_repository_ids.is_empty() {
                return Err(DomainError::DirectRunDirtyCheckouts {
                    repository_ids: dirty_repository_ids,
                });
            }
            let mut shared_run_ids = Vec::new();
            let mut shared_paths = Vec::new();
            for active_run in state.runs.iter().filter(|run| {
                run.machine_id == machine_id
                    && run.state != RunState::Finished
                    && run.pane_status != RunPaneStatus::Missing
            }) {
                for checkout in &checkouts {
                    if active_run
                        .direct_checkouts
                        .iter()
                        .any(|active_checkout| active_checkout.path == checkout.path)
                    {
                        shared_run_ids.push(active_run.id);
                        shared_paths.push(checkout.path.clone());
                    }
                }
            }
            shared_run_ids.sort_unstable();
            shared_run_ids.dedup();
            shared_paths.sort();
            shared_paths.dedup();
            if !allow_shared_checkouts && !shared_run_ids.is_empty() {
                return Err(DomainError::DirectRunSharedCheckouts {
                    run_ids: shared_run_ids,
                    paths: shared_paths,
                });
            }
            let prompt = clean_name(prompt, DomainError::EmptyRunPrompt)?;
            let working_directory =
                clean_name(working_directory, DomainError::EmptyRunWorkingDirectory)?;
            if !checkouts
                .iter()
                .any(|checkout| checkout.path == working_directory)
            {
                return Err(DomainError::InvalidDirectRunCheckout {
                    repository_id: checkouts[0].repository_id,
                });
            }
            if checkouts
                .iter()
                .find(|checkout| checkout.repository_id == repository_id)
                .map(|checkout| checkout.path.as_str())
                != Some(working_directory.as_str())
            {
                return Err(DomainError::RunWorkingDirectoryRepositoryMismatch { repository_id });
            }
            let session_name = clean_name(session_name, DomainError::EmptyRunSessionName)?;
            let pane_id = clean_name(pane_id, DomainError::EmptyRunPaneId)?;
            if state.runs.iter().any(|run| {
                run.machine_id == machine_id
                    && run.session_name == session_name
                    && run.pane_id == pane_id
            }) {
                return Err(DomainError::RunAlreadyAttached {
                    machine_id,
                    session_name,
                    pane_id,
                });
            }
            let id = state.next_run_id;
            let next_run_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let run = Run {
                id,
                item_id,
                workspace_id: Some(workspace_id),
                repository_id: Some(repository_id),
                worktree_id: None,
                machine_id,
                agent,
                execution_profile,
                prompt,
                working_directory,
                session_name,
                pane_id,
                started_at,
                state: RunState::Unknown,
                pane_status: RunPaneStatus::Available,
                direct_checkouts: checkouts,
            };
            state.next_run_id = next_run_id;
            state.runs.push(run.clone());
            Ok(Decision {
                state,
                effects: vec![Effect::PersistRun { run, next_run_id }],
            })
        }
        Event::StartWorktreeRun {
            item_id,
            workspace_id,
            worktree_id,
            machine_id,
            agent,
            execution_profile,
            prompt,
            working_directory,
            session_name,
            pane_id,
            started_at,
            prompt_selection,
        } => {
            let context_id = item_context_id(&state, item_id)?;
            let workspace = state
                .workspaces
                .iter()
                .find(|workspace| workspace.id == workspace_id)
                .ok_or(DomainError::WorkspaceNotFound { workspace_id })?;
            if workspace.item_id != item_id {
                return Err(DomainError::WorkspaceItemMismatch {
                    workspace_id,
                    item_id,
                });
            }
            let worktree = state
                .worktrees
                .iter()
                .find(|worktree| worktree.id == worktree_id)
                .ok_or(DomainError::WorktreeNotFound { worktree_id })?;
            if worktree.workspace_id != workspace_id {
                return Err(DomainError::RunWorktreeWorkspaceMismatch {
                    worktree_id,
                    workspace_id,
                });
            }
            if worktree.machine_id != machine_id {
                return Err(DomainError::MachineContextMismatch {
                    machine_id,
                    context_id,
                });
            }
            let machine = state
                .machines
                .iter()
                .find(|machine| machine.id == machine_id)
                .ok_or(DomainError::MachineNotFound { machine_id })?;
            if machine.context_id != context_id {
                return Err(DomainError::MachineContextMismatch {
                    machine_id,
                    context_id,
                });
            }
            if !workspace
                .repositories
                .iter()
                .any(|repository| repository.repository_id == worktree.repository_id)
            {
                return Err(DomainError::RunRepositoryNotSelected {
                    repository_id: worktree.repository_id,
                    workspace_id,
                });
            }
            for external_object_id in prompt_selection.external_object_ids {
                if !state.links.iter().any(|link| {
                    link.item_id == item_id && link.external_object_id == external_object_id
                }) {
                    return Err(DomainError::RunPromptSourceNotLinked {
                        external_object_id,
                        item_id,
                    });
                }
            }
            let prompt = clean_name(prompt, DomainError::EmptyRunPrompt)?;
            let working_directory =
                clean_name(working_directory, DomainError::EmptyRunWorkingDirectory)?;
            if working_directory != worktree.path {
                return Err(DomainError::RunWorkingDirectoryRepositoryMismatch {
                    repository_id: worktree.repository_id,
                });
            }
            let session_name = clean_name(session_name, DomainError::EmptyRunSessionName)?;
            let pane_id = clean_name(pane_id, DomainError::EmptyRunPaneId)?;
            if state.runs.iter().any(|run| {
                run.machine_id == machine_id
                    && run.session_name == session_name
                    && run.pane_id == pane_id
            }) {
                return Err(DomainError::RunAlreadyAttached {
                    machine_id,
                    session_name,
                    pane_id,
                });
            }
            let id = state.next_run_id;
            let next_run_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let run = Run {
                id,
                item_id,
                workspace_id: Some(workspace_id),
                repository_id: Some(worktree.repository_id),
                worktree_id: Some(worktree_id),
                machine_id,
                agent,
                execution_profile,
                prompt,
                working_directory,
                session_name,
                pane_id,
                started_at,
                state: RunState::Unknown,
                pane_status: RunPaneStatus::Available,
                direct_checkouts: Vec::new(),
            };
            state.next_run_id = next_run_id;
            state.runs.push(run.clone());
            Ok(Decision {
                state,
                effects: vec![Effect::PersistRun { run, next_run_id }],
            })
        }
        Event::AttachRun {
            item_id,
            workspace_id,
            worktree_id,
            repository_id,
            machine_id,
            agent,
            working_directory,
            session_name,
            pane_id,
            attached_at,
        } => {
            let context_id = item_context_id(&state, item_id)?;
            let workspace = state
                .workspaces
                .iter()
                .find(|workspace| workspace.id == workspace_id)
                .ok_or(DomainError::WorkspaceNotFound { workspace_id })?;
            if workspace.item_id != item_id {
                return Err(DomainError::WorkspaceItemMismatch {
                    workspace_id,
                    item_id,
                });
            }
            if !workspace
                .repositories
                .iter()
                .any(|repository| repository.repository_id == repository_id)
            {
                return Err(DomainError::RunRepositoryNotSelected {
                    repository_id,
                    workspace_id,
                });
            }
            if let Some(worktree_id) = worktree_id {
                let worktree = state
                    .worktrees
                    .iter()
                    .find(|worktree| worktree.id == worktree_id)
                    .ok_or(DomainError::WorktreeNotFound { worktree_id })?;
                if worktree.workspace_id != workspace_id
                    || worktree.repository_id != repository_id
                    || worktree.path != working_directory
                {
                    return Err(DomainError::RunWorkingDirectoryRepositoryMismatch {
                        repository_id,
                    });
                }
            } else {
                let location_matches = state.repository_locations.iter().any(|location| {
                    location.repository_id == repository_id
                        && location.machine_id == machine_id
                        && path_is_within(&location.checkout_path, &working_directory)
                });
                if !location_matches {
                    return Err(DomainError::RunWorkingDirectoryRepositoryMismatch {
                        repository_id,
                    });
                }
            }
            let machine = state
                .machines
                .iter()
                .find(|machine| machine.id == machine_id)
                .ok_or(DomainError::MachineNotFound { machine_id })?;
            if machine.context_id != context_id {
                return Err(DomainError::MachineContextMismatch {
                    machine_id,
                    context_id,
                });
            }
            let working_directory =
                clean_name(working_directory, DomainError::EmptyRunWorkingDirectory)?;
            let session_name = clean_name(session_name, DomainError::EmptyRunSessionName)?;
            let pane_id = clean_name(pane_id, DomainError::EmptyRunPaneId)?;
            if state.runs.iter().any(|run| {
                run.machine_id == machine_id
                    && run.session_name == session_name
                    && run.pane_id == pane_id
            }) {
                return Err(DomainError::RunAlreadyAttached {
                    machine_id,
                    session_name,
                    pane_id,
                });
            }
            let id = state.next_run_id;
            let next_run_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let run = Run {
                id,
                item_id,
                workspace_id: Some(workspace_id),
                repository_id: Some(repository_id),
                worktree_id,
                machine_id,
                agent,
                execution_profile: ExecutionProfile::CustomPrompt,
                prompt: "Attached existing agent".into(),
                working_directory,
                session_name,
                pane_id,
                started_at: attached_at,
                state: RunState::Unknown,
                pane_status: RunPaneStatus::Available,
                direct_checkouts: Vec::new(),
            };
            state.next_run_id = next_run_id;
            state.runs.push(run.clone());
            Ok(Decision {
                state,
                effects: vec![Effect::PersistRun { run, next_run_id }],
            })
        }
        Event::UpdateRunState {
            run_id,
            state: run_state,
        } => {
            let run = state
                .runs
                .iter_mut()
                .find(|run| run.id == run_id)
                .ok_or(DomainError::RunNotFound { run_id })?;
            run.state = run_state;
            let run = run.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistRunState { run }],
            })
        }
        Event::SetRunPaneStatus { run_id, status } => {
            let run = state
                .runs
                .iter_mut()
                .find(|run| run.id == run_id)
                .ok_or(DomainError::RunNotFound { run_id })?;
            run.pane_status = status;
            let run = run.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistRunPaneStatus { run }],
            })
        }
        Event::SetItemStatus { item_id, status } => {
            let item = state
                .items
                .iter_mut()
                .find(|item| item.id == item_id)
                .ok_or(DomainError::ItemNotFound { item_id })?;
            item.status = status;
            let item = item.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistItemUpdate { item }],
            })
        }
        Event::SetItemTitle { item_id, title } => {
            let title = title.trim();
            if title.is_empty() {
                return Err(DomainError::EmptyTitle);
            }
            let item = state
                .items
                .iter_mut()
                .find(|item| item.id == item_id)
                .ok_or(DomainError::ItemNotFound { item_id })?;
            item.title = title.to_owned();
            let item = item.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistItemUpdate { item }],
            })
        }
        Event::SetItemNotes { item_id, notes } => {
            let item = state
                .items
                .iter_mut()
                .find(|item| item.id == item_id)
                .ok_or(DomainError::ItemNotFound { item_id })?;
            item.notes = notes;
            let item = item.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistItemUpdate { item }],
            })
        }
        Event::SetItemRelation {
            from_item_id,
            to_item_id,
            kind,
        } => {
            if from_item_id == to_item_id {
                return Err(DomainError::SelfRelation {
                    item_id: from_item_id,
                });
            }
            let from_context_id = item_context_id(&state, from_item_id)?;
            let to_context_id = item_context_id(&state, to_item_id)?;
            if from_context_id != to_context_id {
                return Err(DomainError::ItemContextMismatch {
                    from_item_id,
                    to_item_id,
                });
            }

            let relation = ItemRelation {
                from_item_id,
                to_item_id,
                kind,
            };
            if state.relationships.contains(&relation) {
                return Err(DomainError::RelationAlreadyExists);
            }
            state.relationships.push(relation.clone());

            Ok(Decision {
                state,
                effects: vec![Effect::PersistItemRelation { relation }],
            })
        }
        Event::AddItemReminder { item_id, remind_at } => {
            let remind_at = clean_name(remind_at, DomainError::EmptyReminderAt)?;
            let item = state
                .items
                .iter_mut()
                .find(|item| item.id == item_id)
                .ok_or(DomainError::ItemNotFound { item_id })?;
            let id = state.next_reminder_id;
            let next_reminder_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            item.reminders.push(Reminder { id, remind_at });
            state.next_reminder_id = next_reminder_id;
            let item = item.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistItemReminders {
                    item,
                    next_reminder_id,
                }],
            })
        }
        Event::RemoveItemReminder {
            item_id,
            reminder_id,
        } => {
            let item = state
                .items
                .iter_mut()
                .find(|item| item.id == item_id)
                .ok_or(DomainError::ItemNotFound { item_id })?;
            let position = item
                .reminders
                .iter()
                .position(|reminder| reminder.id == reminder_id)
                .ok_or(DomainError::ReminderNotFound {
                    item_id,
                    reminder_id,
                })?;
            item.reminders.remove(position);
            let item = item.clone();
            let next_reminder_id = state.next_reminder_id;

            Ok(Decision {
                state,
                effects: vec![Effect::PersistItemReminders {
                    item,
                    next_reminder_id,
                }],
            })
        }
        Event::SetLinkWatchUntil {
            link_id,
            watch_until,
        } => {
            let link = state
                .links
                .iter_mut()
                .find(|link| link.id == link_id)
                .ok_or(DomainError::LinkNotFound { link_id })?;
            link.watch_until = watch_until;
            let link = link.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistLinkState { link }],
            })
        }
        Event::SetLinkReviewAt { link_id, review_at } => {
            let link = state
                .links
                .iter_mut()
                .find(|link| link.id == link_id)
                .ok_or(DomainError::LinkNotFound { link_id })?;
            link.review_at = review_at;
            let link = link.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistLinkState { link }],
            })
        }
        Event::ClearLinkReviewAt { link_id } => {
            let link = state
                .links
                .iter_mut()
                .find(|link| link.id == link_id)
                .ok_or(DomainError::LinkNotFound { link_id })?;
            link.review_at = None;
            let link = link.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistLinkState { link }],
            })
        }
        Event::LinkExternalObject {
            item_id,
            object,
            snapshot,
        } => {
            ensure_item(&state, item_id)?;
            if object.canonical_url.trim().is_empty() {
                return Err(DomainError::EmptyExternalUrl);
            }
            if object.external_key.trim().is_empty() {
                return Err(DomainError::EmptyExternalObjectKey);
            }

            let existing_object = state
                .external_objects
                .iter()
                .find(|candidate| {
                    candidate.provider == object.provider
                        && candidate.external_key == object.external_key
                })
                .cloned();
            let (external_object, is_new_object) = match existing_object {
                Some(object) => (object, false),
                None => {
                    let id = state.next_external_object_id;
                    let next_external_object_id =
                        id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
                    let object = ExternalObject {
                        id,
                        provider: object.provider,
                        kind: object.kind,
                        external_key: object.external_key,
                        canonical_url: object.canonical_url.trim().to_owned(),
                    };
                    state.next_external_object_id = next_external_object_id;
                    state.external_objects.push(object.clone());
                    (object, true)
                }
            };

            if state.links.iter().any(|link| {
                link.item_id == item_id && link.external_object_id == external_object.id
            }) {
                return Err(DomainError::LinkAlreadyExists);
            }

            let link_id = state.next_link_id;
            let next_link_id = link_id
                .checked_add(1)
                .ok_or(DomainError::SequenceExhausted)?;
            let reviewed_activity_id = state
                .activities
                .iter()
                .filter(|activity| activity.external_object_id == external_object.id)
                .map(|activity| activity.id)
                .max()
                .unwrap_or_default();
            let link = Link {
                id: link_id,
                item_id,
                external_object_id: external_object.id,
                reviewed_activity_id,
                attention_policy: None,
                watch_until: None,
                review_at: None,
            };
            state.next_link_id = next_link_id;
            state.links.push(link.clone());

            let mut effects = Vec::new();
            if is_new_object {
                effects.push(Effect::PersistExternalObject {
                    object: external_object.clone(),
                    next_external_object_id: state.next_external_object_id,
                });
            }
            effects.push(Effect::PersistLink { link, next_link_id });
            if let Some(snapshot_data) = snapshot {
                let snapshot = ExternalSnapshot {
                    external_object_id: external_object.id,
                    title: snapshot_data.title,
                    state: snapshot_data.state,
                    metadata: snapshot_data.metadata,
                    fetched_at: snapshot_data.fetched_at,
                };
                upsert_snapshot(&mut state, snapshot.clone());
                effects.push(Effect::PersistExternalSnapshot { snapshot });
            }

            Ok(Decision { state, effects })
        }
        Event::RefreshExternalObject {
            external_object_id,
            snapshot: snapshot_data,
        } => {
            if !state
                .external_objects
                .iter()
                .any(|object| object.id == external_object_id)
            {
                return Err(DomainError::ExternalObjectNotFound { external_object_id });
            }
            let snapshot = ExternalSnapshot {
                external_object_id,
                title: snapshot_data.title,
                state: snapshot_data.state,
                metadata: snapshot_data.metadata,
                fetched_at: snapshot_data.fetched_at,
            };
            let changes = state
                .snapshots
                .iter()
                .find(|existing| existing.external_object_id == external_object_id)
                .map(|previous| snapshot_changes(previous, &snapshot))
                .unwrap_or_default();
            upsert_snapshot(&mut state, snapshot.clone());

            let mut effects = Vec::new();
            if !changes.is_empty() {
                let id = state.next_activity_id;
                let next_activity_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
                let activity = Activity {
                    id,
                    external_object_id,
                    observed_at: snapshot.fetched_at,
                    changes,
                };
                state.next_activity_id = next_activity_id;
                state.activities.push(activity.clone());
                effects.push(Effect::PersistActivity {
                    activity,
                    next_activity_id,
                });
            }
            effects.push(Effect::PersistExternalSnapshot { snapshot });

            Ok(Decision { state, effects })
        }
        Event::SetLinkAttentionPolicy { link_id, policy } => {
            let link = state
                .links
                .iter_mut()
                .find(|link| link.id == link_id)
                .ok_or(DomainError::LinkNotFound { link_id })?;
            link.attention_policy = policy;
            let link = link.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistLinkState { link }],
            })
        }
        Event::SetContextAttentionDefault {
            context_id,
            object_kind,
            policy,
        } => {
            ensure_context(&state, context_id)?;
            let attention_default = ContextAttentionDefault {
                context_id,
                object_kind,
                policy,
            };
            if let Some(existing) = state.attention_defaults.iter_mut().find(|existing| {
                existing.context_id == context_id && existing.object_kind == object_kind
            }) {
                *existing = attention_default.clone();
            } else {
                state.attention_defaults.push(attention_default.clone());
            }

            Ok(Decision {
                state,
                effects: vec![Effect::PersistContextAttentionDefault { attention_default }],
            })
        }
        Event::MarkLinkReviewed { link_id } => {
            let external_object_id = state
                .links
                .iter()
                .find(|link| link.id == link_id)
                .ok_or(DomainError::LinkNotFound { link_id })?
                .external_object_id;
            let reviewed_activity_id = state
                .activities
                .iter()
                .filter(|activity| activity.external_object_id == external_object_id)
                .map(|activity| activity.id)
                .max()
                .unwrap_or_default();
            let link = state
                .links
                .iter_mut()
                .find(|link| link.id == link_id)
                .expect("the Link was checked above");
            link.reviewed_activity_id = reviewed_activity_id;
            let link = link.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistLinkState { link }],
            })
        }
    }
}

pub fn suggest_untracked_runs(
    state: &DomainState,
    panes: &[AgentPaneObservation],
) -> Vec<RunSuggestion> {
    let mut suggestions = panes
        .iter()
        .filter(|pane| {
            !state.runs.iter().any(|run| {
                run.machine_id == pane.machine_id
                    && run.session_name == pane.session_name
                    && run.pane_id == pane.pane_id
            })
        })
        .filter_map(|pane| {
            let machine = state
                .machines
                .iter()
                .find(|machine| machine.id == pane.machine_id)?;
            let workspace_location = state
                .workspaces
                .iter()
                .filter_map(|workspace| {
                    let worktree = state
                        .worktrees
                        .iter()
                        .filter(|worktree| {
                            worktree.workspace_id == workspace.id
                                && worktree.machine_id == pane.machine_id
                                && path_is_within(&worktree.path, &pane.current_path)
                        })
                        .max_by_key(|worktree| worktree.path.len());
                    if let Some(worktree) = worktree {
                        return Some((
                            worktree.path.len(),
                            workspace.id,
                            worktree.repository_id,
                            Some(worktree.id),
                            worktree.path.clone(),
                        ));
                    }
                    workspace.repositories.iter().find_map(|selected| {
                        state
                            .repository_locations
                            .iter()
                            .filter(|location| {
                                location.repository_id == selected.repository_id
                                    && location.machine_id == pane.machine_id
                                    && path_is_within(&location.checkout_path, &pane.current_path)
                            })
                            .max_by_key(|location| location.checkout_path.len())
                            .map(|location| {
                                (
                                    location.checkout_path.len(),
                                    workspace.id,
                                    selected.repository_id,
                                    None,
                                    location.checkout_path.clone(),
                                )
                            })
                    })
                })
                .max_by_key(|candidate| candidate.0);
            let (_, workspace_id, repository_id, worktree_id, location_path) = workspace_location?;
            let workspace = state
                .workspaces
                .iter()
                .find(|workspace| workspace.id == workspace_id)?;
            let item_id = workspace.item_id;
            let item = state.items.iter().find(|item| item.id == item_id)?;
            let project = state
                .projects
                .iter()
                .find(|project| project.id == item.project_id)?;
            let context = state
                .contexts
                .iter()
                .find(|context| context.id == project.context_id)?;

            Some(RunSuggestion {
                machine_id: machine.id,
                machine_name: machine.name.clone(),
                agent: pane.agent,
                session_name: pane.session_name.clone(),
                pane_id: pane.pane_id.clone(),
                current_path: pane.current_path.clone(),
                item_id: item.id,
                item_identifier: item.human_identifier.clone(),
                item_title: item.title.clone(),
                context_id: context.id,
                context_name: context.name.clone(),
                workspace_id: Some(workspace_id),
                repository_id: Some(repository_id),
                worktree_id,
                location_path: Some(location_path),
            })
        })
        .collect::<Vec<_>>();
    suggestions.sort_by(|left, right| {
        left.machine_id
            .cmp(&right.machine_id)
            .then_with(|| left.session_name.cmp(&right.session_name))
            .then_with(|| left.pane_id.cmp(&right.pane_id))
    });
    suggestions
}

fn path_is_within(root: &str, path: &str) -> bool {
    let root = without_macos_private_prefix(root).trim_end_matches('/');
    let path = without_macos_private_prefix(path);
    if let Some(home_relative_root) = root.strip_prefix("~/") {
        return path == home_relative_root
            || path.ends_with(&format!("/{home_relative_root}"))
            || path.ends_with(&format!("/{home_relative_root}/"))
            || path.contains(&format!("/{home_relative_root}/"));
    }
    root == "/" || path == root || path.starts_with(&format!("{root}/"))
}

fn without_macos_private_prefix(path: &str) -> &str {
    path.strip_prefix("/private")
        .filter(|path| path.starts_with('/'))
        .unwrap_or(path)
}

pub fn home_view(state: &DomainState, context_id: Option<i64>, now: &str) -> HomeView {
    let mut view = HomeView {
        needs_attention: Vec::new(),
        attention_entries: attention_entries(state, context_id, now),
        running: Vec::new(),
        waiting: Vec::new(),
        due: Vec::new(),
        completed: Vec::new(),
    };

    for item in item_views_at(state, context_id, Some(now)) {
        let is_due = item_has_due_reminder(&item.item, now) && item.item.status != ItemStatus::Done;
        if is_due {
            view.due.push(item.clone());
        }
        if is_due
            || item.item.status == ItemStatus::Inbox
            || (item.item.status != ItemStatus::Done
                && view
                    .attention_entries
                    .iter()
                    .any(|entry| entry.item_id == item.item.id))
        {
            view.needs_attention.push(item.clone());
        }
        match item.item.status {
            ItemStatus::Inbox => {}
            ItemStatus::Active => view.running.push(item),
            ItemStatus::Waiting => view.waiting.push(item),
            ItemStatus::Done => view.completed.push(item),
        }
    }

    view
}

pub fn search_items(state: &DomainState, query: &str, context_id: Option<i64>) -> Vec<ItemView> {
    let query = query.trim().to_lowercase();
    item_views(state, context_id)
        .into_iter()
        .filter(|view| {
            query.is_empty()
                || [
                    view.item.human_identifier.as_str(),
                    view.item.title.as_str(),
                    view.item.notes.as_str(),
                    view.context_name.as_str(),
                    view.project_name.as_str(),
                ]
                .iter()
                .any(|field| field.to_lowercase().contains(&query))
        })
        .collect()
}

pub fn external_link_view(state: &DomainState, link: &Link) -> Option<ExternalLinkView> {
    external_link_view_at(state, link, None)
}

fn external_link_view_at(
    state: &DomainState,
    link: &Link,
    now: Option<&str>,
) -> Option<ExternalLinkView> {
    let object = state
        .external_objects
        .iter()
        .find(|object| object.id == link.external_object_id)?;
    let snapshot = state
        .snapshots
        .iter()
        .find(|snapshot| snapshot.external_object_id == object.id)
        .cloned();
    Some(ExternalLinkView {
        link: link.clone(),
        object: object.clone(),
        snapshot,
        attention_policy: effective_attention_policy(state, link, object),
        attention_entry: attention_entry_for_link_at(state, link, object, now),
    })
}

pub fn attention_entries(
    state: &DomainState,
    context_id: Option<i64>,
    now: &str,
) -> Vec<AttentionEntry> {
    attention_entries_at(state, context_id, Some(now))
}

fn attention_entries_at(
    state: &DomainState,
    context_id: Option<i64>,
    now: Option<&str>,
) -> Vec<AttentionEntry> {
    let mut entries = Vec::new();
    for link in state.links.iter().filter(|link| {
        context_id.is_none_or(|context_id| {
            item_context_id(state, link.item_id)
                .map(|link_context_id| link_context_id == context_id)
                .unwrap_or(false)
        })
    }) {
        let object = state
            .external_objects
            .iter()
            .find(|object| object.id == link.external_object_id);
        let Some(object) = object else {
            continue;
        };
        if let Some(entry) = attention_entry_for_link_at(state, link, object, now) {
            entries.push(entry);
        }
        if link
            .review_at
            .as_deref()
            .is_some_and(|review_at| now.is_some_and(|now| review_at <= now))
        {
            entries.push(AttentionEntry {
                kind: AttentionEntryKind::Review,
                link_id: link.id,
                reminder_id: None,
                run_id: None,
                item_id: link.item_id,
                external_object_id: object.id,
                source_title: state
                    .snapshots
                    .iter()
                    .find(|snapshot| snapshot.external_object_id == object.id)
                    .map(|snapshot| snapshot.title.clone())
                    .unwrap_or_else(|| object.canonical_url.clone()),
                source_url: object.canonical_url.clone(),
                activities: Vec::new(),
                summary: format!(
                    "Review scheduled for {}",
                    link.review_at.as_deref().unwrap_or_default()
                ),
            });
        }
    }

    for item in state.items.iter().filter(|item| {
        context_id.is_none_or(|context_id| {
            item_context_id(state, item.id)
                .map(|item_context_id| item_context_id == context_id)
                .unwrap_or(false)
        })
    }) {
        entries.extend(
            item.reminders
                .iter()
                .filter(|reminder| reminder.remind_at.as_str() <= now.unwrap_or_default())
                .map(|reminder| AttentionEntry {
                    kind: AttentionEntryKind::Reminder,
                    link_id: 0,
                    reminder_id: Some(reminder.id),
                    run_id: None,
                    item_id: item.id,
                    external_object_id: 0,
                    source_title: item.title.clone(),
                    source_url: String::new(),
                    activities: Vec::new(),
                    summary: format!("Reminder due at {}", reminder.remind_at),
                }),
        );
    }

    entries.extend(
        state
            .runs
            .iter()
            .filter(|run| run.state == RunState::Blocked)
            .filter(|run| {
                context_id.is_none_or(|context_id| {
                    item_context_id(state, run.item_id)
                        .map(|run_context_id| run_context_id == context_id)
                        .unwrap_or(false)
                })
            })
            .filter_map(|run| {
                let item = state.items.iter().find(|item| item.id == run.item_id)?;
                Some(AttentionEntry {
                    kind: AttentionEntryKind::BlockedRun,
                    link_id: 0,
                    reminder_id: None,
                    run_id: Some(run.id),
                    item_id: item.id,
                    external_object_id: 0,
                    source_title: item.title.clone(),
                    source_url: String::new(),
                    activities: Vec::new(),
                    summary: format!("Run #{} is blocked and needs your input", run.id),
                })
            }),
    );

    entries
}

fn item_has_due_reminder(item: &Item, now: &str) -> bool {
    item.reminders
        .iter()
        .any(|reminder| reminder.remind_at.as_str() <= now)
}

fn item_views(state: &DomainState, context_id: Option<i64>) -> Vec<ItemView> {
    item_views_at(state, context_id, None)
}

fn item_views_at(state: &DomainState, context_id: Option<i64>, now: Option<&str>) -> Vec<ItemView> {
    state
        .items
        .iter()
        .filter_map(|item| {
            let project = state
                .projects
                .iter()
                .find(|project| project.id == item.project_id)?;
            if context_id.is_some_and(|candidate| candidate != project.context_id) {
                return None;
            }
            let context = state
                .contexts
                .iter()
                .find(|context| context.id == project.context_id)?;
            let relationships = state
                .relationships
                .iter()
                .filter(|relation| {
                    relation.from_item_id == item.id || relation.to_item_id == item.id
                })
                .cloned()
                .collect();
            let links = state
                .links
                .iter()
                .filter(|link| link.item_id == item.id)
                .filter_map(|link| external_link_view_at(state, link, now))
                .collect();
            let workspaces = state
                .workspaces
                .iter()
                .filter(|workspace| workspace.item_id == item.id)
                .cloned()
                .collect();
            let runs = state
                .runs
                .iter()
                .filter(|run| run.item_id == item.id)
                .cloned()
                .collect();
            let worktrees = state
                .worktrees
                .iter()
                .filter(|worktree| {
                    state.workspaces.iter().any(|workspace| {
                        workspace.id == worktree.workspace_id && workspace.item_id == item.id
                    })
                })
                .cloned()
                .collect();
            Some(ItemView {
                item: item.clone(),
                context_id: context.id,
                context_name: context.name.clone(),
                project_name: project.name.clone(),
                relationships,
                workspaces,
                worktrees,
                runs,
                links,
            })
        })
        .collect()
}

pub fn compose_run_prompt(
    state: &DomainState,
    item_id: i64,
    profile: ExecutionProfile,
    selection: &RunPromptSelection,
    custom_prompt: Option<&str>,
) -> Result<String, DomainError> {
    let item = state
        .items
        .iter()
        .find(|item| item.id == item_id)
        .ok_or(DomainError::ItemNotFound { item_id })?;
    let mut sections = Vec::new();
    if selection.include_objective {
        sections.push(format!("Item objective:\n{}", item.title));
    }
    if selection.include_notes && !item.notes.trim().is_empty() {
        sections.push(format!("Item notes:\n{}", item.notes.trim()));
    }
    for external_object_id in &selection.external_object_ids {
        let link = state
            .links
            .iter()
            .find(|link| link.item_id == item_id && link.external_object_id == *external_object_id)
            .ok_or(DomainError::RunPromptSourceNotLinked {
                external_object_id: *external_object_id,
                item_id,
            })?;
        let object = state
            .external_objects
            .iter()
            .find(|object| object.id == link.external_object_id)
            .ok_or(DomainError::ExternalObjectNotFound {
                external_object_id: *external_object_id,
            })?;
        let title = state
            .snapshots
            .iter()
            .find(|snapshot| snapshot.external_object_id == object.id)
            .map(|snapshot| snapshot.title.as_str())
            .unwrap_or("Linked external object");
        sections.push(format!("Linked source:\n{title}\n{}", object.canonical_url));
    }

    let instruction = match profile {
        ExecutionProfile::Investigate => {
            "Investigate this work, inspect the relevant code, and report findings before changing files."
                .to_owned()
        }
        ExecutionProfile::Implement => {
            "Implement this work in the selected Workspace, run the relevant checks, and leave the changes ready for review."
                .to_owned()
        }
        ExecutionProfile::Review => {
            "Review the current Workspace changes for correctness, regressions, and missing test coverage."
                .to_owned()
        }
        ExecutionProfile::CustomPrompt => clean_name(
            custom_prompt.unwrap_or_default().to_owned(),
            DomainError::EmptyRunPrompt,
        )?,
    };
    sections.insert(0, instruction);
    let prompt = sections.join("\n\n");
    clean_name(prompt, DomainError::EmptyRunPrompt)
}

fn clean_name<E>(name: String, empty_error: E) -> Result<String, E> {
    let name = name.trim();
    if name.is_empty() {
        return Err(empty_error);
    }
    Ok(name.to_owned())
}

fn clean_machine_transport(transport: MachineTransport) -> Result<MachineTransport, DomainError> {
    match transport {
        MachineTransport::Local => Ok(MachineTransport::Local),
        MachineTransport::Ssh {
            host,
            user,
            port,
            identity_file,
            known_hosts_file,
            strict_host_key_checking,
        } => {
            let host = host.trim().to_owned();
            if host.is_empty() {
                return Err(DomainError::EmptyMachineHost);
            }
            if !host
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b".@:_-".contains(&byte))
            {
                return Err(DomainError::InvalidMachineHost);
            }
            let user = user.map(|value| value.trim().to_owned());
            if user.as_deref().is_some_and(str::is_empty) {
                return Err(DomainError::EmptyMachineUser);
            }
            if user.as_deref().is_some_and(|value| {
                !value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
            }) {
                return Err(DomainError::InvalidMachineUser);
            }
            if port == Some(0) {
                return Err(DomainError::InvalidMachinePort);
            }
            if strict_host_key_checking
                .as_deref()
                .is_some_and(|value| !matches!(value, "yes" | "accept-new" | "no"))
            {
                return Err(DomainError::InvalidMachineHostKeyChecking);
            }
            Ok(MachineTransport::Ssh {
                host,
                user,
                port,
                identity_file: identity_file.map(|value| value.trim().to_owned()),
                known_hosts_file: known_hosts_file.map(|value| value.trim().to_owned()),
                strict_host_key_checking,
            })
        }
    }
}

fn ensure_context(state: &DomainState, context_id: i64) -> Result<(), DomainError> {
    if state
        .contexts
        .iter()
        .any(|context| context.id == context_id)
    {
        Ok(())
    } else {
        Err(DomainError::ContextNotFound { context_id })
    }
}

fn item_project_id(state: &DomainState, item_id: i64) -> Result<i64, DomainError> {
    let item = state
        .items
        .iter()
        .find(|item| item.id == item_id)
        .ok_or(DomainError::ItemNotFound { item_id })?;
    state
        .projects
        .iter()
        .find(|project| project.id == item.project_id)
        .map(|project| project.id)
        .ok_or(DomainError::ProjectNotFound {
            project_id: item.project_id,
        })
}

fn normalize_workspace_repositories(
    state: &DomainState,
    project_id: i64,
    repositories: Vec<WorkspaceRepositoryInput>,
) -> Result<Vec<WorkspaceRepository>, DomainError> {
    let mut normalized = Vec::with_capacity(repositories.len());
    for input in repositories {
        let repository = state
            .repositories
            .iter()
            .find(|repository| repository.id == input.repository_id)
            .ok_or(DomainError::RepositoryNotFound {
                repository_id: input.repository_id,
            })?;
        if repository.project_id != project_id {
            return Err(DomainError::RepositoryProjectMismatch {
                repository_id: input.repository_id,
                project_id,
            });
        }
        if normalized
            .iter()
            .any(|selected: &WorkspaceRepository| selected.repository_id == input.repository_id)
        {
            return Err(DomainError::DuplicateRepositorySelection {
                repository_id: input.repository_id,
            });
        }
        normalized.push(WorkspaceRepository {
            repository_id: input.repository_id,
            branch: clean_name(input.branch, DomainError::EmptyBranch)?,
            base_branch: clean_name(input.base_branch, DomainError::EmptyBranch)?,
        });
    }
    Ok(normalized)
}

fn clean_optional_branch(branch: Option<String>) -> Result<Option<String>, DomainError> {
    branch
        .map(|branch| clean_name(branch, DomainError::EmptyBranch))
        .transpose()
}

fn clean_repository_name(name: String) -> Result<String, DomainError> {
    let name = clean_name(name, DomainError::EmptyRepositoryName)?;
    if name == "." || name == ".." || name.contains('/') || name.contains('\\') {
        return Err(DomainError::InvalidRepositoryName);
    }
    Ok(name)
}

fn item_context_id(state: &DomainState, item_id: i64) -> Result<i64, DomainError> {
    let item = state
        .items
        .iter()
        .find(|item| item.id == item_id)
        .ok_or(DomainError::ItemNotFound { item_id })?;
    state
        .projects
        .iter()
        .find(|project| project.id == item.project_id)
        .map(|project| project.context_id)
        .ok_or(DomainError::ProjectNotFound {
            project_id: item.project_id,
        })
}

fn ensure_item(state: &DomainState, item_id: i64) -> Result<(), DomainError> {
    if state.items.iter().any(|item| item.id == item_id) {
        Ok(())
    } else {
        Err(DomainError::ItemNotFound { item_id })
    }
}

fn upsert_snapshot(state: &mut DomainState, snapshot: ExternalSnapshot) {
    if let Some(existing) = state
        .snapshots
        .iter_mut()
        .find(|existing| existing.external_object_id == snapshot.external_object_id)
    {
        *existing = snapshot;
    } else {
        state.snapshots.push(snapshot);
    }
}

fn effective_attention_policy(
    state: &DomainState,
    link: &Link,
    object: &ExternalObject,
) -> ExternalChangePolicy {
    if let Some(policy) = link.attention_policy {
        return policy;
    }
    let context_id = item_context_id(state, link.item_id).ok();
    context_id
        .and_then(|context_id| {
            state.attention_defaults.iter().find(|attention_default| {
                attention_default.context_id == context_id
                    && attention_default.object_kind == object.kind
            })
        })
        .map(|attention_default| attention_default.policy)
        .unwrap_or_else(ExternalChangePolicy::all)
}

fn attention_entry_for_link_at(
    state: &DomainState,
    link: &Link,
    object: &ExternalObject,
    now: Option<&str>,
) -> Option<AttentionEntry> {
    let policy = effective_attention_policy(state, link, object);
    let watch_active = now.is_none_or(|now| {
        link.watch_until
            .as_deref()
            .is_none_or(|watch_until| watch_until > now)
    });
    let activities = state
        .activities
        .iter()
        .filter(|activity| {
            watch_active
                && activity.external_object_id == object.id
                && activity.id > link.reviewed_activity_id
        })
        .filter_map(|activity| {
            let changes = activity
                .changes
                .iter()
                .filter(|change| policy.allows(change.kind))
                .cloned()
                .collect::<Vec<_>>();
            (!changes.is_empty()).then_some(Activity {
                id: activity.id,
                external_object_id: activity.external_object_id,
                observed_at: activity.observed_at,
                changes,
            })
        })
        .collect::<Vec<_>>();
    if activities.is_empty() {
        return None;
    }

    let source_title = state
        .snapshots
        .iter()
        .find(|snapshot| snapshot.external_object_id == object.id)
        .map(|snapshot| snapshot.title.clone())
        .unwrap_or_else(|| object.canonical_url.clone());
    let summary = activities
        .iter()
        .flat_map(|activity| activity.changes.iter())
        .map(format_change)
        .collect::<Vec<_>>()
        .join("; ");

    Some(AttentionEntry {
        kind: AttentionEntryKind::ExternalChange,
        link_id: link.id,
        reminder_id: None,
        run_id: None,
        item_id: link.item_id,
        external_object_id: object.id,
        source_title,
        source_url: object.canonical_url.clone(),
        activities,
        summary,
    })
}

fn snapshot_changes(
    previous: &ExternalSnapshot,
    current: &ExternalSnapshot,
) -> Vec<ExternalChange> {
    let mut changes = Vec::new();
    if previous.title != current.title {
        changes.push(ExternalChange {
            kind: ExternalChangeKind::Title,
            key: None,
            previous: Some(previous.title.clone()),
            current: Some(current.title.clone()),
        });
    }
    if previous.state != current.state {
        changes.push(ExternalChange {
            kind: ExternalChangeKind::State,
            key: None,
            previous: Some(previous.state.clone()),
            current: Some(current.state.clone()),
        });
    }

    let mut keys = previous
        .metadata
        .iter()
        .map(|metadata| metadata.key.clone())
        .chain(current.metadata.iter().map(|metadata| metadata.key.clone()))
        .collect::<Vec<_>>();
    keys.sort();
    keys.dedup();
    for key in keys {
        let previous_value = previous
            .metadata
            .iter()
            .find(|metadata| metadata.key == key)
            .map(|metadata| metadata.value.clone());
        let current_value = current
            .metadata
            .iter()
            .find(|metadata| metadata.key == key)
            .map(|metadata| metadata.value.clone());
        if previous_value != current_value {
            changes.push(ExternalChange {
                kind: ExternalChangeKind::Metadata,
                key: Some(key),
                previous: previous_value,
                current: current_value,
            });
        }
    }
    changes
}

fn format_change(change: &ExternalChange) -> String {
    let label = match change.kind {
        ExternalChangeKind::Title => "Title".to_owned(),
        ExternalChangeKind::State => "State".to_owned(),
        ExternalChangeKind::Metadata => {
            format!("Metadata {}", change.key.as_deref().unwrap_or("value"))
        }
    };
    match (&change.previous, &change.current) {
        (Some(previous), Some(current)) => format!("{label} changed from {previous} to {current}"),
        (None, Some(current)) => format!("{label} added as {current}"),
        (Some(previous), None) => format!("{label} removed (was {previous})"),
        (None, None) => format!("{label} changed"),
    }
}

#[cfg(test)]
mod workspace_contract_tests {
    use super::*;

    #[test]
    fn a_workspace_selects_repositories_and_worktree_execution_keeps_context() {
        let mut state = DomainState {
            next_context_id: 2,
            next_project_id: 2,
            next_item_id: 2,
            next_item_number: 2,
            next_repository_id: 2,
            next_workspace_id: 1,
            next_worktree_id: 1,
            next_machine_id: 2,
            next_run_id: 1,
            next_external_object_id: 1,
            next_link_id: 1,
            next_activity_id: 1,
            next_reminder_id: 1,
            contexts: vec![Context {
                id: 1,
                name: "Personal".into(),
            }],
            projects: vec![Project {
                id: 1,
                context_id: 1,
                name: "Default".into(),
                defaults: ProjectDefaults::default(),
            }],
            repositories: vec![Repository {
                id: 1,
                project_id: 1,
                name: "mission-manager".into(),
                remote_url: "https://example.test/repo".into(),
                base_branch: "main".into(),
            }],
            repository_locations: Vec::new(),
            items: vec![Item {
                id: 1,
                human_identifier: "MC-1".into(),
                title: "Use Workspace vocabulary".into(),
                project_id: 1,
                status: ItemStatus::Active,
                notes: String::new(),
                reminders: Vec::new(),
            }],
            workspaces: Vec::new(),
            worktrees: Vec::new(),
            machines: vec![Machine {
                id: 1,
                context_id: 1,
                name: "Local".into(),
                socket_name: "mission".into(),
                transport: MachineTransport::Local,
                last_observed: MachineObservation::Available,
                last_observed_at: None,
            }],
            runs: Vec::new(),
            relationships: Vec::new(),
            external_objects: Vec::new(),
            links: Vec::new(),
            snapshots: Vec::new(),
            activities: Vec::new(),
            attention_defaults: Vec::new(),
        };

        let workspace = decide(
            state.clone(),
            Event::CreateWorkspace {
                item_id: 1,
                repositories: vec![WorkspaceRepositoryInput {
                    repository_id: 1,
                    branch: "feature/contract".into(),
                    base_branch: "main".into(),
                }],
            },
        )
        .expect("Workspace creation should succeed");
        state = workspace.state;
        assert_eq!(state.workspaces[0].item_id, 1);
        assert_eq!(state.workspaces[0].repositories[0].repository_id, 1);

        let worktree = decide(
            state,
            Event::CreateWorktree {
                workspace_id: 1,
                repository_id: 1,
                machine_id: 1,
                path: "/tmp/worktrees/mission-manager".into(),
                branch: "feature/contract".into(),
                base_branch: "main".into(),
                is_dirty: false,
            },
        )
        .expect("Worktree creation should succeed");
        assert_eq!(worktree.state.worktrees[0].workspace_id, 1);
        assert_eq!(
            worktree.state.workspaces[0].preparation_state,
            WorkspacePreparationState::Ready
        );
    }

    #[test]
    fn run_suggestions_only_attach_to_registered_workspace_locations() {
        let state = DomainState {
            next_context_id: 2,
            next_project_id: 2,
            next_item_id: 2,
            next_item_number: 2,
            next_repository_id: 2,
            next_workspace_id: 2,
            next_worktree_id: 1,
            next_machine_id: 2,
            next_run_id: 1,
            next_external_object_id: 1,
            next_link_id: 1,
            next_activity_id: 1,
            next_reminder_id: 1,
            contexts: vec![Context {
                id: 1,
                name: "Personal".into(),
            }],
            projects: vec![Project {
                id: 1,
                context_id: 1,
                name: "Default".into(),
                defaults: ProjectDefaults::default(),
            }],
            repositories: vec![Repository {
                id: 1,
                project_id: 1,
                name: "repo".into(),
                remote_url: "https://example.test/repo".into(),
                base_branch: "main".into(),
            }],
            repository_locations: vec![RepositoryLocation {
                repository_id: 1,
                machine_id: 1,
                checkout_path: "/tmp/checkouts/repo".into(),
                worktree_root: "/tmp/worktrees".into(),
            }],
            items: vec![Item {
                id: 1,
                human_identifier: "MC-1".into(),
                title: "Attach".into(),
                project_id: 1,
                status: ItemStatus::Active,
                notes: String::new(),
                reminders: Vec::new(),
            }],
            workspaces: vec![Workspace {
                id: 1,
                item_id: 1,
                repositories: vec![WorkspaceRepository {
                    repository_id: 1,
                    branch: "main".into(),
                    base_branch: "main".into(),
                }],
                preparation_state: WorkspacePreparationState::Pending,
            }],
            worktrees: Vec::new(),
            machines: vec![Machine {
                id: 1,
                context_id: 1,
                name: "Local".into(),
                socket_name: "mission".into(),
                transport: MachineTransport::Local,
                last_observed: MachineObservation::Available,
                last_observed_at: None,
            }],
            runs: Vec::new(),
            relationships: Vec::new(),
            external_objects: Vec::new(),
            links: Vec::new(),
            snapshots: Vec::new(),
            activities: Vec::new(),
            attention_defaults: Vec::new(),
        };

        let suggestions = suggest_untracked_runs(
            &state,
            &[AgentPaneObservation {
                machine_id: 1,
                agent: AgentKind::Codex,
                session_name: "mission".into(),
                pane_id: "%1".into(),
                current_path: "/tmp/checkouts/repo/src".into(),
            }],
        );
        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].workspace_id, Some(1));
        assert_eq!(suggestions[0].repository_id, Some(1));
        assert_eq!(
            suggestions[0].location_path.as_deref(),
            Some("/tmp/checkouts/repo")
        );
    }
}
