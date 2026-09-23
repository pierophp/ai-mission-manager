use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Context {
    pub id: i64,
    pub name: String,
    #[serde(default)]
    pub execution_machine_id: Option<i64>,
    pub grill_defaults: GrillConfiguration,
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
    #[serde(rename = "grill")]
    Grill,
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
#[serde(rename_all = "camelCase")]
pub enum GrillPhase {
    Starting,
    Working,
    WaitingForAnswers,
    AwaitingNextAction,
    RecoverablePaneLoss,
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
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub skill_snapshot: Option<String>,
    pub prompt: String,
    pub working_directory: String,
    pub session_name: String,
    pub pane_id: String,
    pub started_at: i64,
    pub state: RunState,
    pub pane_status: RunPaneStatus,
    pub direct_checkouts: Vec<RunCheckout>,
    #[serde(default)]
    pub transcript: String,
    #[serde(default)]
    pub grill_question_group: Option<GrillQuestionGroup>,
    #[serde(default)]
    pub grill_answers: Vec<GrillAnswer>,
    #[serde(default)]
    pub grill_decisions: Vec<GrillAnswer>,
    #[serde(default)]
    pub grill_response: Option<String>,
    #[serde(default)]
    pub grill_phase: Option<GrillPhase>,
    #[serde(default)]
    pub grill_action: Option<GrillContinuationAction>,
}

pub fn run_is_active(run: &Run) -> bool {
    run.state != RunState::Finished
        || (run.execution_profile == ExecutionProfile::Grill
            && run.grill_phase != Some(GrillPhase::Finished))
}

pub fn run_is_finished(run: &Run) -> bool {
    !run_is_active(run)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentPaneObservation {
    pub machine_id: i64,
    pub agent: AgentKind,
    pub session_name: String,
    pub pane_id: String,
    pub current_path: String,
    /// The home directory resolved by the application adapter for this Machine.
    ///
    /// Keeping this as input makes path matching deterministic and prevents the
    /// pure domain core from consulting the process environment.
    pub machine_home: String,
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

    pub(crate) fn allows(self, kind: ExternalChangeKind) -> bool {
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
    #[serde(default)]
    pub provenance: Option<LinkProvenance>,
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
    pub worktree_ids: Vec<i64>,
    pub repository_location_repository_ids: Vec<i64>,
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
        #[serde(default)]
        worktree_count: Option<usize>,
    },
    RunCreated {
        run_id: i64,
    },
    RunStopped {
        run_id: i64,
    },
    RunFinished {
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
