export type AgentKind = "claude" | "codex";
export type ExecutionProfile = "investigate" | "implement" | "review" | "custom";
export type RunState = "unknown" | "working" | "blocked" | "finished";
export type RunPaneStatus = "unknown" | "available" | "missing";

export type Run = {
  id: number;
  item_id: number;
  workset_id: number;
  machine_id: number;
  agent: AgentKind;
  execution_profile: ExecutionProfile;
  prompt: string;
  working_directory: string;
  session_name: string;
  pane_id: string;
  started_at: number;
  state: RunState;
  pane_status: RunPaneStatus;
};

export type RunSuggestion = {
  machineId: number;
  machineName: string;
  agent: AgentKind;
  sessionName: string;
  paneId: string;
  currentPath: string;
  worksetId: number;
  worksetRootDirectory: string;
  worksetBranch: string;
  itemId: number;
  itemIdentifier: string;
  itemTitle: string;
  contextId: number;
  contextName: string;
};

export type RunPromptSelection = {
  includeObjective: boolean;
  includeNotes: boolean;
  externalObjectIds: number[];
};
