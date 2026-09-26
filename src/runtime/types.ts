import type {
  AgentKind,
  GrillContinuationAction,
  Run,
  RunPaneStatus,
  RunState,
} from "./execution-types";
import type { RunCheckout } from "./execution-types";

export type Context = {
  id: number;
  name: string;
  execution_machine_id: number | null;
  grill_defaults: GrillConfiguration;
  implement_defaults: GrillConfiguration;
};

export type GrillConfiguration = {
  agent: AgentKind;
  model: string;
  effort: string;
};

export type GrillEffort = {
  id: string;
  label: string;
};

export type GrillModel = {
  id: string;
  label: string;
  efforts: GrillEffort[];
};

export type GrillAgentCatalog = {
  agent: AgentKind;
  models: GrillModel[];
};

export type ProviderChoice = "github" | "none";
export type DependencyState =
  | "available"
  | "missing"
  | "unauthenticated"
  | "notConfigured"
  | "unavailable";

export type SetupState = {
  completed: boolean;
  provider: ProviderChoice;
};

export type DependencyStatus = {
  key: string;
  label: string;
  state: DependencyState;
  executablePath: string | null;
  message: string;
  action: string | null;
};

export type HealthStatus = {
  runtime: DependencyStatus;
  provider: DependencyStatus;
  agents: DependencyStatus[];
  checkedAt: number;
};

export type ItemStatus = "Inbox" | "Active" | "Waiting" | "Done";
export type ExecutionMode = "direct" | "worktree";

export type Project = {
  id: number;
  context_id: number;
  name: string;
  defaults: {
    item_status: ItemStatus;
    execution_mode: ExecutionMode;
  };
};

export type Repository = {
  id: number;
  project_id: number;
  name: string;
  remote_url: string;
  base_branch: string;
};

export type RepositoryLocation = {
  repository_id: number;
  machine_id: number;
  checkout_path: string;
  worktree_root: string;
};

export type WorkspaceRepository = {
  repository_id: number;
  branch: string;
  base_branch: string;
};

export type WorkspaceRepositoryInput = {
  repositoryId: number;
  branch: string;
  baseBranch: string;
};

export type Workspace = {
  id: number;
  item_id: number;
  repositories: WorkspaceRepository[];
  preparation_state: "pending" | "resumable" | "ready";
};

export type DirectRunCheckoutPreview = {
  repositoryId: number;
  repositoryName: string;
  path: string;
  branch: string;
  isDirty: boolean;
};

export type DirectRunSharedRun = {
  runId: number;
  itemId: number;
  path: string;
};

export type DirectRunPreview = {
  workspaceId: number;
  machineId: number;
  machineName: string;
  workingDirectory: string;
  checkouts: RunCheckout[];
  checkoutDetails: DirectRunCheckoutPreview[];
  currentBranches: string[];
  dirtyRepositoryIds: number[];
  sharedRuns: DirectRunSharedRun[];
  sharedPaths: string[];
};

export type Worktree = {
  id: number;
  workspace_id: number;
  repository_id: number;
  machine_id: number;
  path: string;
  branch: string;
  base_branch: string;
  is_dirty: boolean;
};

export type WorktreeRemovalReport = {
  worktreeId: number;
  workspaceId: number;
  repositoryId: number;
  repositoryName: string;
  machineId: number;
  path: string;
  branch: string;
  isDirty: boolean;
  requiresDestructiveConfirmation: boolean;
};

export type MachineTransport =
  | { kind: "local" }
  | {
      kind: "ssh";
      host: string;
      user: string | null;
      port: number | null;
      identityFile: string | null;
      knownHostsFile: string | null;
      strictHostKeyChecking: string | null;
    };

export type Machine = {
  id: number;
  context_id: number;
  name: string;
  socket_name: string;
  transport: MachineTransport;
  last_observed: "unknown" | "available" | "offline";
  last_observed_at: number | null;
  readiness?: MachineReadiness | null;
};

export type AgentHookReadiness = {
  provisioned: boolean | null;
  current: boolean | null;
  error: string | null;
};

export type MachineReadiness = {
  reachable: boolean | null;
  tmuxAvailable: boolean | null;
  claudeExecutableResolved: boolean | null;
  codexExecutableResolved: boolean | null;
  stateDirectoryWritable: boolean | null;
  claudeHooks: AgentHookReadiness;
  codexHooks: AgentHookReadiness;
  lastProvisioningError: string | null;
  error: string | null;
};

export type Item = {
  id: number;
  human_identifier: string;
  title: string;
  project_id: number;
  status: ItemStatus;
  notes: string;
  reminders: { id: number; remind_at: string }[];
};

export type ItemRelationKind = "Blocks" | "BlockedBy" | "RelatedTo";

export type ItemRelation = {
  from_item_id: number;
  to_item_id: number;
  kind: ItemRelationKind;
};

export type ExternalObject = {
  id: number;
  provider: "github" | "generic";
  kind: "issue" | "pull_request" | "generic";
  external_key: string;
  canonical_url: string;
};

export type ExternalObjectKind = ExternalObject["kind"];

export type ExternalMetadata = {
  key: string;
  value: string;
};

export type ExternalSnapshot = {
  external_object_id: number;
  title: string;
  state: string;
  metadata: ExternalMetadata[];
  fetched_at: number;
};

/** A GitHub Issue read as a document, fresh from GitHub. */
export type IssueDocument = {
  body: string;
  subIssues: SubIssue[];
};

export type SubIssue = {
  number: number;
  title: string;
  state: string;
  url: string;
};

export type ImplementationQueueStart = {
  specExternalObjectId: number;
  specUrl: string;
  entries: { position: number; ticketNumber: number; ticketTitle: string; ticketUrl: string; ticketState: string; runId: number | null; done: boolean; skipped?: boolean }[];
};

export type ImplementationQueuePauseReason =
  | { kind: "ticket_still_open" }
  | { kind: "checkout_dirty" }
  | { kind: "run_stopped" }
  | { kind: "pane_missing" }
  | { kind: "launch_failed"; message: string };

export type ImplementationQueue = {
  id: number;
  itemId: number;
  specExternalObjectId: number;
  specUrl: string;
  workspaceId: number;
  repositoryId: number;
  configuration: GrillConfiguration;
  allowDirty: boolean;
  allowSharedCheckouts: boolean;
  entries: ImplementationQueueStart["entries"];
  active: boolean;
  pausedReason: ImplementationQueuePauseReason | null;
};

export type ExternalChangeKind = "title" | "state" | "metadata";

export type ExternalChange = {
  kind: ExternalChangeKind;
  key: string | null;
  previous: string | null;
  current: string | null;
};

export type Activity = {
  id: number;
  external_object_id: number;
  observed_at: number;
  changes: ExternalChange[];
};

export type ObservedActivity = {
  activity: Activity;
  object: ExternalObject;
};

export type ActivityTabView = {
  audit_entries: AuditEntry[];
  activities: ObservedActivity[];
};

export type ExternalChangePolicy = {
  title: boolean;
  state: boolean;
  metadata: boolean;
};

export type AttentionEntry = {
  kind: "external_change" | "review" | "reminder" | "blocked_run" | "implementation_queue";
  link_id: number;
  reminder_id: number | null;
  run_id: number | null;
  queue_id: number | null;
  item_id: number;
  external_object_id: number;
  source_title: string;
  source_url: string;
  activities: Activity[];
  summary: string;
};

export type ExternalLink = {
  id: number;
  item_id: number;
  external_object_id: number;
  reviewed_activity_id: number;
  attention_policy: ExternalChangePolicy | null;
  watch_until: string | null;
  review_at: string | null;
  purpose: LinkPurpose;
  provenance: {
    run_id: number;
    action: GrillContinuationAction;
    discovery: "structured-event" | "output-url";
  } | null;
};

export type LinkPurpose = "to-spec" | "to-tickets" | "others";

export type ExternalLinkView = {
  link: ExternalLink;
  object: ExternalObject;
  snapshot: ExternalSnapshot | null;
  attention_policy: ExternalChangePolicy;
  attention_entry: AttentionEntry | null;
};

export type ExternalLinkAction = {
  link: ExternalLinkView;
  warning: string | null;
};

export type ItemView = {
  item: Item;
  context_id: number;
  context_name: string;
  project_name: string;
  relationships: ItemRelation[];
  workspaces: Workspace[];
  worktrees: Worktree[];
  runs: Run[];
  implementation_queues: ImplementationQueue[];
  links: ExternalLinkView[];
};

export type ItemDeletionPlan = {
  itemId: number;
  humanIdentifier: string;
  title: string;
  reminderCount: number;
  relationshipCount: number;
  workspaces: {
    id: number;
    itemId: number;
  }[];
  runIds: number[];
  activeRunIds: number[];
  linkIds: number[];
  orphanedExternalObjectIds: number[];
  orphanedSnapshotCount: number;
  orphanedActivityCount: number;
};

export type ItemDeletionPreview = {
  plan: ItemDeletionPlan;
  blockers: string[];
};

export type ItemDeletionResult = {
  summary: {
    itemId: number;
    reminderCount: number;
    relationshipCount: number;
    workspaceCount: number;
    runCount: number;
    linkCount: number;
    externalObjectCount: number;
    snapshotCount: number;
    activityCount: number;
  };
};

export type ExternalObjectDeletionPlan = {
  externalObjectId: number;
  provider: ExternalObject["provider"];
  kind: ExternalObject["kind"];
  externalKey: string;
  canonicalUrl: string;
  linkIds: number[];
  snapshotCount: number;
  activityCount: number;
};

export type ExternalObjectDeletionPreview = {
  plan: ExternalObjectDeletionPlan;
  links: {
    linkId: number;
    itemId: number;
    itemIdentifier: string;
    itemTitle: string;
  }[];
  providerWarning: string;
};

export type ExternalObjectDeletionResult = {
  summary: {
    externalObjectId: number;
    linkCount: number;
    snapshotCount: number;
    activityCount: number;
  };
};

export type ExternalLinkDeletionResult = {
  linkId: number;
  externalObjectId: number;
  externalObjectDeleted: boolean;
};

export type RepositoryDeletionPlan = {
  repositoryId: number;
  name: string;
  remoteUrl: string;
  workspaces: {
    id: number;
    itemId: number;
  }[];
};

export type RepositoryDeletionPreview = {
  plan: RepositoryDeletionPlan;
  blockers: string[];
};

export type MachineDeletionRun = {
  id: number;
  itemId: number;
  itemIdentifier: string;
  itemTitle: string;
  workspaceId: number | null;
  worktreeId: number | null;
  state: RunState;
  paneStatus: RunPaneStatus;
};

export type MachineDeletionPreview = {
  plan: {
    machineId: number;
    name: string;
    runs: MachineDeletionRun[];
    activeRunIds: number[];
    worktreeIds: number[];
    repositoryLocationRepositoryIds: number[];
  };
  blockers: string[];
};

export type MachineDeletionResult = {
  machineId: number;
  runCount: number;
  worktreeCount: number;
  repositoryLocationCount: number;
  stopAttemptCount: number;
  stopFailureCount: number;
};

export type ParentDeletionPlan = {
  contextId: number | null;
  projectId: number | null;
  name: string;
  projects: { id: number; name: string }[];
  items: {
    id: number;
    humanIdentifier: string;
    title: string;
    projectId: number;
  }[];
  repositories: {
    id: number;
    name: string;
    remoteUrl: string;
    projectId: number;
  }[];
  machines: { id: number; name: string }[];
  workspaces: {
    id: number;
    itemId: number;
  }[];
  runs: {
    id: number;
    itemId: number;
    itemIdentifier: string;
    itemTitle: string;
    workspaceId: number | null;
    worktreeId: number | null;
    machineId: number;
    state: RunState;
    paneStatus: RunPaneStatus;
  }[];
  activeRunIds: number[];
  reminderCount: number;
  relationshipCount: number;
  linkIds: number[];
  attentionDefaults: ContextAttentionDefault[];
  orphanedExternalObjectIds: number[];
  orphanedSnapshotCount: number;
  orphanedActivityCount: number;
};

export type ParentDeletionPreview = {
  plan: ParentDeletionPlan;
  blockers: string[];
};

export type ParentDeletionResult = {
  summary: {
    contextId: number | null;
    projectId: number | null;
    projectCount: number;
    itemCount: number;
    repositoryCount: number;
    machineCount: number;
    workspaceCount: number;
    runCount: number;
    reminderCount: number;
    relationshipCount: number;
    linkCount: number;
    attentionDefaultCount: number;
    externalObjectCount: number;
    snapshotCount: number;
    activityCount: number;
  };
};

export type ResetLocalDataSummary = {
  contextCount: number;
  projectCount: number;
  repositoryCount: number;
  itemCount: number;
  workspaceCount: number;
  machineCount: number;
  runCount: number;
  reminderCount: number;
  relationshipCount: number;
  linkCount: number;
  externalObjectCount: number;
  snapshotCount: number;
  activityCount: number;
  attentionDefaultCount: number;
};

export type ResetLocalDataRecord = {
  kind: string;
  id: number;
  label: string;
};

export type ResetLocalDataPreview = {
  plan: {
    summary: ResetLocalDataSummary;
    affectedRecords: ResetLocalDataRecord[];
  workspaces: ItemDeletionPlan["workspaces"];
  };
  auditEntryCount: number;
  blockers: string[];
  confirmationPhrase: string;
};

export type ResetLocalDataResult = {
  summary: ResetLocalDataSummary;
  auditEntryCount: number;
};

export type RunDeletionResult = {
  runId: number;
};

export type RepositoryDeletionResult = {
  repositoryId: number;
  workspaceCount: number;
};

export type HomeView = {
  needs_attention: ItemView[];
  attention_entries: AttentionEntry[];
  running: ItemView[];
  waiting: ItemView[];
  due: ItemView[];
  completed: ItemView[];
};

export type PollResult = {
  refreshed: number;
  failures: { external_object_id: number; error: string }[];
};

export type MachineObservationFailureKind = "unreachable" | "tmuxQueryFailed";

export type MachineObservationFailure = {
  machineId: number;
  machineName: string;
  kind: MachineObservationFailureKind;
  message: string;
};

export type RunReconciliationResult = {
  failures: MachineObservationFailure[];
  changed: boolean;
};

export type AuditAction = {
  action: string;
  context_id?: number;
  project_id?: number;
  repository_id?: number;
  item_id?: number;
  from?: string;
  to?: string;
  workspace_id?: number;
  worktree_id?: number;
  repository_count?: number | null;
  workspace_count?: number | null;
  archived?: boolean;
  machine_id?: number;
  run_count?: number | null;
  observation?: string;
  run_id?: number;
  external_object_id?: number | null;
  link_id?: number;
  external_object_deleted?: boolean | null;
  link_count?: number | null;
  snapshot_count?: number | null;
  activity_count?: number | null;
  from_item_id?: number;
  to_item_id?: number;
  summary?: ItemDeletionResult["summary"] | ParentDeletionResult["summary"];
};

export type AuditEntry = {
  id: number;
  recorded_at: number;
  action: AuditAction;
};

export type ContextAttentionDefault = {
  context_id: number;
  object_kind: ExternalObjectKind;
  policy: ExternalChangePolicy;
};

export type * from "./execution-types";
export type * from "./terminal-types";
