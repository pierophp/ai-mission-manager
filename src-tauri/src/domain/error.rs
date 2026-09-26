use thiserror::Error;

use super::*;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DomainError {
    #[error("Item {item_id} already has an active Implementation Queue")]
    ImplementationQueueAlreadyActive { item_id: i64 },
    #[error("Implementation Queue {queue_id} does not exist")]
    ImplementationQueueNotFound { queue_id: i64 },
    #[error("Run {run_id} is not an entry in Implementation Queue {queue_id}")]
    ImplementationQueueRunNotFound { queue_id: i64, run_id: i64 },
    #[error("Run {run_id} has not finished and cannot advance Implementation Queue {queue_id}")]
    ImplementationQueueRunNotFinished { queue_id: i64, run_id: i64 },
    #[error("Entry {position} does not exist in Implementation Queue {queue_id}")]
    ImplementationQueueEntryNotFound { queue_id: i64, position: i64 },
    #[error("Entry {position} in Implementation Queue {queue_id} is already done")]
    ImplementationQueueEntryAlreadyDone { queue_id: i64, position: i64 },
    #[error("Implementation Queue {queue_id} is not paused")]
    ImplementationQueueNotPaused { queue_id: i64 },
    #[error("Implementation Queue requires one or more open tickets")]
    ImplementationQueueContainsClosedTicket,
    #[error("Implementation Queue configuration is missing")]
    ImplementationQueueConfigurationMissing,
    #[error("Implementation spec does not exist or is not a GitHub Issue")]
    ImplementationSpecNotFound,
    #[error("Implementation spec is not linked to the Item")]
    ImplementationSpecNotLinked,
    #[error("Implementation spec URL does not match the linked spec")]
    ImplementationSpecUrlMismatch,
    #[error("Implementation Queue entries must be ordered, unique, and open")]
    InvalidImplementationQueueEntries,
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
    #[error("Repository {repository_id} is used by Run #{run_id} and cannot be removed")]
    RepositoryInUseByRun { repository_id: i64, run_id: i64 },
    #[error("Repository {repository_id} has already been configured on Machine {machine_id}")]
    RepositoryLocationAlreadyExists { repository_id: i64, machine_id: i64 },
    #[error("Repository {repository_id} has no location on Machine {machine_id}")]
    RepositoryLocationNotFound { repository_id: i64, machine_id: i64 },
    #[error(
        "Repository {repository_id} deletion must include its Item execution references {expected_workspace_ids:?}; received {provided_workspace_ids:?}"
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
    #[error(
        "Machine {machine_id} deletion must include Worktrees {expected_worktree_ids:?}; received {provided_worktree_ids:?}"
    )]
    MachineWorktreesMismatch {
        machine_id: i64,
        expected_worktree_ids: Vec<i64>,
        provided_worktree_ids: Vec<i64>,
    },
    #[error(
        "Machine {machine_id} deletion must include Repository locations {expected_repository_ids:?}; received {provided_repository_ids:?}"
    )]
    MachineRepositoryLocationsMismatch {
        machine_id: i64,
        expected_repository_ids: Vec<i64>,
        provided_repository_ids: Vec<i64>,
    },
    #[error("a branch cannot be blank")]
    EmptyBranch,
    #[error("Project Repository execution setup must include at least one Repository")]
    EmptyWorkspaceRepositories,
    #[error("Project Repository execution setup {workspace_id} does not exist")]
    WorkspaceNotFound { workspace_id: i64 },
    #[error("Item execution setup {workspace_id} does not belong to Item {item_id}")]
    WorkspaceItemMismatch { workspace_id: i64, item_id: i64 },
    #[error("Worktree {worktree_id} does not belong to this Item's execution setup")]
    RunWorktreeWorkspaceMismatch { worktree_id: i64, workspace_id: i64 },
    #[error("Run working directory does not match Repository {repository_id}")]
    RunWorkingDirectoryRepositoryMismatch { repository_id: i64 },
    #[error(
        "Repository {repository_id} is already configured for Item execution setup {workspace_id}"
    )]
    RepositoryAlreadyInWorkspace {
        repository_id: i64,
        workspace_id: i64,
    },
    #[error("Worktree {worktree_id} does not exist")]
    WorktreeNotFound { worktree_id: i64 },
    #[error("Repository {repository_id} already has a registered Worktree for this Item")]
    WorktreeAlreadyExists {
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
    #[error("Context {context_id} has no execution Machine configured")]
    ContextHasNoExecutionMachine { context_id: i64 },
    #[error(
        "Machine {machine_id} is not the execution Machine configured for Context {context_id}"
    )]
    ContextExecutionMachineMismatch { context_id: i64, machine_id: i64 },
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
    #[error("Grill configuration is invalid for {agent:?}: model {model}, effort {effort}")]
    InvalidGrillConfiguration {
        agent: AgentKind,
        model: String,
        effort: String,
    },
    #[error("Item {item_id} already has an active Grill Run {run_id}")]
    ActiveGrillRun { item_id: i64, run_id: i64 },
    #[error("a Grill skill snapshot cannot be blank")]
    EmptyGrillSkillSnapshot,
    #[error("a Grill answer cannot be blank")]
    EmptyGrillAnswer,
    #[error("a Grill question cannot have more than one answer")]
    DuplicateGrillAnswer,
    #[error("Run {run_id} has no parsed Grill question group")]
    GrillQuestionGroupNotFound { run_id: i64 },
    #[error("Grill answer is for unknown question {question_number}")]
    UnknownGrillQuestion { question_number: u32 },
    #[error("Run {run_id} already has a submitted Grill response")]
    GrillResponseAlreadySubmitted { run_id: i64 },
    #[error("Run {run_id} is not a Grill Run")]
    NotGrillRun { run_id: i64 },
    #[error("a Grill response cannot be blank")]
    EmptyGrillResponse,
    #[error("Run {run_id} does not exist")]
    RunNotFound { run_id: i64 },
    #[error("Run {run_id} cannot continue Grill from phase {phase:?}")]
    GrillContinuationNotAvailable {
        run_id: i64,
        phase: Option<GrillPhase>,
    },
    #[error("Run {run_id} cannot capture downstream Issues for action {action:?}")]
    DownstreamCaptureNotAvailable {
        run_id: i64,
        action: GrillContinuationAction,
    },
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
    #[error("Only a GitHub Issue can have the Spec Link purpose")]
    LinkCannotBeSpec,
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
