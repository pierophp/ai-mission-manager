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
}

impl Default for ProjectDefaults {
    fn default() -> Self {
        Self {
            item_status: ItemStatus::Inbox,
        }
    }
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorksetRepositoryInput {
    pub repository_id: i64,
    pub branch_override: Option<String>,
    pub base_branch_override: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachedRepositoryInput {
    pub name: String,
    pub remote_url: String,
    pub current_branch: String,
    pub is_dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorksetRepository {
    pub repository_id: i64,
    pub branch_override: Option<String>,
    pub base_branch_override: Option<String>,
    pub current_branch: String,
    pub is_dirty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workset {
    pub id: i64,
    pub item_id: i64,
    pub root_directory: String,
    pub branch: String,
    pub archived: bool,
    pub repositories: Vec<WorksetRepository>,
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
    pub workset_id: i64,
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
    pub workset_id: i64,
    pub workset_root_directory: String,
    pub workset_branch: String,
    pub item_id: i64,
    pub item_identifier: String,
    pub item_title: String,
    pub context_id: i64,
    pub context_name: String,
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
    pub worksets: Vec<Workset>,
    pub archived_worksets: Vec<Workset>,
    pub runs: Vec<Run>,
    pub links: Vec<ExternalLinkView>,
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
    WorksetCreated {
        workset_id: i64,
    },
    WorksetUpdated {
        workset_id: i64,
    },
    WorksetArchived {
        workset_id: i64,
        archived: bool,
    },
    WorksetRemoved {
        workset_id: i64,
    },
    MachineRegistered {
        machine_id: i64,
    },
    MachineObserved {
        machine_id: i64,
        observation: MachineObservation,
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
    ContextAttentionDefaultChanged {
        context_id: i64,
        object_kind: ExternalObjectKind,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: i64,
    pub recorded_at: i64,
    pub action: AuditAction,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainState {
    pub next_context_id: i64,
    pub next_project_id: i64,
    pub next_item_id: i64,
    pub next_item_number: i64,
    pub next_repository_id: i64,
    pub next_workset_id: i64,
    pub next_machine_id: i64,
    pub next_run_id: i64,
    pub next_external_object_id: i64,
    pub next_link_id: i64,
    pub next_activity_id: i64,
    pub next_reminder_id: i64,
    pub contexts: Vec<Context>,
    pub projects: Vec<Project>,
    pub repositories: Vec<Repository>,
    pub items: Vec<Item>,
    pub worksets: Vec<Workset>,
    pub machines: Vec<Machine>,
    pub runs: Vec<Run>,
    pub relationships: Vec<ItemRelation>,
    pub external_objects: Vec<ExternalObject>,
    pub links: Vec<Link>,
    pub snapshots: Vec<ExternalSnapshot>,
    pub activities: Vec<Activity>,
    pub attention_defaults: Vec<ContextAttentionDefault>,
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
    CreateItem {
        title: String,
        context_id: i64,
        project_id: i64,
    },
    CreateWorkset {
        item_id: i64,
        root_directory: String,
        branch: String,
        repositories: Vec<WorksetRepositoryInput>,
    },
    AttachWorkset {
        item_id: i64,
        root_directory: String,
        repositories: Vec<AttachedRepositoryInput>,
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
    AddRepositoryToWorkset {
        workset_id: i64,
        repository_id: i64,
        branch_override: Option<String>,
        base_branch_override: Option<String>,
    },
    SetWorksetArchived {
        workset_id: i64,
        archived: bool,
    },
    RemoveWorkset {
        workset_id: i64,
    },
    StartRun {
        item_id: i64,
        workset_id: i64,
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
        workset_id: i64,
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
    PersistItem {
        item: Item,
        next_item_number: i64,
        next_item_id: i64,
    },
    PersistWorkset {
        workset: Workset,
        next_workset_id: i64,
    },
    PersistWorksetUpdate {
        workset: Workset,
    },
    RemoveWorkset {
        workset_id: i64,
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decision {
    pub state: DomainState,
    pub effects: Vec<Effect>,
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
    #[error("Project {project_id} belongs to another Context")]
    ProjectContextMismatch { project_id: i64, context_id: i64 },
    #[error("a Repository name cannot be blank")]
    EmptyRepositoryName,
    #[error("a Repository name must be a single directory name")]
    InvalidRepositoryName,
    #[error("a Repository remote URL cannot be blank")]
    EmptyRepositoryRemoteUrl,
    #[error("Repository name already exists in Project {project_id}: {name}")]
    RepositoryNameTaken { project_id: i64, name: String },
    #[error("Repository {name} in Project {project_id} has a different remote URL")]
    RepositoryRemoteMismatch { project_id: i64, name: String },
    #[error("Repository {repository_id} does not exist")]
    RepositoryNotFound { repository_id: i64 },
    #[error("Repository {repository_id} belongs to another Project than {project_id}")]
    RepositoryProjectMismatch { repository_id: i64, project_id: i64 },
    #[error("a Workset root directory cannot be blank")]
    EmptyWorksetRoot,
    #[error("a Workset branch cannot be blank")]
    EmptyWorksetBranch,
    #[error("a Workset must include at least one Repository")]
    EmptyWorksetRepositories,
    #[error("Workset {workset_id} does not exist")]
    WorksetNotFound { workset_id: i64 },
    #[error("Workset {workset_id} has Run history and cannot be removed")]
    WorksetHasRuns { workset_id: i64 },
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
    #[error("Run working directory does not match Workset {workset_id}")]
    RunWorkingDirectoryMismatch { workset_id: i64 },
    #[error("a Run session name cannot be blank")]
    EmptyRunSessionName,
    #[error("a Run Pane identity cannot be blank")]
    EmptyRunPaneId,
    #[error("Run {run_id} does not exist")]
    RunNotFound { run_id: i64 },
    #[error(
        "a Run is already attached to Machine {machine_id}, session {session_name}, Pane {pane_id}"
    )]
    RunAlreadyAttached {
        machine_id: i64,
        session_name: String,
        pane_id: String,
    },
    #[error("Workset {workset_id} belongs to another Item")]
    WorksetItemMismatch { workset_id: i64, item_id: i64 },
    #[error("External Object {external_object_id} is not linked to Item {item_id}")]
    RunPromptSourceNotLinked {
        external_object_id: i64,
        item_id: i64,
    },
    #[error("Repository {repository_id} is already in Workset {workset_id}")]
    RepositoryAlreadyInWorkset { repository_id: i64, workset_id: i64 },
    #[error("Repository {repository_id} was selected more than once")]
    DuplicateRepositorySelection { repository_id: i64 },
    #[error("Item {item_id} does not exist")]
    ItemNotFound { item_id: i64 },
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
        Event::CreateWorkset {
            item_id,
            root_directory,
            branch,
            repositories,
        } => {
            let root_directory = clean_name(root_directory, DomainError::EmptyWorksetRoot)?;
            let branch = clean_name(branch, DomainError::EmptyWorksetBranch)?;
            let project_id = item_project_id(&state, item_id)?;
            if repositories.is_empty() {
                return Err(DomainError::EmptyWorksetRepositories);
            }
            let repositories =
                normalize_workset_repositories(&state, project_id, &branch, repositories)?;
            let id = state.next_workset_id;
            let next_workset_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let workset = Workset {
                id,
                item_id,
                root_directory,
                branch,
                archived: false,
                repositories,
            };
            state.next_workset_id = next_workset_id;
            state.worksets.push(workset.clone());

            Ok(Decision {
                state,
                effects: vec![Effect::PersistWorkset {
                    workset,
                    next_workset_id,
                }],
            })
        }
        Event::AttachWorkset {
            item_id,
            root_directory,
            repositories,
        } => {
            let root_directory = clean_name(root_directory, DomainError::EmptyWorksetRoot)?;
            let project_id = item_project_id(&state, item_id)?;
            if repositories.is_empty() {
                return Err(DomainError::EmptyWorksetRepositories);
            }

            let mut attached_repositories = Vec::with_capacity(repositories.len());
            let mut new_repositories = Vec::new();
            let mut next_repository_id = state.next_repository_id;
            let mut branch = None;
            for input in repositories {
                let name = clean_name(input.name, DomainError::EmptyRepositoryName)?;
                if name == "." || name == ".." || name.contains('/') || name.contains('\\') {
                    return Err(DomainError::InvalidRepositoryName);
                }
                if attached_repositories
                    .iter()
                    .any(|repository: &WorksetRepository| {
                        state
                            .repositories
                            .iter()
                            .find(|candidate| candidate.id == repository.repository_id)
                            .is_some_and(|candidate| candidate.name == name)
                    })
                {
                    return Err(DomainError::RepositoryNameTaken { project_id, name });
                }
                let remote_url =
                    clean_name(input.remote_url, DomainError::EmptyRepositoryRemoteUrl)?;
                let current_branch =
                    clean_name(input.current_branch, DomainError::EmptyWorksetBranch)?;
                let repository = state
                    .repositories
                    .iter()
                    .find(|repository| {
                        repository.project_id == project_id && repository.name == name
                    })
                    .cloned();
                let repository = match repository {
                    Some(repository) if repository.remote_url != remote_url => {
                        return Err(DomainError::RepositoryRemoteMismatch { project_id, name });
                    }
                    Some(repository) => repository,
                    None => {
                        let id = next_repository_id;
                        next_repository_id =
                            id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
                        let repository = Repository {
                            id,
                            project_id,
                            name,
                            remote_url,
                        };
                        state.repositories.push(repository.clone());
                        new_repositories.push((repository.clone(), next_repository_id));
                        repository
                    }
                };
                branch.get_or_insert_with(|| current_branch.clone());
                attached_repositories.push(WorksetRepository {
                    repository_id: repository.id,
                    branch_override: None,
                    base_branch_override: None,
                    current_branch,
                    is_dirty: input.is_dirty,
                });
            }

            let id = state.next_workset_id;
            let next_workset_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let branch = branch.expect("a non-empty attachment has a branch");
            for repository in &mut attached_repositories {
                if repository.current_branch != branch
                    && !repository.current_branch.starts_with("HEAD (detached at ")
                {
                    repository.branch_override = Some(repository.current_branch.clone());
                }
            }
            let workset = Workset {
                id,
                item_id,
                root_directory,
                branch,
                archived: false,
                repositories: attached_repositories,
            };
            state.next_repository_id = next_repository_id;
            state.next_workset_id = next_workset_id;
            state.worksets.push(workset.clone());

            let mut effects = new_repositories
                .into_iter()
                .map(
                    |(repository, next_repository_id)| Effect::PersistRepository {
                        repository,
                        next_repository_id,
                    },
                )
                .collect::<Vec<_>>();
            effects.push(Effect::PersistWorkset {
                workset,
                next_workset_id,
            });
            Ok(Decision { state, effects })
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
        Event::AddRepositoryToWorkset {
            workset_id,
            repository_id,
            branch_override,
            base_branch_override,
        } => {
            let workset = state
                .worksets
                .iter()
                .find(|workset| workset.id == workset_id)
                .cloned()
                .ok_or(DomainError::WorksetNotFound { workset_id })?;
            let project_id = item_project_id(&state, workset.item_id)?;
            let repository = state
                .repositories
                .iter()
                .find(|repository| repository.id == repository_id)
                .ok_or(DomainError::RepositoryNotFound { repository_id })?;
            if repository.project_id != project_id {
                return Err(DomainError::RepositoryProjectMismatch {
                    repository_id,
                    project_id,
                });
            }
            if workset
                .repositories
                .iter()
                .any(|selected| selected.repository_id == repository_id)
            {
                return Err(DomainError::RepositoryAlreadyInWorkset {
                    repository_id,
                    workset_id,
                });
            }
            let branch_override = clean_optional_branch(branch_override)?;
            let current_branch = branch_override
                .as_deref()
                .unwrap_or(&workset.branch)
                .to_owned();
            let selected = WorksetRepository {
                repository_id,
                branch_override,
                base_branch_override: clean_optional_branch(base_branch_override)?,
                current_branch,
                is_dirty: false,
            };
            let workset = state
                .worksets
                .iter_mut()
                .find(|candidate| candidate.id == workset_id)
                .expect("the Workset was checked above");
            workset.repositories.push(selected);
            let workset = workset.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistWorksetUpdate { workset }],
            })
        }
        Event::SetWorksetArchived {
            workset_id,
            archived,
        } => {
            let workset = state
                .worksets
                .iter_mut()
                .find(|workset| workset.id == workset_id)
                .ok_or(DomainError::WorksetNotFound { workset_id })?;
            workset.archived = archived;
            let workset = workset.clone();

            Ok(Decision {
                state,
                effects: vec![Effect::PersistWorksetUpdate { workset }],
            })
        }
        Event::RemoveWorkset { workset_id } => {
            let position = state
                .worksets
                .iter()
                .position(|workset| workset.id == workset_id)
                .ok_or(DomainError::WorksetNotFound { workset_id })?;
            if state.runs.iter().any(|run| run.workset_id == workset_id) {
                return Err(DomainError::WorksetHasRuns { workset_id });
            }
            state.worksets.remove(position);

            Ok(Decision {
                state,
                effects: vec![Effect::RemoveWorkset { workset_id }],
            })
        }
        Event::StartRun {
            item_id,
            workset_id,
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
            let workset = state
                .worksets
                .iter()
                .find(|workset| workset.id == workset_id)
                .ok_or(DomainError::WorksetNotFound { workset_id })?;
            if workset.item_id != item_id {
                return Err(DomainError::WorksetItemMismatch {
                    workset_id,
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
            if working_directory != workset.root_directory {
                return Err(DomainError::RunWorkingDirectoryMismatch { workset_id });
            }
            let session_name = clean_name(session_name, DomainError::EmptyRunSessionName)?;
            let pane_id = clean_name(pane_id, DomainError::EmptyRunPaneId)?;
            let id = state.next_run_id;
            let next_run_id = id.checked_add(1).ok_or(DomainError::SequenceExhausted)?;
            let run = Run {
                id,
                item_id,
                workset_id,
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
            workset_id,
            machine_id,
            agent,
            working_directory,
            session_name,
            pane_id,
            attached_at,
        } => {
            let context_id = item_context_id(&state, item_id)?;
            let workset = state
                .worksets
                .iter()
                .find(|workset| workset.id == workset_id)
                .ok_or(DomainError::WorksetNotFound { workset_id })?;
            if workset.item_id != item_id {
                return Err(DomainError::WorksetItemMismatch {
                    workset_id,
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
            let working_directory =
                clean_name(working_directory, DomainError::EmptyRunWorkingDirectory)?;
            if working_directory != workset.root_directory {
                return Err(DomainError::RunWorkingDirectoryMismatch { workset_id });
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
                workset_id,
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
            let workset = state
                .worksets
                .iter()
                .filter(|workset| {
                    !workset.archived
                        && path_is_within_workset(&workset.root_directory, &pane.current_path)
                })
                .max_by_key(|workset| workset.root_directory.len())?;
            let item = state.items.iter().find(|item| item.id == workset.item_id)?;
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
                workset_id: workset.id,
                workset_root_directory: workset.root_directory.clone(),
                workset_branch: workset.branch.clone(),
                item_id: item.id,
                item_identifier: item.human_identifier.clone(),
                item_title: item.title.clone(),
                context_id: context.id,
                context_name: context.name.clone(),
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

fn path_is_within_workset(root: &str, path: &str) -> bool {
    let root = without_macos_private_prefix(root).trim_end_matches('/');
    let path = without_macos_private_prefix(path);
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
            let (worksets, archived_worksets): (Vec<_>, Vec<_>) = state
                .worksets
                .iter()
                .filter(|workset| workset.item_id == item.id)
                .cloned()
                .partition(|workset| !workset.archived);
            let runs = state
                .runs
                .iter()
                .filter(|run| run.item_id == item.id)
                .cloned()
                .collect();
            Some(ItemView {
                item: item.clone(),
                context_id: context.id,
                context_name: context.name.clone(),
                project_name: project.name.clone(),
                relationships,
                worksets,
                archived_worksets,
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
            "Implement this work in the Workset, run the relevant checks, and leave the changes ready for review."
                .to_owned()
        }
        ExecutionProfile::Review => {
            "Review the current Workset changes for correctness, regressions, and missing test coverage."
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

fn normalize_workset_repositories(
    state: &DomainState,
    project_id: i64,
    branch: &str,
    repositories: Vec<WorksetRepositoryInput>,
) -> Result<Vec<WorksetRepository>, DomainError> {
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
            .any(|selected: &WorksetRepository| selected.repository_id == input.repository_id)
        {
            return Err(DomainError::DuplicateRepositorySelection {
                repository_id: input.repository_id,
            });
        }
        normalized.push(WorksetRepository {
            repository_id: input.repository_id,
            current_branch: input
                .branch_override
                .as_deref()
                .unwrap_or(branch)
                .to_owned(),
            branch_override: clean_optional_branch(input.branch_override)?,
            base_branch_override: clean_optional_branch(input.base_branch_override)?,
            is_dirty: false,
        });
    }
    Ok(normalized)
}

fn clean_optional_branch(branch: Option<String>) -> Result<Option<String>, DomainError> {
    branch
        .map(|branch| clean_name(branch, DomainError::EmptyWorksetBranch))
        .transpose()
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
mod tests {
    use super::*;

    #[test]
    fn creating_a_context_creates_its_default_project() {
        let decision = decide(
            empty_state(),
            Event::CreateContext {
                name: "Work".into(),
            },
        )
        .expect("Context creation should succeed");

        assert_eq!(
            decision.state.contexts,
            vec![Context {
                id: 1,
                name: "Work".into(),
            }]
        );
        assert_eq!(
            decision.state.projects,
            vec![Project {
                id: 1,
                context_id: 1,
                name: "Default".into(),
                defaults: ProjectDefaults {
                    item_status: ItemStatus::Inbox,
                },
            }]
        );
        assert_eq!(
            decision.effects,
            vec![
                Effect::PersistContext {
                    context: decision.state.contexts[0].clone(),
                    next_context_id: 2,
                },
                Effect::PersistProject {
                    project: decision.state.projects[0].clone(),
                    next_project_id: 2,
                },
            ]
        );
    }

    #[test]
    fn creating_a_project_keeps_its_item_defaults() {
        let decision = decide(
            state_with_context(7, "Work"),
            Event::CreateProject {
                context_id: 7,
                name: "Billing".into(),
                defaults: ProjectDefaults {
                    item_status: ItemStatus::Active,
                },
            },
        )
        .expect("Project creation should succeed");

        assert_eq!(
            decision.state.projects[1],
            Project {
                id: 2,
                context_id: 7,
                name: "Billing".into(),
                defaults: ProjectDefaults {
                    item_status: ItemStatus::Active,
                },
            }
        );
    }

    #[test]
    fn creating_an_item_assigns_the_project_and_inherits_its_defaults() {
        let mut state = state_with_context(7, "Work");
        state.projects.push(Project {
            id: 2,
            context_id: 7,
            name: "Billing".into(),
            defaults: ProjectDefaults {
                item_status: ItemStatus::Active,
            },
        });
        state.next_project_id = 3;

        let decision = decide(
            state,
            Event::CreateItem {
                title: "Investigate timeout".into(),
                context_id: 7,
                project_id: 2,
            },
        )
        .expect("item creation should succeed");

        assert_eq!(decision.state.items.len(), 1);
        assert_eq!(
            decision.state.items[0],
            Item {
                id: 1,
                human_identifier: "MC-1".into(),
                title: "Investigate timeout".into(),
                project_id: 2,
                status: ItemStatus::Active,
                notes: String::new(),
                reminders: Vec::new(),
            }
        );
        assert_eq!(decision.state.next_item_number, 2);
        assert_eq!(decision.state.next_item_id, 2);
        assert_eq!(
            decision.effects,
            vec![Effect::PersistItem {
                item: decision.state.items[0].clone(),
                next_item_number: 2,
                next_item_id: 2,
            }]
        );
    }

    #[test]
    fn item_identifiers_use_one_global_sequence_across_projects() {
        let state = state_with_contexts(&[(7, "Work"), (8, "Personal")]);

        let first = decide(
            state,
            Event::CreateItem {
                title: "Work item".into(),
                context_id: 7,
                project_id: 1,
            },
        )
        .expect("first item should succeed");
        let second = decide(
            first.state,
            Event::CreateItem {
                title: "Personal item".into(),
                context_id: 8,
                project_id: 2,
            },
        )
        .expect("second item should succeed");

        assert_eq!(second.state.items[0].human_identifier, "MC-1");
        assert_eq!(second.state.items[1].human_identifier, "MC-2");
    }

    #[test]
    fn an_action_cannot_cross_contexts() {
        let state = state_with_contexts(&[(7, "Work"), (8, "Personal")]);

        assert_eq!(
            decide(
                state,
                Event::CreateItem {
                    title: "Wrong boundary".into(),
                    context_id: 7,
                    project_id: 2,
                },
            ),
            Err(DomainError::ProjectContextMismatch {
                project_id: 2,
                context_id: 7,
            })
        );
    }

    #[test]
    fn creation_rejects_blank_names_titles_and_unknown_owners() {
        let state = state_with_context(7, "Work");

        assert_eq!(
            decide(state.clone(), Event::CreateContext { name: "   ".into() },),
            Err(DomainError::EmptyContextName)
        );
        assert_eq!(
            decide(
                state.clone(),
                Event::CreateProject {
                    context_id: 99,
                    name: "No context".into(),
                    defaults: ProjectDefaults {
                        item_status: ItemStatus::Inbox,
                    },
                },
            ),
            Err(DomainError::ContextNotFound { context_id: 99 })
        );
        assert_eq!(
            decide(
                state.clone(),
                Event::CreateItem {
                    title: "   ".into(),
                    context_id: 7,
                    project_id: 1,
                },
            ),
            Err(DomainError::EmptyTitle)
        );
        assert_eq!(
            decide(
                state.clone(),
                Event::CreateItem {
                    title: "No context".into(),
                    context_id: 99,
                    project_id: 1,
                },
            ),
            Err(DomainError::ContextNotFound { context_id: 99 })
        );
        assert_eq!(
            decide(
                state,
                Event::CreateItem {
                    title: "No project".into(),
                    context_id: 7,
                    project_id: 99,
                },
            ),
            Err(DomainError::ProjectNotFound { project_id: 99 })
        );
    }

    #[test]
    fn an_item_can_move_between_statuses_in_any_order() {
        let mut state = state_with_context(7, "Work");
        state.items.push(Item {
            id: 1,
            human_identifier: "MC-1".into(),
            title: "Ship the change".into(),
            project_id: 1,
            status: ItemStatus::Inbox,
            notes: String::new(),
            reminders: Vec::new(),
        });
        state.next_item_id = 2;
        state.next_item_number = 2;

        let active = decide(
            state,
            Event::SetItemStatus {
                item_id: 1,
                status: ItemStatus::Active,
            },
        )
        .expect("an Item should move to Active");
        assert_eq!(active.state.items[0].status, ItemStatus::Active);
        assert_eq!(
            active.effects,
            vec![Effect::PersistItemUpdate {
                item: active.state.items[0].clone(),
            }]
        );

        let waiting = decide(
            active.state,
            Event::SetItemStatus {
                item_id: 1,
                status: ItemStatus::Waiting,
            },
        )
        .expect("an Item should move to Waiting");
        assert_eq!(waiting.state.items[0].status, ItemStatus::Waiting);

        let done = decide(
            waiting.state,
            Event::SetItemStatus {
                item_id: 1,
                status: ItemStatus::Done,
            },
        )
        .expect("an Item should move to Done");
        let inbox = decide(
            done.state,
            Event::SetItemStatus {
                item_id: 1,
                status: ItemStatus::Inbox,
            },
        )
        .expect("a Done Item should be able to return to Inbox");
        assert_eq!(inbox.state.items[0].status, ItemStatus::Inbox);
    }

    #[test]
    fn an_item_can_record_notes_and_relationships_within_its_context() {
        let mut state = state_with_context(7, "Work");
        state.items = vec![
            Item {
                id: 1,
                human_identifier: "MC-1".into(),
                title: "Ship the change".into(),
                project_id: 1,
                status: ItemStatus::Active,
                notes: String::new(),
                reminders: Vec::new(),
            },
            Item {
                id: 2,
                human_identifier: "MC-2".into(),
                title: "Prepare the release".into(),
                project_id: 1,
                status: ItemStatus::Waiting,
                notes: String::new(),
                reminders: Vec::new(),
            },
        ];
        state.next_item_id = 3;
        state.next_item_number = 3;

        let noted = decide(
            state,
            Event::SetItemNotes {
                item_id: 1,
                notes: "Release after the migration is verified.".into(),
            },
        )
        .expect("notes should be saved");
        assert_eq!(
            noted.state.items[0].notes,
            "Release after the migration is verified."
        );
        assert_eq!(
            noted.effects,
            vec![Effect::PersistItemUpdate {
                item: noted.state.items[0].clone(),
            }]
        );

        let related = decide(
            noted.state,
            Event::SetItemRelation {
                from_item_id: 1,
                to_item_id: 2,
                kind: ItemRelationKind::Blocks,
            },
        )
        .expect("Items in one Context should be related");
        assert_eq!(
            related.state.relationships,
            vec![ItemRelation {
                from_item_id: 1,
                to_item_id: 2,
                kind: ItemRelationKind::Blocks,
            }]
        );
        assert_eq!(
            related.effects,
            vec![Effect::PersistItemRelation {
                relation: related.state.relationships[0].clone(),
            }]
        );
    }

    #[test]
    fn relationships_cannot_cross_contexts_or_point_to_themselves() {
        let mut state = state_with_contexts(&[(7, "Work"), (8, "Personal")]);
        state.items = vec![
            Item {
                id: 1,
                human_identifier: "MC-1".into(),
                title: "Work item".into(),
                project_id: 1,
                status: ItemStatus::Inbox,
                notes: String::new(),
                reminders: Vec::new(),
            },
            Item {
                id: 2,
                human_identifier: "MC-2".into(),
                title: "Personal item".into(),
                project_id: 2,
                status: ItemStatus::Inbox,
                notes: String::new(),
                reminders: Vec::new(),
            },
        ];

        assert_eq!(
            decide(
                state.clone(),
                Event::SetItemRelation {
                    from_item_id: 1,
                    to_item_id: 2,
                    kind: ItemRelationKind::BlockedBy,
                },
            ),
            Err(DomainError::ItemContextMismatch {
                from_item_id: 1,
                to_item_id: 2,
            })
        );
        assert_eq!(
            decide(
                state,
                Event::SetItemRelation {
                    from_item_id: 1,
                    to_item_id: 1,
                    kind: ItemRelationKind::RelatedTo,
                },
            ),
            Err(DomainError::SelfRelation { item_id: 1 })
        );
    }

    #[test]
    fn home_view_groups_items_and_searches_across_contexts() {
        let mut state = state_with_contexts(&[(7, "Work"), (8, "Personal")]);
        state.items = vec![
            Item {
                id: 1,
                human_identifier: "MC-1".into(),
                title: "Reply to the design review".into(),
                project_id: 1,
                status: ItemStatus::Inbox,
                notes: "Needs a decision".into(),
                reminders: Vec::new(),
            },
            Item {
                id: 2,
                human_identifier: "MC-2".into(),
                title: "Implement the parser".into(),
                project_id: 1,
                status: ItemStatus::Active,
                notes: String::new(),
                reminders: vec![Reminder {
                    id: 1,
                    remind_at: "2026-09-18T09:00".into(),
                }],
            },
            Item {
                id: 3,
                human_identifier: "MC-3".into(),
                title: "Wait for approval".into(),
                project_id: 2,
                status: ItemStatus::Waiting,
                notes: String::new(),
                reminders: Vec::new(),
            },
            Item {
                id: 4,
                human_identifier: "MC-4".into(),
                title: "Archive the old plan".into(),
                project_id: 2,
                status: ItemStatus::Done,
                notes: String::new(),
                reminders: vec![Reminder {
                    id: 2,
                    remind_at: "2026-09-17T09:00".into(),
                }],
            },
        ];

        let view = home_view(&state, None, "2026-09-19T09:00");
        assert_eq!(view.needs_attention[0].item.id, 1);
        assert_eq!(view.needs_attention[1].item.id, 2);
        assert_eq!(view.running[0].item.id, 2);
        assert_eq!(view.waiting[0].item.id, 3);
        assert_eq!(view.due[0].item.id, 2);
        assert_eq!(view.completed[0].item.id, 4);
        assert_eq!(view.attention_entries.len(), 2);
        assert!(view
            .attention_entries
            .iter()
            .all(|entry| entry.kind == AttentionEntryKind::Reminder));
        assert_eq!(view.running[0].context_name, "Work");

        let personal = home_view(&state, Some(8), "2026-09-19T09:00");
        assert!(personal.needs_attention.is_empty());
        assert_eq!(personal.attention_entries.len(), 1);
        assert_eq!(personal.waiting[0].context_name, "Personal");

        let results = search_items(&state, "design", None);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].item.human_identifier, "MC-1");
        assert_eq!(results[0].context_name, "Work");

        let all_personal = search_items(&state, "", Some(8));
        assert_eq!(all_personal.len(), 2);
        assert!(all_personal
            .iter()
            .all(|result| result.context_name == "Personal"));
    }

    #[test]
    fn linking_a_github_object_creates_one_object_and_one_link_with_a_snapshot() {
        let state = state_with_item(7, "Work");
        let decision = decide(
            state,
            Event::LinkExternalObject {
                item_id: 1,
                object: ExternalObjectInput {
                    provider: ExternalProvider::GitHub,
                    kind: ExternalObjectKind::PullRequest,
                    external_key: "pull:acme/app#42".into(),
                    canonical_url: "https://github.com/acme/app/pull/42".into(),
                },
                snapshot: Some(ExternalSnapshotData {
                    title: "Ship the parser".into(),
                    state: "OPEN".into(),
                    metadata: vec![ExternalMetadata {
                        key: "author".into(),
                        value: "octocat".into(),
                    }],
                    fetched_at: 100,
                }),
            },
        )
        .expect("a GitHub object should link to an Item");

        assert_eq!(decision.state.external_objects.len(), 1);
        assert_eq!(decision.state.links.len(), 1);
        assert_eq!(decision.state.snapshots.len(), 1);
        assert_eq!(decision.state.links[0].item_id, 1);
        assert_eq!(decision.state.links[0].external_object_id, 1);
        assert_eq!(decision.state.snapshots[0].title, "Ship the parser");
        let view = search_items(&decision.state, "", None);
        assert_eq!(view[0].links.len(), 1);
        assert_eq!(
            view[0].links[0].snapshot.as_ref().unwrap().title,
            "Ship the parser"
        );
        assert_eq!(
            decision.effects,
            vec![
                Effect::PersistExternalObject {
                    object: decision.state.external_objects[0].clone(),
                    next_external_object_id: 2,
                },
                Effect::PersistLink {
                    link: decision.state.links[0].clone(),
                    next_link_id: 2,
                },
                Effect::PersistExternalSnapshot {
                    snapshot: decision.state.snapshots[0].clone(),
                },
            ]
        );
    }

    #[test]
    fn two_items_share_one_external_object_but_keep_two_links_and_fetch_once() {
        let mut state = state_with_context(7, "Work");
        state.items = vec![
            Item {
                id: 1,
                human_identifier: "MC-1".into(),
                title: "First commitment".into(),
                project_id: 1,
                status: ItemStatus::Inbox,
                notes: String::new(),
                reminders: Vec::new(),
            },
            Item {
                id: 2,
                human_identifier: "MC-2".into(),
                title: "Second commitment".into(),
                project_id: 1,
                status: ItemStatus::Inbox,
                notes: String::new(),
                reminders: Vec::new(),
            },
        ];
        state.next_item_id = 3;
        state.next_item_number = 3;
        let object = ExternalObjectInput {
            provider: ExternalProvider::GitHub,
            kind: ExternalObjectKind::Issue,
            external_key: "issue:acme/app#7".into(),
            canonical_url: "https://github.com/acme/app/issues/7".into(),
        };

        let first = decide(
            state,
            Event::LinkExternalObject {
                item_id: 1,
                object: object.clone(),
                snapshot: Some(snapshot_data("Shared issue", 100)),
            },
        )
        .expect("the first Link should succeed");
        let second = decide(
            first.state,
            Event::LinkExternalObject {
                item_id: 2,
                object,
                snapshot: None,
            },
        )
        .expect("the second Link should reuse the object");

        assert_eq!(second.state.external_objects.len(), 1);
        assert_eq!(second.state.links.len(), 2);
        assert_eq!(second.state.snapshots.len(), 1);
        assert_eq!(second.effects.len(), 1);
        assert!(matches!(second.effects[0], Effect::PersistLink { .. }));
    }

    #[test]
    fn an_unrecognised_url_is_stored_as_a_generic_link_and_an_item_can_have_many_links() {
        let state = state_with_item(7, "Work");
        let first = decide(
            state,
            Event::LinkExternalObject {
                item_id: 1,
                object: ExternalObjectInput {
                    provider: ExternalProvider::Generic,
                    kind: ExternalObjectKind::Generic,
                    external_key: "https://example.com/design".into(),
                    canonical_url: "https://example.com/design".into(),
                },
                snapshot: None,
            },
        )
        .expect("an unrecognised URL should still link");
        let second = decide(
            first.state,
            Event::LinkExternalObject {
                item_id: 1,
                object: ExternalObjectInput {
                    provider: ExternalProvider::Generic,
                    kind: ExternalObjectKind::Generic,
                    external_key: "https://example.com/spec".into(),
                    canonical_url: "https://example.com/spec".into(),
                },
                snapshot: None,
            },
        )
        .expect("one Item should accept several Links");

        assert_eq!(second.state.external_objects.len(), 2);
        assert_eq!(second.state.links.len(), 2);
        assert!(second.state.snapshots.is_empty());
        assert_eq!(second.state.links[0].item_id, 1);
        assert_eq!(second.state.links[1].item_id, 1);
    }

    #[test]
    fn refreshing_an_external_object_replaces_its_snapshot_without_touching_the_link() {
        let linked = decide(
            state_with_item(7, "Work"),
            Event::LinkExternalObject {
                item_id: 1,
                object: ExternalObjectInput {
                    provider: ExternalProvider::GitHub,
                    kind: ExternalObjectKind::Issue,
                    external_key: "issue:acme/app#7".into(),
                    canonical_url: "https://github.com/acme/app/issues/7".into(),
                },
                snapshot: Some(snapshot_data("Old title", 100)),
            },
        )
        .expect("the Link should exist");
        let refreshed = decide(
            linked.state,
            Event::RefreshExternalObject {
                external_object_id: 1,
                snapshot: snapshot_data("New title", 200),
            },
        )
        .expect("a fresh snapshot should replace the old one");

        assert_eq!(refreshed.state.links.len(), 1);
        assert_eq!(refreshed.state.snapshots[0].title, "New title");
        assert_eq!(refreshed.state.snapshots[0].fetched_at, 200);
        assert_eq!(
            refreshed.effects,
            vec![
                Effect::PersistActivity {
                    activity: refreshed.state.activities[0].clone(),
                    next_activity_id: 2,
                },
                Effect::PersistExternalSnapshot {
                    snapshot: refreshed.state.snapshots[0].clone(),
                },
            ]
        );
    }

    #[test]
    fn refreshing_a_changed_object_records_activity_and_one_attention_entry_per_link() {
        let linked = decide(
            state_with_item(7, "Work"),
            Event::LinkExternalObject {
                item_id: 1,
                object: ExternalObjectInput {
                    provider: ExternalProvider::GitHub,
                    kind: ExternalObjectKind::Issue,
                    external_key: "issue:acme/app#7".into(),
                    canonical_url: "https://github.com/acme/app/issues/7".into(),
                },
                snapshot: Some(ExternalSnapshotData {
                    title: "Old title".into(),
                    state: "OPEN".into(),
                    metadata: vec![ExternalMetadata {
                        key: "author".into(),
                        value: "octocat".into(),
                    }],
                    fetched_at: 100,
                }),
            },
        )
        .expect("the object should link to the Item");

        let refreshed = decide(
            linked.state,
            Event::RefreshExternalObject {
                external_object_id: 1,
                snapshot: ExternalSnapshotData {
                    title: "New title".into(),
                    state: "CLOSED".into(),
                    metadata: vec![ExternalMetadata {
                        key: "author".into(),
                        value: "octocat".into(),
                    }],
                    fetched_at: 200,
                },
            },
        )
        .expect("the changed snapshot should be recorded");

        assert_eq!(refreshed.state.activities.len(), 1);
        assert_eq!(refreshed.state.activities[0].changes.len(), 2);
        assert_eq!(
            refreshed.state.activities[0].changes[0].kind,
            ExternalChangeKind::Title
        );
        assert_eq!(
            refreshed.state.activities[0].changes[1].kind,
            ExternalChangeKind::State
        );
        let view = home_view(&refreshed.state, None, "2026-09-20T00:00");
        assert_eq!(view.attention_entries.len(), 1);
        assert_eq!(view.attention_entries[0].activities.len(), 1);
        assert!(view.attention_entries[0].summary.contains("State changed"));
        assert_eq!(
            refreshed.state.links[0].reviewed_activity_id, 0,
            "refreshing must not review the Link"
        );
    }

    #[test]
    fn attention_defaults_and_link_overrides_filter_activity_without_losing_history() {
        let configured = decide(
            state_with_item(7, "Work"),
            Event::SetContextAttentionDefault {
                context_id: 7,
                object_kind: ExternalObjectKind::Issue,
                policy: ExternalChangePolicy {
                    title: false,
                    state: true,
                    metadata: false,
                },
            },
        )
        .expect("the Context default should be configurable");
        let linked = decide(
            configured.state,
            Event::LinkExternalObject {
                item_id: 1,
                object: ExternalObjectInput {
                    provider: ExternalProvider::GitHub,
                    kind: ExternalObjectKind::Issue,
                    external_key: "issue:acme/app#7".into(),
                    canonical_url: "https://github.com/acme/app/issues/7".into(),
                },
                snapshot: Some(ExternalSnapshotData {
                    title: "Old title".into(),
                    state: "OPEN".into(),
                    metadata: vec![ExternalMetadata {
                        key: "author".into(),
                        value: "octocat".into(),
                    }],
                    fetched_at: 100,
                }),
            },
        )
        .expect("the object should link to the Item");
        let refreshed = decide(
            linked.state,
            Event::RefreshExternalObject {
                external_object_id: 1,
                snapshot: ExternalSnapshotData {
                    title: "New title".into(),
                    state: "CLOSED".into(),
                    metadata: vec![ExternalMetadata {
                        key: "author".into(),
                        value: "someone-else".into(),
                    }],
                    fetched_at: 200,
                },
            },
        )
        .expect("the changed snapshot should be recorded");

        let view = home_view(&refreshed.state, None, "2026-09-20T00:00");
        assert_eq!(view.attention_entries.len(), 1);
        assert_eq!(view.attention_entries[0].activities[0].changes.len(), 1);
        assert_eq!(
            view.attention_entries[0].activities[0].changes[0].kind,
            ExternalChangeKind::State
        );
        assert_eq!(refreshed.state.activities[0].changes.len(), 3);

        let overridden = decide(
            refreshed.state,
            Event::SetLinkAttentionPolicy {
                link_id: 1,
                policy: Some(ExternalChangePolicy {
                    title: true,
                    state: false,
                    metadata: false,
                }),
            },
        )
        .expect("the Link policy should be configurable");
        let overridden_view = home_view(&overridden.state, None, "2026-09-20T00:00");
        assert_eq!(overridden_view.attention_entries.len(), 1);
        assert_eq!(
            overridden_view.attention_entries[0].activities[0].changes[0].kind,
            ExternalChangeKind::Title
        );
    }

    #[test]
    fn marking_one_link_reviewed_leaves_the_other_link_for_the_same_object_unreviewed() {
        let mut state = state_with_item(7, "Work");
        state.items.push(Item {
            id: 2,
            human_identifier: "MC-2".into(),
            title: "Second Item".into(),
            project_id: 1,
            status: ItemStatus::Waiting,
            notes: String::new(),
            reminders: Vec::new(),
        });
        state.next_item_id = 3;
        state.next_item_number = 3;
        let object = ExternalObjectInput {
            provider: ExternalProvider::GitHub,
            kind: ExternalObjectKind::PullRequest,
            external_key: "pull_request:acme/app#7".into(),
            canonical_url: "https://github.com/acme/app/pull/7".into(),
        };
        let first = decide(
            state,
            Event::LinkExternalObject {
                item_id: 1,
                object: object.clone(),
                snapshot: Some(snapshot_data("Old title", 100)),
            },
        )
        .expect("the first Link should be created");
        let second = decide(
            first.state,
            Event::LinkExternalObject {
                item_id: 2,
                object,
                snapshot: None,
            },
        )
        .expect("the second Link should be created");
        let refreshed = decide(
            second.state,
            Event::RefreshExternalObject {
                external_object_id: 1,
                snapshot: snapshot_data("New title", 200),
            },
        )
        .expect("the shared object should refresh once");
        assert_eq!(
            home_view(&refreshed.state, None, "now")
                .attention_entries
                .len(),
            2
        );

        let reviewed = decide(refreshed.state, Event::MarkLinkReviewed { link_id: 1 })
            .expect("mark reviewed should be explicit");
        let entries = home_view(&reviewed.state, None, "now").attention_entries;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].link_id, 2);
        assert_eq!(reviewed.state.links[0].reviewed_activity_id, 1);
        assert_eq!(reviewed.state.links[1].reviewed_activity_id, 0);
    }

    #[test]
    fn an_item_can_carry_multiple_reminders_and_due_reminders_need_attention() {
        let first = decide(
            state_with_item(7, "Work"),
            Event::AddItemReminder {
                item_id: 1,
                remind_at: "2026-09-18T09:00".into(),
            },
        )
        .expect("the first Reminder should be added");
        let second = decide(
            first.state,
            Event::AddItemReminder {
                item_id: 1,
                remind_at: "2026-09-25T09:00".into(),
            },
        )
        .expect("the second Reminder should be added");

        let view = home_view(&second.state, None, "2026-09-19T09:00");
        assert_eq!(second.state.items[0].reminders.len(), 2);
        assert_eq!(view.attention_entries.len(), 1);
        assert_eq!(view.attention_entries[0].kind, AttentionEntryKind::Reminder);
        assert_eq!(
            view.attention_entries[0].reminder_id,
            Some(second.state.items[0].reminders[0].id)
        );
        assert_eq!(view.needs_attention[0].item.id, 1);
        assert_eq!(second.state.items[0].status, ItemStatus::Inbox);

        let removed = decide(
            second.state,
            Event::RemoveItemReminder {
                item_id: 1,
                reminder_id: 1,
            },
        )
        .expect("removing one Reminder should leave the other one intact");
        assert_eq!(removed.state.items[0].reminders.len(), 1);
        assert_eq!(
            removed.state.items[0].reminders[0].remind_at,
            "2026-09-25T09:00"
        );
    }

    #[test]
    fn watch_until_and_review_at_are_independent_and_review_date_needs_attention() {
        let linked = decide(
            state_with_item(7, "Work"),
            Event::LinkExternalObject {
                item_id: 1,
                object: ExternalObjectInput {
                    provider: ExternalProvider::Generic,
                    kind: ExternalObjectKind::Generic,
                    external_key: "https://example.com/spec".into(),
                    canonical_url: "https://example.com/spec".into(),
                },
                snapshot: Some(snapshot_data("Spec", 100)),
            },
        )
        .expect("the External Object should be linkable without execution");
        let watched = decide(
            linked.state,
            Event::SetLinkWatchUntil {
                link_id: 1,
                watch_until: Some("2026-09-20T09:00".into()),
            },
        )
        .expect("watch_until should be configurable");
        let scheduled = decide(
            watched.state,
            Event::SetLinkReviewAt {
                link_id: 1,
                review_at: Some("2026-09-25T09:00".into()),
            },
        )
        .expect("review_at should be configurable independently");

        assert_eq!(
            scheduled.state.links[0].watch_until.as_deref(),
            Some("2026-09-20T09:00")
        );
        assert_eq!(
            scheduled.state.links[0].review_at.as_deref(),
            Some("2026-09-25T09:00")
        );

        let refreshed = decide(
            scheduled.state,
            Event::RefreshExternalObject {
                external_object_id: 1,
                snapshot: snapshot_data("Changed spec", 200),
            },
        )
        .expect("the watched External Object should still record Activity");
        let after_watch = home_view(&refreshed.state, None, "2026-09-21T09:00");
        assert_eq!(refreshed.state.activities.len(), 1);
        assert_eq!(after_watch.attention_entries.len(), 0);

        let at_review = home_view(&refreshed.state, None, "2026-09-25T09:00");
        assert_eq!(at_review.attention_entries.len(), 1);
        assert_eq!(
            at_review.attention_entries[0].kind,
            AttentionEntryKind::Review
        );
        assert_eq!(at_review.attention_entries[0].link_id, 1);
        assert_eq!(refreshed.state.items[0].status, ItemStatus::Inbox);

        let watch_extended = decide(
            refreshed.state.clone(),
            Event::SetLinkWatchUntil {
                link_id: 1,
                watch_until: Some("2026-09-30T09:00".into()),
            },
        )
        .expect("the watch period should remain independent");
        let both_due = home_view(&watch_extended.state, None, "2026-09-25T09:00");
        assert_eq!(both_due.attention_entries.len(), 2);
        assert!(both_due
            .attention_entries
            .iter()
            .any(|entry| entry.kind == AttentionEntryKind::ExternalChange));
        assert!(both_due
            .attention_entries
            .iter()
            .any(|entry| entry.kind == AttentionEntryKind::Review));
        let changes_only = decide(
            watch_extended.state,
            Event::ClearLinkReviewAt { link_id: 1 },
        )
        .expect("clearing the review date should leave change attention alone");
        let changes_only_view = home_view(&changes_only.state, None, "2026-09-25T09:00");
        assert_eq!(changes_only_view.attention_entries.len(), 1);
        assert_eq!(
            changes_only_view.attention_entries[0].kind,
            AttentionEntryKind::ExternalChange
        );

        let cleared = decide(refreshed.state, Event::ClearLinkReviewAt { link_id: 1 })
            .expect("the reached review date should be dismissible explicitly");
        assert!(home_view(&cleared.state, None, "2026-09-25T00:00")
            .attention_entries
            .is_empty());
        assert_eq!(
            cleared.state.links[0].watch_until.as_deref(),
            Some("2026-09-20T09:00")
        );
        assert_eq!(cleared.state.items[0].status, ItemStatus::Inbox);
    }

    #[test]
    fn registering_a_repository_keeps_it_under_its_project() {
        let decision = decide(
            state_with_item(7, "Work"),
            Event::RegisterRepository {
                project_id: 1,
                name: "service-a".into(),
                remote_url: "git@github.com:acme/service-a.git".into(),
            },
        )
        .expect("repository registration should succeed");

        assert_eq!(
            decision.state.repositories,
            vec![Repository {
                id: 1,
                project_id: 1,
                name: "service-a".into(),
                remote_url: "git@github.com:acme/service-a.git".into(),
            }]
        );
        assert_eq!(
            decision.effects,
            vec![Effect::PersistRepository {
                repository: decision.state.repositories[0].clone(),
                next_repository_id: 2,
            }]
        );
    }

    #[test]
    fn attaching_a_workset_registers_detected_repositories_and_keeps_their_state() {
        let decision = decide(
            state_with_item(7, "Work"),
            Event::AttachWorkset {
                item_id: 1,
                root_directory: "/Users/me/worksets/PLAT-847".into(),
                repositories: vec![AttachedRepositoryInput {
                    name: "service-a".into(),
                    remote_url: "git@github.com:acme/service-a.git".into(),
                    current_branch: "feature/PLAT-847".into(),
                    is_dirty: true,
                }],
            },
        )
        .expect("an existing Workset should attach");

        assert_eq!(
            decision.state.repositories,
            vec![Repository {
                id: 1,
                project_id: 1,
                name: "service-a".into(),
                remote_url: "git@github.com:acme/service-a.git".into(),
            }]
        );
        assert_eq!(
            decision.state.worksets,
            vec![Workset {
                id: 1,
                item_id: 1,
                root_directory: "/Users/me/worksets/PLAT-847".into(),
                branch: "feature/PLAT-847".into(),
                archived: false,
                repositories: vec![WorksetRepository {
                    repository_id: 1,
                    branch_override: None,
                    base_branch_override: None,
                    current_branch: "feature/PLAT-847".into(),
                    is_dirty: true,
                }],
            }]
        );
        assert!(matches!(
            decision.effects.as_slice(),
            [
                Effect::PersistRepository {
                    next_repository_id: 2,
                    ..
                },
                Effect::PersistWorkset {
                    next_workset_id: 2,
                    ..
                }
            ]
        ));
    }

    #[test]
    fn attaching_a_workset_preserves_different_repository_branches_as_overrides() {
        let decision = decide(
            state_with_item(7, "Work"),
            Event::AttachWorkset {
                item_id: 1,
                root_directory: "/Users/me/worksets/PLAT-847".into(),
                repositories: vec![
                    AttachedRepositoryInput {
                        name: "service-a".into(),
                        remote_url: "/Users/me/service-a".into(),
                        current_branch: "main".into(),
                        is_dirty: false,
                    },
                    AttachedRepositoryInput {
                        name: "service-b".into(),
                        remote_url: "/Users/me/service-b".into(),
                        current_branch: "develop".into(),
                        is_dirty: true,
                    },
                ],
            },
        )
        .expect("an existing Workset should attach");

        assert_eq!(decision.state.worksets[0].branch, "main");
        assert_eq!(
            decision.state.worksets[0].repositories[1],
            WorksetRepository {
                repository_id: 2,
                branch_override: Some("develop".into()),
                base_branch_override: None,
                current_branch: "develop".into(),
                is_dirty: true,
            }
        );
    }

    #[test]
    fn creating_a_workset_selects_repositories_with_independent_branch_overrides() {
        let mut state = state_with_item(7, "Work");
        state.repositories = vec![
            Repository {
                id: 1,
                project_id: 1,
                name: "service-a".into(),
                remote_url: "https://example.com/service-a.git".into(),
            },
            Repository {
                id: 2,
                project_id: 1,
                name: "service-b".into(),
                remote_url: "https://example.com/service-b.git".into(),
            },
        ];
        state.next_repository_id = 3;

        let decision = decide(
            state,
            Event::CreateWorkset {
                item_id: 1,
                root_directory: "/tmp/worksets/PLAT-847".into(),
                branch: "feature/PLAT-847".into(),
                repositories: vec![
                    WorksetRepositoryInput {
                        repository_id: 1,
                        branch_override: None,
                        base_branch_override: Some("main".into()),
                    },
                    WorksetRepositoryInput {
                        repository_id: 2,
                        branch_override: Some("feature/PLAT-847-worker".into()),
                        base_branch_override: Some("develop".into()),
                    },
                ],
            },
        )
        .expect("Workset creation should succeed");

        assert_eq!(decision.state.worksets.len(), 1);
        assert_eq!(
            decision.state.worksets[0],
            Workset {
                id: 1,
                item_id: 1,
                root_directory: "/tmp/worksets/PLAT-847".into(),
                branch: "feature/PLAT-847".into(),
                archived: false,
                repositories: vec![
                    WorksetRepository {
                        repository_id: 1,
                        branch_override: None,
                        base_branch_override: Some("main".into()),
                        current_branch: "feature/PLAT-847".into(),
                        is_dirty: false,
                    },
                    WorksetRepository {
                        repository_id: 2,
                        branch_override: Some("feature/PLAT-847-worker".into()),
                        base_branch_override: Some("develop".into()),
                        current_branch: "feature/PLAT-847-worker".into(),
                        is_dirty: false,
                    },
                ],
            }
        );
        assert_eq!(decision.state.next_workset_id, 2);
        assert!(matches!(
            decision.effects.as_slice(),
            [Effect::PersistWorkset {
                next_workset_id: 2,
                ..
            }]
        ));
    }

    #[test]
    fn an_item_can_have_multiple_worksets_and_add_a_repository_later() {
        let mut state = state_with_item(7, "Work");
        state.repositories = vec![
            Repository {
                id: 1,
                project_id: 1,
                name: "service-a".into(),
                remote_url: "https://example.com/service-a.git".into(),
            },
            Repository {
                id: 2,
                project_id: 1,
                name: "service-b".into(),
                remote_url: "https://example.com/service-b.git".into(),
            },
        ];
        state.next_repository_id = 3;

        let first = decide(
            state,
            Event::CreateWorkset {
                item_id: 1,
                root_directory: "/tmp/worksets/first".into(),
                branch: "feature/first".into(),
                repositories: vec![WorksetRepositoryInput {
                    repository_id: 1,
                    branch_override: None,
                    base_branch_override: None,
                }],
            },
        )
        .expect("first Workset should succeed");
        let second = decide(
            first.state,
            Event::CreateWorkset {
                item_id: 1,
                root_directory: "/tmp/worksets/second".into(),
                branch: "feature/second".into(),
                repositories: vec![WorksetRepositoryInput {
                    repository_id: 2,
                    branch_override: None,
                    base_branch_override: None,
                }],
            },
        )
        .expect("the same Item should accept another Workset");
        let extended = decide(
            second.state,
            Event::AddRepositoryToWorkset {
                workset_id: 1,
                repository_id: 2,
                branch_override: Some("feature/first-worker".into()),
                base_branch_override: Some("develop".into()),
            },
        )
        .expect("a Repository should be addable to an existing Workset");

        assert_eq!(extended.state.worksets.len(), 2);
        assert_eq!(extended.state.worksets[0].repositories.len(), 2);
        assert_eq!(
            extended.state.worksets[0].repositories[1],
            WorksetRepository {
                repository_id: 2,
                branch_override: Some("feature/first-worker".into()),
                base_branch_override: Some("develop".into()),
                current_branch: "feature/first-worker".into(),
                is_dirty: false,
            }
        );
        assert!(matches!(
            extended.effects.as_slice(),
            [Effect::PersistWorksetUpdate { .. }]
        ));
    }

    #[test]
    fn archiving_hides_a_workset_without_removing_it_and_removal_is_separate() {
        let mut state = state_with_item(7, "Work");
        state.repositories.push(Repository {
            id: 1,
            project_id: 1,
            name: "service-a".into(),
            remote_url: "https://example.com/service-a.git".into(),
        });
        let created = decide(
            state,
            Event::CreateWorkset {
                item_id: 1,
                root_directory: "/tmp/worksets/archive-me".into(),
                branch: "feature/archive-me".into(),
                repositories: vec![WorksetRepositoryInput {
                    repository_id: 1,
                    branch_override: None,
                    base_branch_override: None,
                }],
            },
        )
        .expect("Workset should be created");

        let archived = decide(
            created.state,
            Event::SetWorksetArchived {
                workset_id: 1,
                archived: true,
            },
        )
        .expect("Workset should be archivable");

        assert!(archived.state.worksets[0].archived);
        assert!(item_views(&archived.state, None)[0].worksets.is_empty());
        assert_eq!(
            item_views(&archived.state, None)[0].archived_worksets.len(),
            1
        );
        assert!(matches!(
            archived.effects.as_slice(),
            [Effect::PersistWorksetUpdate { workset }] if workset.archived
        ));

        let removed = decide(archived.state, Event::RemoveWorkset { workset_id: 1 })
            .expect("Workset removal should be a separate decision");

        assert!(removed.state.worksets.is_empty());
        assert_eq!(
            removed.effects,
            vec![Effect::RemoveWorkset { workset_id: 1 }]
        );
    }

    #[test]
    fn worksets_reject_repositories_from_another_project() {
        let mut state = state_with_contexts(&[(7, "Work"), (8, "Personal")]);
        state.items.push(Item {
            id: 1,
            human_identifier: "MC-1".into(),
            title: "Work item".into(),
            project_id: 1,
            status: ItemStatus::Inbox,
            notes: String::new(),
            reminders: Vec::new(),
        });
        state.repositories.push(Repository {
            id: 1,
            project_id: 2,
            name: "personal-repo".into(),
            remote_url: "https://example.com/personal.git".into(),
        });

        assert_eq!(
            decide(
                state,
                Event::CreateWorkset {
                    item_id: 1,
                    root_directory: "/tmp/worksets/wrong".into(),
                    branch: "feature/wrong".into(),
                    repositories: vec![WorksetRepositoryInput {
                        repository_id: 1,
                        branch_override: None,
                        base_branch_override: None,
                    }],
                },
            ),
            Err(DomainError::RepositoryProjectMismatch {
                repository_id: 1,
                project_id: 1,
            })
        );
    }

    #[test]
    fn machines_keep_their_ssh_transport_and_last_observed_state() {
        let state = state_with_contexts(&[(1, "Work")]);
        let registered = decide(
            state,
            Event::RegisterMachine {
                context_id: 1,
                name: "Build host".into(),
                socket_name: "mission-manager".into(),
                transport: MachineTransport::Ssh {
                    host: "build.example.com".into(),
                    user: Some("runner".into()),
                    port: Some(2222),
                    identity_file: Some("/Users/me/.ssh/mission".into()),
                    known_hosts_file: Some("/Users/me/.ssh/known_hosts".into()),
                    strict_host_key_checking: Some("accept-new".into()),
                },
            },
        )
        .expect("a remote Machine should register");

        assert_eq!(
            registered.state.machines[0].transport,
            MachineTransport::Ssh {
                host: "build.example.com".into(),
                user: Some("runner".into()),
                port: Some(2222),
                identity_file: Some("/Users/me/.ssh/mission".into()),
                known_hosts_file: Some("/Users/me/.ssh/known_hosts".into()),
                strict_host_key_checking: Some("accept-new".into()),
            }
        );
        assert_eq!(
            registered.state.machines[0].last_observed,
            MachineObservation::Unknown
        );

        let observed = decide(
            registered.state,
            Event::ObserveMachine {
                machine_id: 1,
                observation: MachineObservation::Available,
                observed_at: 123,
            },
        )
        .expect("a Machine observation should be recorded");
        assert_eq!(
            observed.state.machines[0].last_observed,
            MachineObservation::Available
        );
        assert_eq!(observed.state.machines[0].last_observed_at, Some(123));
        assert!(matches!(
            observed.effects.as_slice(),
            [Effect::PersistMachineObservation { machine }]
                if machine.last_observed == MachineObservation::Available
        ));
    }

    #[test]
    fn runs_keep_history_allow_repeated_workset_use_and_scope_prompt_sources() {
        let mut state = state_with_item(1, "Work");
        state.items[0].notes = "Check the importer boundary".into();
        state.worksets.push(Workset {
            id: 1,
            item_id: 1,
            root_directory: "/tmp/workset".into(),
            branch: "feature/importer".into(),
            archived: false,
            repositories: Vec::new(),
        });
        state.machines.push(Machine {
            id: 1,
            context_id: 1,
            name: "Local Mac".into(),
            socket_name: "mission-manager".into(),
            transport: MachineTransport::Local,
            last_observed: MachineObservation::Unknown,
            last_observed_at: None,
        });
        state.external_objects.push(ExternalObject {
            id: 1,
            provider: ExternalProvider::Generic,
            kind: ExternalObjectKind::Generic,
            external_key: "source-1".into(),
            canonical_url: "https://example.com/source-1".into(),
        });
        state.links.push(Link {
            id: 1,
            item_id: 1,
            external_object_id: 1,
            reviewed_activity_id: 0,
            attention_policy: None,
            watch_until: None,
            review_at: None,
        });
        state.snapshots.push(ExternalSnapshot {
            external_object_id: 1,
            title: "Importer contract".into(),
            state: "OPEN".into(),
            metadata: Vec::new(),
            fetched_at: 1,
        });

        let selection = RunPromptSelection {
            include_objective: true,
            include_notes: false,
            external_object_ids: vec![1],
        };
        let prompt = compose_run_prompt(&state, 1, ExecutionProfile::Implement, &selection, None)
            .expect("the selected prompt sources should be valid");
        assert!(prompt.contains("Existing Item"));
        assert!(prompt.contains("Importer contract"));
        assert!(!prompt.contains("Check the importer boundary"));

        let first = decide(
            state,
            Event::StartRun {
                item_id: 1,
                workset_id: 1,
                machine_id: 1,
                agent: AgentKind::Claude,
                execution_profile: ExecutionProfile::Implement,
                prompt: prompt.clone(),
                working_directory: "/tmp/workset".into(),
                session_name: "mission-item-1-run-1".into(),
                pane_id: "%1".into(),
                started_at: 10,
                prompt_selection: selection.clone(),
            },
        )
        .expect("the first Run should start");
        let second = decide(
            first.state,
            Event::StartRun {
                item_id: 1,
                workset_id: 1,
                machine_id: 1,
                agent: AgentKind::Codex,
                execution_profile: ExecutionProfile::Review,
                prompt,
                working_directory: "/tmp/workset".into(),
                session_name: "mission-item-1-run-2".into(),
                pane_id: "%2".into(),
                started_at: 11,
                prompt_selection: selection,
            },
        )
        .expect("a second Run may use the same Workset");

        assert_eq!(second.state.runs.len(), 2);
        assert_eq!(second.state.runs[0].workset_id, 1);
        assert_eq!(second.state.runs[1].workset_id, 1);
        assert_eq!(second.state.runs[1].agent, AgentKind::Codex);
        assert!(matches!(
            second.effects.as_slice(),
            [Effect::PersistRun { run, next_run_id }] if run.id == 2 && *next_run_id == 3
        ));
    }

    #[test]
    fn an_untracked_agent_in_the_deepest_known_workset_is_suggested_once() {
        let mut state = state_with_item(1, "Work");
        state.worksets.extend([
            Workset {
                id: 1,
                item_id: 1,
                root_directory: "/tmp/worksets".into(),
                branch: "main".into(),
                archived: false,
                repositories: Vec::new(),
            },
            Workset {
                id: 2,
                item_id: 1,
                root_directory: "/tmp/worksets/platform".into(),
                branch: "feature/platform".into(),
                archived: false,
                repositories: Vec::new(),
            },
        ]);
        state.machines.push(Machine {
            id: 1,
            context_id: 1,
            name: "Local Mac".into(),
            socket_name: "mission-manager".into(),
            transport: MachineTransport::Local,
            last_observed: MachineObservation::Unknown,
            last_observed_at: None,
        });
        state.runs.push(Run {
            id: 1,
            item_id: 1,
            workset_id: 1,
            machine_id: 1,
            agent: AgentKind::Codex,
            execution_profile: ExecutionProfile::Implement,
            prompt: "Already attached".into(),
            working_directory: "/tmp/worksets".into(),
            session_name: "known-session".into(),
            pane_id: "%1".into(),
            started_at: 1,
            state: RunState::Unknown,
            pane_status: RunPaneStatus::Available,
        });

        let suggestions = suggest_untracked_runs(
            &state,
            &[
                AgentPaneObservation {
                    machine_id: 1,
                    agent: AgentKind::Claude,
                    session_name: "manual-session".into(),
                    pane_id: "%2".into(),
                    current_path: "/tmp/worksets/platform/service-a".into(),
                },
                AgentPaneObservation {
                    machine_id: 1,
                    agent: AgentKind::Codex,
                    session_name: "known-session".into(),
                    pane_id: "%1".into(),
                    current_path: "/tmp/worksets".into(),
                },
                AgentPaneObservation {
                    machine_id: 1,
                    agent: AgentKind::Claude,
                    session_name: "outside-session".into(),
                    pane_id: "%3".into(),
                    current_path: "/tmp/elsewhere".into(),
                },
            ],
        );

        assert_eq!(suggestions.len(), 1);
        assert_eq!(suggestions[0].workset_id, 2);
        assert_eq!(suggestions[0].item_identifier, "MC-1");
        assert_eq!(suggestions[0].context_name, "Work");
        assert_eq!(suggestions[0].agent, AgentKind::Claude);
    }

    #[test]
    fn attaching_a_suggestion_creates_an_unknown_run_without_starting_an_agent() {
        let mut state = state_with_item(1, "Work");
        state.worksets.push(Workset {
            id: 1,
            item_id: 1,
            root_directory: "/tmp/workset".into(),
            branch: "main".into(),
            archived: false,
            repositories: Vec::new(),
        });
        state.machines.push(Machine {
            id: 1,
            context_id: 1,
            name: "Local Mac".into(),
            socket_name: "mission-manager".into(),
            transport: MachineTransport::Local,
            last_observed: MachineObservation::Unknown,
            last_observed_at: None,
        });

        let attached = decide(
            state,
            Event::AttachRun {
                item_id: 1,
                workset_id: 1,
                machine_id: 1,
                agent: AgentKind::Claude,
                working_directory: "/tmp/workset".into(),
                session_name: "manual-session".into(),
                pane_id: "%2".into(),
                attached_at: 123,
            },
        )
        .expect("the explicit attachment should succeed");

        assert_eq!(attached.state.runs.len(), 1);
        assert_eq!(attached.state.runs[0].state, RunState::Unknown);
        assert_eq!(attached.state.runs[0].pane_status, RunPaneStatus::Available);
        assert_eq!(
            attached.state.runs[0].execution_profile,
            ExecutionProfile::CustomPrompt
        );
        assert_eq!(attached.state.runs[0].prompt, "Attached existing agent");
        assert!(matches!(
            attached.effects.as_slice(),
            [Effect::PersistRun { run, next_run_id }]
                if run.id == 1 && *next_run_id == 2
        ));

        let duplicate = decide(
            attached.state,
            Event::AttachRun {
                item_id: 1,
                workset_id: 1,
                machine_id: 1,
                agent: AgentKind::Claude,
                working_directory: "/tmp/workset".into(),
                session_name: "manual-session".into(),
                pane_id: "%2".into(),
                attached_at: 124,
            },
        )
        .expect_err("the same Pane must not be attached twice");
        assert_eq!(
            duplicate,
            DomainError::RunAlreadyAttached {
                machine_id: 1,
                session_name: "manual-session".into(),
                pane_id: "%2".into(),
            }
        );
    }

    #[test]
    fn a_run_state_is_unknown_until_reported_and_blocked_runs_need_attention() {
        let mut state = state_with_item(1, "Work");
        state.worksets.push(Workset {
            id: 1,
            item_id: 1,
            root_directory: "/tmp/workset".into(),
            branch: "main".into(),
            archived: false,
            repositories: Vec::new(),
        });
        state.machines.push(Machine {
            id: 1,
            context_id: 1,
            name: "Local Mac".into(),
            socket_name: "mission-manager".into(),
            transport: MachineTransport::Local,
            last_observed: MachineObservation::Unknown,
            last_observed_at: None,
        });

        let started = decide(
            state,
            Event::StartRun {
                item_id: 1,
                workset_id: 1,
                machine_id: 1,
                agent: AgentKind::Claude,
                execution_profile: ExecutionProfile::Implement,
                prompt: "Do the work".into(),
                working_directory: "/tmp/workset".into(),
                session_name: "mission-item-1-run-1".into(),
                pane_id: "%1".into(),
                started_at: 10,
                prompt_selection: RunPromptSelection {
                    include_objective: true,
                    include_notes: false,
                    external_object_ids: Vec::new(),
                },
            },
        )
        .expect("Run should start without guessing its state");

        assert_eq!(started.state.runs[0].state, RunState::Unknown);
        assert!(home_view(&started.state, None, "now")
            .attention_entries
            .is_empty());

        let blocked = decide(
            started.state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Blocked,
            },
        )
        .expect("a hook report should update the Run");
        let blocked_home = home_view(&blocked.state, None, "now");
        assert_eq!(blocked_home.attention_entries.len(), 1);
        assert_eq!(
            blocked_home.attention_entries[0].kind,
            AttentionEntryKind::BlockedRun
        );
        assert_eq!(blocked_home.attention_entries[0].run_id, Some(1));
        assert_eq!(blocked_home.needs_attention[0].item.id, 1);
        assert_eq!(blocked.state.items[0].status, ItemStatus::Inbox);

        let finished = decide(
            blocked.state,
            Event::UpdateRunState {
                run_id: 1,
                state: RunState::Finished,
            },
        )
        .expect("a finished hook report should update the Run");
        assert!(home_view(&finished.state, None, "now")
            .attention_entries
            .is_empty());
        assert_eq!(finished.state.items[0].status, ItemStatus::Inbox);
    }

    #[test]
    fn a_run_pane_can_be_marked_missing_without_changing_agent_state() {
        let mut state = state_with_item(1, "Work");
        state.worksets.push(Workset {
            id: 1,
            item_id: 1,
            root_directory: "/tmp/workset".into(),
            branch: "main".into(),
            archived: false,
            repositories: Vec::new(),
        });
        state.machines.push(Machine {
            id: 1,
            context_id: 1,
            name: "Local Mac".into(),
            socket_name: "mission-manager".into(),
            transport: MachineTransport::Local,
            last_observed: MachineObservation::Unknown,
            last_observed_at: None,
        });
        let started = decide(
            state,
            Event::StartRun {
                item_id: 1,
                workset_id: 1,
                machine_id: 1,
                agent: AgentKind::Claude,
                execution_profile: ExecutionProfile::Implement,
                prompt: "Do the work".into(),
                working_directory: "/tmp/workset".into(),
                session_name: "mission-item-1-run-1".into(),
                pane_id: "%1".into(),
                started_at: 10,
                prompt_selection: RunPromptSelection {
                    include_objective: true,
                    include_notes: false,
                    external_object_ids: Vec::new(),
                },
            },
        )
        .expect("Run should start");

        assert_eq!(started.state.runs[0].pane_status, RunPaneStatus::Available);

        let reconciled = decide(
            started.state,
            Event::SetRunPaneStatus {
                run_id: 1,
                status: RunPaneStatus::Missing,
            },
        )
        .expect("Pane reconciliation should update the Run");

        assert_eq!(reconciled.state.runs[0].pane_status, RunPaneStatus::Missing);
        assert_eq!(reconciled.state.runs[0].state, RunState::Unknown);
        assert!(matches!(
            reconciled.effects.as_slice(),
            [Effect::PersistRunPaneStatus { run }]
                if run.id == 1 && run.pane_status == RunPaneStatus::Missing
        ));
    }

    #[test]
    fn a_run_cannot_use_a_machine_or_prompt_source_from_another_context() {
        let mut state = state_with_contexts(&[(1, "Work"), (2, "Personal")]);
        state.items.push(Item {
            id: 1,
            human_identifier: "MC-1".into(),
            title: "Work item".into(),
            project_id: 1,
            status: ItemStatus::Inbox,
            notes: String::new(),
            reminders: Vec::new(),
        });
        state.worksets.push(Workset {
            id: 1,
            item_id: 1,
            root_directory: "/tmp/workset".into(),
            branch: "main".into(),
            archived: false,
            repositories: Vec::new(),
        });
        state.machines.push(Machine {
            id: 1,
            context_id: 2,
            name: "Personal Mac".into(),
            socket_name: "personal".into(),
            transport: MachineTransport::Local,
            last_observed: MachineObservation::Unknown,
            last_observed_at: None,
        });

        let error = decide(
            state,
            Event::StartRun {
                item_id: 1,
                workset_id: 1,
                machine_id: 1,
                agent: AgentKind::Claude,
                execution_profile: ExecutionProfile::Investigate,
                prompt: "Inspect the work".into(),
                working_directory: "/tmp/workset".into(),
                session_name: "mission-item-1-run-1".into(),
                pane_id: "%1".into(),
                started_at: 10,
                prompt_selection: RunPromptSelection {
                    include_objective: true,
                    include_notes: false,
                    external_object_ids: Vec::new(),
                },
            },
        )
        .expect_err("a Run must not cross Context boundaries");
        assert_eq!(
            error,
            DomainError::MachineContextMismatch {
                machine_id: 1,
                context_id: 1,
            }
        );
    }

    fn snapshot_data(title: &str, fetched_at: i64) -> ExternalSnapshotData {
        ExternalSnapshotData {
            title: title.into(),
            state: "OPEN".into(),
            metadata: Vec::new(),
            fetched_at,
        }
    }

    fn state_with_context(id: i64, name: &str) -> DomainState {
        state_with_contexts(&[(id, name)])
    }

    fn state_with_item(id: i64, name: &str) -> DomainState {
        let mut state = state_with_context(id, name);
        state.items.push(Item {
            id: 1,
            human_identifier: "MC-1".into(),
            title: "Existing Item".into(),
            project_id: 1,
            status: ItemStatus::Inbox,
            notes: String::new(),
            reminders: Vec::new(),
        });
        state.next_item_id = 2;
        state.next_item_number = 2;
        state
    }

    fn state_with_contexts(contexts: &[(i64, &str)]) -> DomainState {
        DomainState {
            next_context_id: contexts.iter().map(|(id, _)| *id).max().unwrap_or(0) + 1,
            next_project_id: contexts.len() as i64 + 1,
            next_item_id: 1,
            next_item_number: 1,
            next_repository_id: 1,
            next_workset_id: 1,
            next_machine_id: 1,
            next_run_id: 1,
            next_external_object_id: 1,
            next_link_id: 1,
            next_activity_id: 1,
            next_reminder_id: 1,
            contexts: contexts
                .iter()
                .map(|(id, name)| Context {
                    id: *id,
                    name: (*name).into(),
                })
                .collect(),
            projects: contexts
                .iter()
                .enumerate()
                .map(|(index, (context_id, _))| Project {
                    id: index as i64 + 1,
                    context_id: *context_id,
                    name: "Default".into(),
                    defaults: ProjectDefaults {
                        item_status: ItemStatus::Inbox,
                    },
                })
                .collect(),
            repositories: Vec::new(),
            items: Vec::new(),
            worksets: Vec::new(),
            machines: Vec::new(),
            runs: Vec::new(),
            relationships: Vec::new(),
            external_objects: Vec::new(),
            links: Vec::new(),
            snapshots: Vec::new(),
            activities: Vec::new(),
            attention_defaults: Vec::new(),
        }
    }

    fn empty_state() -> DomainState {
        DomainState {
            next_context_id: 1,
            next_project_id: 1,
            next_item_id: 1,
            next_item_number: 1,
            next_repository_id: 1,
            next_workset_id: 1,
            next_machine_id: 1,
            next_run_id: 1,
            next_external_object_id: 1,
            next_link_id: 1,
            next_activity_id: 1,
            next_reminder_id: 1,
            contexts: Vec::new(),
            projects: Vec::new(),
            repositories: Vec::new(),
            items: Vec::new(),
            worksets: Vec::new(),
            machines: Vec::new(),
            runs: Vec::new(),
            relationships: Vec::new(),
            external_objects: Vec::new(),
            links: Vec::new(),
            snapshots: Vec::new(),
            activities: Vec::new(),
            attention_defaults: Vec::new(),
        }
    }
}
