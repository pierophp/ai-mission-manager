use super::*;

pub enum Event {
    CreateContext {
        name: String,
    },
    SetContextGrillDefaults {
        context_id: i64,
        defaults: GrillConfiguration,
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
    StartGrillRun {
        item_id: i64,
        workspace_id: i64,
        repository_id: i64,
        machine_id: i64,
        configuration: GrillConfiguration,
        prompt: String,
        skill_snapshot: String,
        working_directory: String,
        session_name: String,
        pane_id: String,
        started_at: i64,
        checkouts: Vec<RunCheckout>,
    },
    AttachRun {
        item_id: i64,
        workspace_id: i64,
        worktree_id: Option<i64>,
        repository_id: i64,
        machine_id: i64,
        agent: AgentKind,
        working_directory: String,
        machine_home: String,
        session_name: String,
        pane_id: String,
        attached_at: i64,
    },
    UpdateRunState {
        run_id: i64,
        state: RunState,
    },
    FinishRun {
        run_id: i64,
    },
    ContinueGrill {
        run_id: i64,
        action: GrillContinuationAction,
    },
    CaptureDownstreamIssues {
        run_id: i64,
        action: GrillContinuationAction,
        issues: Vec<ConfirmedDownstreamIssue>,
    },
    RecordRunTranscript {
        run_id: i64,
        transcript: String,
        question_group: Option<GrillQuestionGroup>,
    },
    RecordGrillAnswers {
        run_id: i64,
        answers: Vec<GrillAnswer>,
    },
    RecordGrillResponse {
        run_id: i64,
        response: String,
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
    PersistContextGrillDefaults {
        context: Context,
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
    PersistRunTranscript {
        run: Run,
    },
    PersistGrillAnswers {
        run: Run,
    },
    PersistGrillResponse {
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
