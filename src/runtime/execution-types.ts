export type AgentKind = "claude" | "codex";
export type ExecutionProfile =
  | "investigate"
  | "implement"
  | "review"
  | "custom"
  | "grill";
export type RunState = "unknown" | "working" | "blocked" | "finished";
export type RunPaneStatus = "unknown" | "available" | "missing";

export type RunCheckout = {
  repositoryId: number;
  path: string;
  branch: string;
  isDirty: boolean;
};

export type Run = {
  id: number;
  item_id: number;
  workspace_id: number | null;
  repository_id: number | null;
  worktree_id: number | null;
  machine_id: number;
  agent: AgentKind;
  execution_profile: ExecutionProfile;
  model: string | null;
  effort: string | null;
  skill_snapshot: string | null;
  prompt: string;
  working_directory: string;
  session_name: string;
  pane_id: string;
  started_at: number;
  state: RunState;
  pane_status: RunPaneStatus;
  direct_checkouts: RunCheckout[];
};

export type RunSuggestion = {
  machineId: number;
  machineName: string;
  agent: AgentKind;
  sessionName: string;
  paneId: string;
  currentPath: string;
  itemId: number;
  itemIdentifier: string;
  itemTitle: string;
  contextId: number;
  contextName: string;
  workspaceId: number | null;
  repositoryId: number | null;
  worktreeId: number | null;
  locationPath: string | null;
};

export type RunPromptSelection = {
  includeObjective: boolean;
  includeNotes: boolean;
  externalObjectIds: number[];
};
