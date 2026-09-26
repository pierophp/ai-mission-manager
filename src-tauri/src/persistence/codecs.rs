use super::*;

pub(super) fn bool_as_i64(value: bool) -> i64 {
    i64::from(value)
}

pub(super) fn agent_kind_as_str(agent: AgentKind) -> &'static str {
    match agent {
        AgentKind::Claude => "claude",
        AgentKind::Codex => "codex",
    }
}

pub(super) fn parse_agent_kind(agent: &str) -> Result<AgentKind, StoreError> {
    match agent {
        "claude" => Ok(AgentKind::Claude),
        "codex" => Ok(AgentKind::Codex),
        other => Err(StoreError::InvalidAgentKind(other.into())),
    }
}

pub(super) fn execution_profile_as_str(profile: ExecutionProfile) -> &'static str {
    match profile {
        ExecutionProfile::Investigate => "investigate",
        ExecutionProfile::Implement => "implement",
        ExecutionProfile::Review => "review",
        ExecutionProfile::CustomPrompt => "custom",
        ExecutionProfile::Grill => "grill",
    }
}

pub(super) fn parse_execution_profile(profile: &str) -> Result<ExecutionProfile, StoreError> {
    match profile {
        "investigate" => Ok(ExecutionProfile::Investigate),
        "implement" => Ok(ExecutionProfile::Implement),
        "review" => Ok(ExecutionProfile::Review),
        "custom" => Ok(ExecutionProfile::CustomPrompt),
        "grill" => Ok(ExecutionProfile::Grill),
        other => Err(StoreError::InvalidExecutionProfile(other.into())),
    }
}

pub(super) fn execution_mode_as_str(mode: ExecutionMode) -> &'static str {
    match mode {
        ExecutionMode::Direct => "direct",
        ExecutionMode::Worktree => "worktree",
    }
}

pub(super) fn parse_execution_mode(mode: &str) -> Result<ExecutionMode, StoreError> {
    match mode {
        "direct" => Ok(ExecutionMode::Direct),
        "worktree" => Ok(ExecutionMode::Worktree),
        other => Err(StoreError::InvalidExecutionMode(other.into())),
    }
}

pub(super) fn workspace_preparation_state_as_str(state: WorkspacePreparationState) -> &'static str {
    match state {
        WorkspacePreparationState::Pending => "pending",
        WorkspacePreparationState::Resumable => "resumable",
        WorkspacePreparationState::Ready => "ready",
    }
}

pub(super) fn parse_workspace_preparation_state(
    state: &str,
) -> Result<WorkspacePreparationState, StoreError> {
    match state {
        "pending" => Ok(WorkspacePreparationState::Pending),
        "resumable" => Ok(WorkspacePreparationState::Resumable),
        "ready" => Ok(WorkspacePreparationState::Ready),
        other => Err(StoreError::InvalidWorkspacePreparationState(other.into())),
    }
}

pub(super) fn run_state_as_str(state: RunState) -> &'static str {
    match state {
        RunState::Unknown => "unknown",
        RunState::Working => "working",
        RunState::Blocked => "blocked",
        RunState::Finished => "finished",
    }
}

pub(super) fn parse_run_state(state: &str) -> Result<RunState, StoreError> {
    match state {
        "unknown" => Ok(RunState::Unknown),
        "working" => Ok(RunState::Working),
        "blocked" => Ok(RunState::Blocked),
        "finished" => Ok(RunState::Finished),
        other => Err(StoreError::InvalidRunState(other.into())),
    }
}

pub(super) fn run_pane_status_as_str(status: RunPaneStatus) -> &'static str {
    match status {
        RunPaneStatus::Unknown => "unknown",
        RunPaneStatus::Available => "available",
        RunPaneStatus::Missing => "missing",
    }
}

pub(super) fn parse_run_pane_status(status: &str) -> Result<RunPaneStatus, StoreError> {
    match status {
        "unknown" => Ok(RunPaneStatus::Unknown),
        "available" => Ok(RunPaneStatus::Available),
        "missing" => Ok(RunPaneStatus::Missing),
        other => Err(StoreError::InvalidRunPaneStatus(other.into())),
    }
}

pub(super) fn grill_phase_as_str(phase: GrillPhase) -> &'static str {
    match phase {
        GrillPhase::Starting => "starting",
        GrillPhase::Working => "working",
        GrillPhase::WaitingForAnswers => "waiting_for_answers",
        GrillPhase::AwaitingNextAction => "awaiting_next_action",
        GrillPhase::RecoverablePaneLoss => "recoverable_pane_loss",
        GrillPhase::Finished => "finished",
    }
}

pub(super) fn parse_grill_phase(phase: &str) -> Result<GrillPhase, StoreError> {
    match phase {
        "starting" => Ok(GrillPhase::Starting),
        "working" => Ok(GrillPhase::Working),
        "waiting_for_answers" => Ok(GrillPhase::WaitingForAnswers),
        "awaiting_next_action" => Ok(GrillPhase::AwaitingNextAction),
        "recoverable_pane_loss" => Ok(GrillPhase::RecoverablePaneLoss),
        "finished" => Ok(GrillPhase::Finished),
        other => Err(StoreError::InvalidGrillPhase(other.into())),
    }
}

pub(super) fn grill_continuation_action_as_str(action: GrillContinuationAction) -> &'static str {
    action.as_str()
}

pub(super) fn parse_grill_continuation_action(
    action: &str,
) -> Result<GrillContinuationAction, StoreError> {
    match action {
        "to-spec" => Ok(GrillContinuationAction::ToSpec),
        "to-tickets" => Ok(GrillContinuationAction::ToTickets),
        "implement" => Ok(GrillContinuationAction::Implement),
        other => Err(StoreError::InvalidDownstreamAction(other.into())),
    }
}

pub(super) fn machine_observation_as_str(observation: MachineObservation) -> &'static str {
    match observation {
        MachineObservation::Unknown => "unknown",
        MachineObservation::Available => "available",
        MachineObservation::Offline => "offline",
    }
}

pub(super) fn parse_machine_observation(
    observation: &str,
) -> Result<MachineObservation, StoreError> {
    match observation {
        "unknown" => Ok(MachineObservation::Unknown),
        "available" => Ok(MachineObservation::Available),
        "offline" => Ok(MachineObservation::Offline),
        other => Err(StoreError::InvalidMachineObservation(other.into())),
    }
}

pub(super) fn item_status_as_str(status: ItemStatus) -> &'static str {
    match status {
        ItemStatus::Inbox => "Inbox",
        ItemStatus::Active => "Active",
        ItemStatus::Waiting => "Waiting",
        ItemStatus::Done => "Done",
    }
}

pub(super) fn parse_item_status(status: &str) -> Result<ItemStatus, StoreError> {
    parse_status(status).map_err(|other| StoreError::InvalidItemStatus(other.into()))
}

pub(super) fn parse_project_default_status(status: &str) -> Result<ItemStatus, StoreError> {
    parse_status(status).map_err(|other| StoreError::InvalidProjectDefaultStatus(other.into()))
}

pub(super) fn item_relation_kind_as_str(kind: ItemRelationKind) -> &'static str {
    match kind {
        ItemRelationKind::Blocks => "blocks",
        ItemRelationKind::BlockedBy => "blocked_by",
        ItemRelationKind::RelatedTo => "related_to",
    }
}

pub(super) fn parse_item_relation_kind(kind: &str) -> Result<ItemRelationKind, StoreError> {
    match kind {
        "blocks" => Ok(ItemRelationKind::Blocks),
        "blocked_by" => Ok(ItemRelationKind::BlockedBy),
        "related_to" => Ok(ItemRelationKind::RelatedTo),
        other => Err(StoreError::InvalidItemRelationKind(other.into())),
    }
}

pub(super) fn external_provider_as_str(provider: ExternalProvider) -> &'static str {
    match provider {
        ExternalProvider::GitHub => "github",
        ExternalProvider::Generic => "generic",
    }
}

pub(super) fn parse_external_provider(provider: &str) -> Result<ExternalProvider, StoreError> {
    match provider {
        "github" => Ok(ExternalProvider::GitHub),
        "generic" => Ok(ExternalProvider::Generic),
        other => Err(StoreError::InvalidExternalProvider(other.into())),
    }
}

pub(super) fn external_object_kind_as_str(kind: ExternalObjectKind) -> &'static str {
    match kind {
        ExternalObjectKind::Issue => "issue",
        ExternalObjectKind::PullRequest => "pull_request",
        ExternalObjectKind::Generic => "generic",
    }
}

pub(super) fn link_purpose_as_str(purpose: LinkPurpose) -> &'static str {
    match purpose {
        LinkPurpose::ToSpec => "to-spec",
        LinkPurpose::ToTickets => "to-tickets",
        LinkPurpose::Others => "others",
    }
}

pub(super) fn parse_link_purpose(purpose: &str) -> Result<LinkPurpose, StoreError> {
    match purpose {
        "to-spec" => Ok(LinkPurpose::ToSpec),
        "to-tickets" => Ok(LinkPurpose::ToTickets),
        "others" => Ok(LinkPurpose::Others),
        other => Err(StoreError::InvalidLinkPurpose(other.into())),
    }
}

pub(super) fn parse_external_object_kind(kind: &str) -> Result<ExternalObjectKind, StoreError> {
    match kind {
        "issue" => Ok(ExternalObjectKind::Issue),
        "pull_request" => Ok(ExternalObjectKind::PullRequest),
        "generic" => Ok(ExternalObjectKind::Generic),
        other => Err(StoreError::InvalidExternalObjectKind(other.into())),
    }
}

pub(super) fn parse_status(status: &str) -> Result<ItemStatus, &str> {
    match status {
        "Inbox" => Ok(ItemStatus::Inbox),
        "Active" => Ok(ItemStatus::Active),
        "Waiting" => Ok(ItemStatus::Waiting),
        "Done" => Ok(ItemStatus::Done),
        other => Err(other),
    }
}
