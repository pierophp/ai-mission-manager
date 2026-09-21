import {
  FormEvent,
  ReactNode,
  useRef,
  useEffect,
  useMemo,
  useState,
} from "react";
import { Link, useRouter, useRouterState } from "@tanstack/react-router";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import { Moon, Sun } from "lucide-react";
import "@xterm/xterm/css/xterm.css";
import { Alert, AlertDescription } from "./components/ui/alert";
import { Badge } from "./components/ui/badge";
import { Button } from "./components/ui/button";
import { Card } from "./components/ui/card";
import { Empty, EmptyDescription } from "./components/ui/empty";
import { appShellLayoutClassName } from "./components/app-shell-layout";
import {
  applyTheme,
  loadStoredTheme,
  loadTheme,
  saveTheme,
  type Theme,
} from "./theme";
import {
  structureAdapter,
  terminalRuntimeAdapter,
} from "./runtime/adapters";
import { useAppRuntime } from "./runtime/AppRuntimeProvider";
import { errorMessage } from "./runtime/errors";
import { parseWorkSearch } from "./features/work/work-search";
import { WorkPage } from "./features/work/WorkPage";
import {
  externalObjectKindLabel,
  flattenHome,
  uniqueItems,
} from "./features/work/work-utils";

export type Context = {
  id: number;
  name: string;
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

export type Project = {
  id: number;
  context_id: number;
  name: string;
  defaults: {
    item_status: ItemStatus;
  };
};

export type Repository = {
  id: number;
  project_id: number;
  name: string;
  remote_url: string;
};

export type WorksetRepository = {
  repository_id: number;
  branch_override: string | null;
  base_branch_override: string | null;
  current_branch: string;
  is_dirty: boolean;
};

export type Workset = {
  id: number;
  item_id: number;
  root_directory: string;
  branch: string;
  archived: boolean;
  repositories: WorksetRepository[];
};

export type WorksetRepositoryInput = {
  repositoryId: number;
  branchOverride: string | null;
  baseBranchOverride: string | null;
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
};

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

export type PaneTab = {
  paneId: string;
  sessionName: string;
  runId: number;
  label: string;
  available: boolean;
  paneIndex: number;
  pid: number;
  columns: number;
  rows: number;
  title: string;
  currentCommand: string;
  currentPath: string;
};

export type TerminalAttachment = {
  terminalId: string;
  sessionName: string;
  paneId: string;
  snapshot: number[];
  panes: PaneTab[];
};

export type TerminalOutputEvent = {
  terminalId: string;
  paneId: string;
  data: number[];
};

export type TerminalExitEvent = {
  terminalId: string;
  paneId: string;
  code: number | null;
};

export type RunPromptSelection = {
  includeObjective: boolean;
  includeNotes: boolean;
  externalObjectIds: number[];
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
  kind: "external_change" | "review" | "reminder" | "blocked_run";
  link_id: number;
  reminder_id: number | null;
  run_id: number | null;
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
};

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
  worksets: Workset[];
  archived_worksets: Workset[];
  runs: Run[];
  links: ExternalLinkView[];
};

export type RepositoryRemovalReport = {
  repository_id: number;
  name: string;
  path: string;
  current_branch: string;
  unpushed_commits: string[];
  unpushed_commits_unknown: boolean;
  uncommitted_changes: string[];
};

export type WorksetRemovalReport = {
  workset_id: number;
  root_directory: string;
  repositories: RepositoryRemovalReport[];
  safe: boolean;
  blockers: string[];
};

export type ItemDeletionPlan = {
  itemId: number;
  humanIdentifier: string;
  title: string;
  reminderCount: number;
  relationshipCount: number;
  worksets: {
    id: number;
    rootDirectory: string;
    branch: string;
    archived: boolean;
  }[];
  runIds: number[];
  activeRunIds: number[];
  linkIds: number[];
  orphanedExternalObjectIds: number[];
  orphanedSnapshotCount: number;
  orphanedActivityCount: number;
};

export type WorksetDeletionPreview = {
  worksetId: number;
  rootDirectory: string;
  branch: string;
  archived: boolean;
  safe: boolean;
  blockers: string[];
  safetyReport: WorksetRemovalReport | null;
};

export type ItemDeletionPreview = {
  plan: ItemDeletionPlan;
  worksets: WorksetDeletionPreview[];
  blockers: string[];
};

export type ItemDeletionResult = {
  summary: {
    itemId: number;
    reminderCount: number;
    relationshipCount: number;
    worksetCount: number;
    runCount: number;
    linkCount: number;
    externalObjectCount: number;
    snapshotCount: number;
    activityCount: number;
  };
  worksetDirectoriesDeleted: boolean;
  physicalCleanupWarning: string | null;
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
  worksets: {
    id: number;
    rootDirectory: string;
    branch: string;
    archived: boolean;
  }[];
};

export type RepositoryDeletionPreview = {
  plan: RepositoryDeletionPlan;
  worksets: WorksetDeletionPreview[];
  blockers: string[];
};

export type MachineDeletionRun = {
  id: number;
  itemId: number;
  itemIdentifier: string;
  itemTitle: string;
  worksetId: number;
  state: RunState;
  paneStatus: RunPaneStatus;
};

export type MachineDeletionPreview = {
  plan: {
    machineId: number;
    name: string;
    runs: MachineDeletionRun[];
    activeRunIds: number[];
  };
  blockers: string[];
};

export type MachineDeletionResult = {
  machineId: number;
  runCount: number;
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
  worksets: {
    id: number;
    itemId: number;
    rootDirectory: string;
    branch: string;
    archived: boolean;
  }[];
  runs: {
    id: number;
    itemId: number;
    itemIdentifier: string;
    itemTitle: string;
    worksetId: number;
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
  worksets: WorksetDeletionPreview[];
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
    worksetCount: number;
    runCount: number;
    reminderCount: number;
    relationshipCount: number;
    linkCount: number;
    attentionDefaultCount: number;
    externalObjectCount: number;
    snapshotCount: number;
    activityCount: number;
  };
  worksetDirectoriesDeleted: boolean;
  physicalCleanupWarning: string | null;
};

export type ResetLocalDataSummary = {
  contextCount: number;
  projectCount: number;
  repositoryCount: number;
  itemCount: number;
  worksetCount: number;
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
    worksets: ItemDeletionPlan["worksets"];
  };
  auditEntryCount: number;
  worksets: WorksetDeletionPreview[];
  blockers: string[];
  confirmationPhrase: string;
};

export type ResetLocalDataResult = {
  summary: ResetLocalDataSummary;
  auditEntryCount: number;
  worksetDirectoriesDeleted: boolean;
  physicalCleanupWarning: string | null;
};

export type RunDeletionResult = {
  runId: number;
};

export type RepositoryDeletionResult = {
  repositoryId: number;
  worksetCount: number;
  worksetDirectoriesDeleted: boolean;
  physicalCleanupWarning: string | null;
};

export type WorksetRemovalResult = {
  worksetId: number;
  worksetDirectoriesDeleted: boolean;
  physicalCleanupWarning: string | null;
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

export type AuditAction = {
  action: string;
  context_id?: number;
  project_id?: number;
  repository_id?: number;
  item_id?: number;
  from?: string;
  to?: string;
  workset_id?: number;
  repository_count?: number | null;
  workset_count?: number | null;
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
  summary?:
    | ItemDeletionResult["summary"]
    | ParentDeletionResult["summary"];
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

export type AppTab = "work" | "structure" | "activity";

const appTabs: {
  id: AppTab;
  label: string;
  description: string;
  path: `/${AppTab}`;
}[] = [
  {
    id: "work",
    label: "Work",
    description: "One calm view of what needs your attention, what is moving, and what is waiting.",
    path: "/work",
  },
  {
    id: "structure",
    label: "Structure",
    description: "Contexts, Projects, Repositories, and Machines that support your work.",
    path: "/structure",
  },
  {
    id: "activity",
    label: "Activity",
    description: "A record of the actions the app has taken and the changes it has observed.",
    path: "/activity",
  },
];

const warmOutlineButtonClass =
  "border-[var(--input-border)] bg-transparent text-[var(--warm-text)] hover:border-[var(--accent)] hover:bg-[var(--surface-warm)] hover:text-[var(--accent-strong)]";

const itemStatuses: ItemStatus[] = ["Inbox", "Active", "Waiting", "Done"];

function appTabForPath(pathname: string): AppTab {
  if (pathname === "/structure") return "structure";
  if (pathname === "/activity") return "activity";
  return "work";
}

function isAppRoutePath(pathname: string): boolean {
  return pathname === "/work" || pathname === "/structure" || pathname === "/activity";
}

export function AppShell() {
  const {
    setupState,
    healthStatus,
    structure,
    home,
    activity,
    error,
    isCheckingDependencies,
    setError,
    refreshHome,
    refreshSearch,
    refreshRunSuggestions,
    refreshActivity,
    refreshStructure,
    refreshAll,
    refreshHealthStatus,
    completeSetup,
  } = useAppRuntime();
  const { contexts, projects, repositories, machines, attentionDefaults } = structure;
  const auditHistory = activity.audit_entries;
  const observedActivities = activity.activities;
  const refreshAuditHistory = refreshActivity;
  const [repositoryDeletionPreview, setRepositoryDeletionPreview] =
    useState<RepositoryDeletionPreview>();
  const [machineDeletionPreview, setMachineDeletionPreview] =
    useState<MachineDeletionPreview>();
  const [parentDeletionPreview, setParentDeletionPreview] =
    useState<ParentDeletionPreview>();
  const [resetLocalDataPreview, setResetLocalDataPreview] =
    useState<ResetLocalDataPreview>();
  const [captureContextId, setCaptureContextId] = useState<number>();
  const [captureProjectId, setCaptureProjectId] = useState<number>();
  const [contextName, setContextName] = useState("");
  const [projectName, setProjectName] = useState("");
  const [projectDefaultStatus, setProjectDefaultStatus] =
    useState<ItemStatus>("Inbox");
  const [repositoryName, setRepositoryName] = useState("");
  const [repositoryRemoteUrl, setRepositoryRemoteUrl] = useState("");
  const [machineName, setMachineName] = useState("");
  const [machineSocketName, setMachineSocketName] = useState("ai-mission-manager");
  const [machineKind, setMachineKind] = useState<"local" | "ssh">("ssh");
  const [machineHost, setMachineHost] = useState("");
  const [machineUser, setMachineUser] = useState("");
  const [machinePort, setMachinePort] = useState("");
  const [machineIdentityFile, setMachineIdentityFile] = useState("");
  const [machineKnownHostsFile, setMachineKnownHostsFile] = useState("");
  const [machineStrictHostKeyChecking, setMachineStrictHostKeyChecking] =
    useState("accept-new");
  const [setupContextName, setSetupContextName] = useState("Personal");
  const [setupProvider, setSetupProvider] = useState<ProviderChoice>("github");
  const [attentionObjectKind, setAttentionObjectKind] =
    useState<ExternalObjectKind>("pull_request");
  const [attentionDefaultPolicy, setAttentionDefaultPolicy] =
    useState<ExternalChangePolicy>({ title: true, state: true, metadata: true });
  const [isSaving, setIsSaving] = useState(false);
  const [showHealthDetails, setShowHealthDetails] = useState(false);
  const [theme, setTheme] = useState<Theme>(loadTheme);
  const themePreferenceRef = useRef<Theme | undefined>(loadStoredTheme());
  const [terminalRequest, setTerminalRequest] = useState<{
    worksetId: number;
    pane: PaneTab;
  }>();

  const captureProjects = projects.filter(
    (project) => project.context_id === captureContextId,
  );
  const allItems = useMemo(
    () => uniqueItems(home ? flattenHome(home) : []),
    [home],
  );
  const router = useRouter();
  const pathname = useRouterState({
    select: (state) => state.location.pathname,
  });
  const locationSearch = useRouterState({
    select: (state) => state.location.search,
  });
  const workSearch = parseWorkSearch(locationSearch);
  const routeContextFilterId = pathname === "/work" ? workSearch.contextId : undefined;
  const routeSearchQuery = pathname === "/work" ? workSearch.q ?? "" : "";
  const activeTab = appTabForPath(pathname);
  const activeTabDetails = appTabs.find((tab) => tab.id === activeTab) ?? appTabs[0];
  const isDarkTheme = theme === "dark";
  const themeToggleLabel = isDarkTheme ? "Switch to light mode" : "Switch to dark mode";
  const themeModeLabel = isDarkTheme ? "Light mode" : "Dark mode";
  const runtimeState = healthStatus?.runtime.state ?? "unavailable";

  useEffect(() => {
    if (isAppRoutePath(pathname)) return;
    void router.navigate({ to: "/work", replace: true });
  }, [pathname, router]);

  useEffect(() => {
    applyTheme(theme);
    if (themePreferenceRef.current) {
      saveTheme(theme);
    }
  }, [theme]);

  useEffect(() => {
    if (contexts[0] && (!setupContextName.trim() || setupContextName === "Personal")) {
      setSetupContextName(contexts[0].name);
    }
  }, [contexts, setupContextName]);

  useEffect(() => {
    if (setupState) {
      setSetupProvider(setupState.completed ? setupState.provider : "github");
    }
  }, [setupState]);

  useEffect(() => {
    const nextCaptureContextId =
      contexts.find((context) => context.id === captureContextId)?.id ?? contexts[0]?.id;
    const nextCaptureProjectId = projects.find(
      (project) =>
        project.id === captureProjectId && project.context_id === nextCaptureContextId,
    )?.id ?? projects.find(
      (project) => project.context_id === nextCaptureContextId,
    )?.id;
    if (nextCaptureContextId !== captureContextId) {
      setCaptureContextId(nextCaptureContextId);
    }
    if (nextCaptureProjectId !== captureProjectId) {
      setCaptureProjectId(nextCaptureProjectId);
    }
  }, [captureContextId, captureProjectId, contexts, projects]);

  async function handleCompleteSetup(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setIsSaving(true);
    try {
      await completeSetup(setupContextName, setupProvider);
      setError(undefined);
    } catch (setupError) {
      setError(errorMessage(setupError));
    } finally {
      setIsSaving(false);
    }
  }

  useEffect(() => {
    const configured = attentionDefaults.find(
      (attentionDefault) =>
        attentionDefault.context_id === captureContextId &&
        attentionDefault.object_kind === attentionObjectKind,
    );
    setAttentionDefaultPolicy(
      configured?.policy ?? { title: true, state: true, metadata: true },
    );
  }, [attentionDefaults, attentionObjectKind, captureContextId]);

  function handleCaptureContextChange(nextContextId: number) {
    setCaptureContextId(nextContextId);
    setCaptureProjectId(
      projects.find((project) => project.context_id === nextContextId)?.id,
    );
  }

  async function handleCreateContext(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setIsSaving(true);
    try {
      const context = await structureAdapter.createContext(contextName);
      const nextStructure = await refreshStructure();
      setCaptureContextId(context.id);
      setCaptureProjectId(
        nextStructure.projects.find((project) => project.context_id === context.id)?.id,
      );
      setContextName("");
      await refreshAuditHistory();
      setError(undefined);
    } catch (saveError) {
      setError(errorMessage(saveError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleCreateProject(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!captureContextId) {
      setError("Choose a Context before creating a Project.");
      return;
    }

    setIsSaving(true);
    try {
      const project = await structureAdapter.createProject(
        projectName,
        captureContextId,
        projectDefaultStatus,
      );
      await refreshStructure();
      setCaptureProjectId(project.id);
      setProjectName("");
      setProjectDefaultStatus("Inbox");
      await refreshAuditHistory();
      setError(undefined);
    } catch (saveError) {
      setError(errorMessage(saveError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handlePrepareProjectDeletion(projectId: number) {
    setIsSaving(true);
    try {
      const preview = await structureAdapter.prepareProjectDeletion(projectId);
      setParentDeletionPreview(preview);
      setError(undefined);
    } catch (previewError) {
      window.alert(errorMessage(previewError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handlePrepareContextDeletion(contextId: number) {
    setIsSaving(true);
    try {
      const preview = await structureAdapter.prepareContextDeletion(contextId);
      setParentDeletionPreview(preview);
      setError(undefined);
    } catch (previewError) {
      window.alert(errorMessage(previewError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handlePrepareResetLocalData() {
    setIsSaving(true);
    try {
      const preview = await structureAdapter.prepareReset();
      setResetLocalDataPreview(preview);
      setError(undefined);
    } catch (previewError) {
      window.alert(errorMessage(previewError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleResetLocalData() {
    if (!resetLocalDataPreview || resetLocalDataPreview.blockers.length > 0) return;
    const confirmation = window.prompt(
      `This permanently resets all local records. Type ${resetLocalDataPreview.confirmationPhrase} to continue. Workset directories will be confirmed separately.`,
      "",
    );
    if (confirmation === null) return;
    const deleteWorksetDirectories =
      resetLocalDataPreview.worksets.length === 0 ||
      window.confirm(
        `Permanently delete these ${resetLocalDataPreview.worksets.length} Workset director${resetLocalDataPreview.worksets.length === 1 ? "y" : "ies"} from disk too? Choose Cancel to keep the directories while removing their local records.`,
      );

    setIsSaving(true);
    try {
      const result = await structureAdapter.reset(confirmation, deleteWorksetDirectories);
      setResetLocalDataPreview(undefined);
      await navigateWorkSearch({ contextId: undefined });
      setTerminalRequest(undefined);
      await refreshAll();
      const summary = result.summary;
      window.alert(
        `Reset local data. Removed ${summary.contextCount} Context(s), ${summary.projectCount} Project(s), ${summary.repositoryCount} Repository record(s), ${summary.itemCount} Item(s), ${summary.worksetCount} Workset(s), ${summary.machineCount} Machine(s), ${summary.runCount} Run(s), ${summary.reminderCount} reminder(s), ${summary.relationshipCount} relationship(s), ${summary.linkCount} Link(s), ${summary.externalObjectCount} External Object(s), ${summary.snapshotCount} snapshot(s), ${summary.activityCount} Activity record(s), ${summary.attentionDefaultCount} attention default(s), and ${result.auditEntryCount} prior audit entr${result.auditEntryCount === 1 ? "y" : "ies"}. A new Personal Context and Default Project are ready.`,
      );
      if (result.physicalCleanupWarning) {
        window.alert(result.physicalCleanupWarning);
      }
    } catch (resetError) {
      window.alert(errorMessage(resetError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleDeleteProject(projectId: number) {
    if (
      !parentDeletionPreview ||
      parentDeletionPreview.plan.projectId !== projectId ||
      parentDeletionPreview.blockers.length > 0
    ) {
      return;
    }
    const { plan } = parentDeletionPreview;
    const deleteWorksetDirectories =
      plan.worksets.length > 0 &&
      parentDeletionPreview.worksets.every((workset) => workset.safe) &&
      window.confirm(
        `Permanently delete these ${plan.worksets.length} Workset director${plan.worksets.length === 1 ? "y" : "ies"} from disk too? Choose Cancel to keep the directories while removing their records.`,
      );

    setIsSaving(true);
    try {
      const result = await structureAdapter.deleteProject(
        projectId,
        plan.items.map((item) => item.id),
        plan.repositories.map((repository) => repository.id),
        plan.worksets.map((workset) => workset.id),
        deleteWorksetDirectories,
      );
      setParentDeletionPreview(undefined);
      await updateHomeAfterEdit();
      showParentDeletionResult("Project", result);
    } catch (deleteError) {
      setParentDeletionPreview(undefined);
      window.alert(errorMessage(deleteError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleDeleteContext(contextId: number) {
    if (
      !parentDeletionPreview ||
      parentDeletionPreview.plan.contextId !== contextId ||
      parentDeletionPreview.blockers.length > 0
    ) {
      return;
    }
    const { plan } = parentDeletionPreview;
    const deleteWorksetDirectories =
      plan.worksets.length > 0 &&
      parentDeletionPreview.worksets.every((workset) => workset.safe) &&
      window.confirm(
        `Permanently delete these ${plan.worksets.length} Workset director${plan.worksets.length === 1 ? "y" : "ies"} from disk too? Choose Cancel to keep the directories while removing their records.`,
      );

    setIsSaving(true);
    try {
      const result = await structureAdapter.deleteContext({
        contextId,
        projectIds: plan.projects.map((project) => project.id),
        itemIds: plan.items.map((item) => item.id),
        repositoryIds: plan.repositories.map((repository) => repository.id),
        worksetIds: plan.worksets.map((workset) => workset.id),
        machineIds: plan.machines.map((machine) => machine.id),
        deleteWorksetDirectories,
      });
      if (routeContextFilterId === contextId) {
        await navigateWorkSearch({ contextId: undefined });
      }
      setParentDeletionPreview(undefined);
      await updateHomeAfterEdit(
        routeContextFilterId === contextId ? undefined : routeContextFilterId,
      );
      showParentDeletionResult("Context", result);
    } catch (deleteError) {
      setParentDeletionPreview(undefined);
      window.alert(errorMessage(deleteError));
    } finally {
      setIsSaving(false);
    }
  }

  function showParentDeletionResult(
    kind: "Project" | "Context",
    result: ParentDeletionResult,
  ) {
    const { summary } = result;
    window.alert(
      `Deleted ${kind}: ${summary.projectCount} Project(s), ${summary.itemCount} Item(s), ${summary.repositoryCount} Repository record(s), ${summary.machineCount} Machine(s), ${summary.worksetCount} Workset(s), ${summary.runCount} Run(s), ${summary.linkCount} Link(s), and ${summary.externalObjectCount} orphaned External Object(s).`,
    );
    if (result.physicalCleanupWarning) {
      window.alert(result.physicalCleanupWarning);
    }
  }

  async function handleRegisterRepository(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!captureProjectId) {
      setError("Choose a Project before registering a Repository.");
      return;
    }

    setIsSaving(true);
    try {
      await structureAdapter.registerRepository(
        captureProjectId,
        repositoryName,
        repositoryRemoteUrl,
      );
      await refreshStructure();
      setRepositoryName("");
      setRepositoryRemoteUrl("");
      await refreshAuditHistory();
      setError(undefined);
    } catch (saveError) {
      setError(errorMessage(saveError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handlePrepareRepositoryDeletion(repositoryId: number) {
    setIsSaving(true);
    try {
      const preview = await structureAdapter.prepareRepositoryDeletion(repositoryId);
      setRepositoryDeletionPreview(preview);
      setError(undefined);
    } catch (previewError) {
      window.alert(errorMessage(previewError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleDeleteRepository(repositoryId: number) {
    if (
      !repositoryDeletionPreview ||
      repositoryDeletionPreview.plan.repositoryId !== repositoryId ||
      repositoryDeletionPreview.blockers.length > 0
    ) {
      return;
    }
    const { plan } = repositoryDeletionPreview;
    if (
      !window.confirm(
        `Delete Repository ${plan.name} and its local Workset records? This cannot be undone.`,
      )
    ) {
      return;
    }
    const deleteWorksetDirectories =
      plan.worksets.length > 0 &&
      window.confirm(
        `Permanently delete these ${plan.worksets.length} Workset director${plan.worksets.length === 1 ? "y" : "ies"} from disk too? Choose Cancel to keep the directories while removing their records.`,
      );

    setIsSaving(true);
    try {
      const result = await structureAdapter.deleteRepository(
        repositoryId,
        plan.worksets.map((workset) => workset.id),
        deleteWorksetDirectories,
      );
      setRepositoryDeletionPreview(undefined);
      await updateHomeAfterEdit();
      if (result.physicalCleanupWarning) {
        window.alert(result.physicalCleanupWarning);
      }
    } catch (deleteError) {
      setRepositoryDeletionPreview(undefined);
      window.alert(errorMessage(deleteError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handlePrepareMachineDeletion(machineId: number) {
    setIsSaving(true);
    try {
      const preview = await structureAdapter.prepareMachineDeletion(machineId);
      setMachineDeletionPreview(preview);
      setError(undefined);
    } catch (previewError) {
      window.alert(errorMessage(previewError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleDeleteMachine(machineId: number) {
    if (
      !machineDeletionPreview ||
      machineDeletionPreview.plan.machineId !== machineId ||
      machineDeletionPreview.blockers.length > 0
    ) {
      return;
    }
    const { plan } = machineDeletionPreview;
    if (
      !window.confirm(
        `Delete Machine ${plan.name} and its ${plan.runs.length} Run record${plan.runs.length === 1 ? "" : "s"}? This cannot be undone.`,
      )
    ) {
      return;
    }

    setIsSaving(true);
    try {
      const result = await structureAdapter.deleteMachine(
        machineId,
        plan.runs.map((run) => run.id),
      );
      setMachineDeletionPreview(undefined);
      await updateHomeAfterEdit();
      window.alert(
        `Deleted Machine ${plan.name} and ${result.runCount} finished Run record${result.runCount === 1 ? "" : "s"}.`,
      );
    } catch (deleteError) {
      setMachineDeletionPreview(undefined);
      window.alert(errorMessage(deleteError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleDeleteFinishedRunFromStructure(runId: number) {
    if (!window.confirm(`Delete finished Run #${runId} from Run history? This cannot be undone.`)) {
      return;
    }
    setIsSaving(true);
    try {
      await structureAdapter.deleteRun(runId);
      setMachineDeletionPreview(undefined);
      await updateHomeAfterEdit();
    } catch (deleteError) {
      setMachineDeletionPreview(undefined);
      window.alert(errorMessage(deleteError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleRegisterMachine(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!captureContextId || !machineName.trim() || !machineSocketName.trim()) {
      setError("Choose a Context and name the Machine before registering it.");
      return;
    }

    const transport: MachineTransport =
      machineKind === "local"
        ? { kind: "local" }
        : {
            kind: "ssh",
            host: machineHost.trim(),
            user: machineUser.trim() || null,
            port: machinePort.trim() ? Number(machinePort) : null,
            identityFile: machineIdentityFile.trim() || null,
            knownHostsFile: machineKnownHostsFile.trim() || null,
            strictHostKeyChecking: machineStrictHostKeyChecking || null,
          };
    setIsSaving(true);
    try {
      await structureAdapter.registerMachine(
        captureContextId,
        machineName,
        machineSocketName,
        transport,
      );
      await refreshStructure();
      setMachineName("");
      setMachineHost("");
      setMachineUser("");
      setMachinePort("");
      setMachineIdentityFile("");
      setMachineKnownHostsFile("");
      await refreshAuditHistory();
      setError(undefined);
    } catch (saveError) {
      setError(errorMessage(saveError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleCheckMachine(machineId: number) {
    try {
      await structureAdapter.checkMachine(machineId);
      await refreshStructure();
      await refreshAuditHistory();
    } catch (checkError) {
      setError(errorMessage(checkError));
    }
  }

  async function saveAttentionDefault(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!captureContextId) return;
    setIsSaving(true);
    try {
      await structureAdapter.setAttentionDefault(
        captureContextId,
        attentionObjectKind,
        attentionDefaultPolicy,
      );
      await refreshStructure();
      await updateHomeAfterEdit();
      setError(undefined);
    } catch (saveError) {
      setError(errorMessage(saveError));
    } finally {
      setIsSaving(false);
    }
  }

  async function navigateWorkSearch(
    updates: Partial<{ contextId: number; q: string }>,
  ) {
    if (pathname !== "/work") return;
    await router.navigate({
      to: "/work",
      search: (current) => ({ ...current, ...updates }),
      replace: true,
    });
  }

  async function updateHomeAfterEdit(nextContextFilterId = routeContextFilterId) {
    const [nextStructure] = await Promise.all([
      refreshStructure(),
      refreshHome(nextContextFilterId),
      refreshSearch(routeSearchQuery),
      refreshRunSuggestions(),
      refreshAuditHistory(),
    ]);
    const nextCaptureContextId =
      nextStructure.contexts.find((context) => context.id === captureContextId)?.id ??
      nextStructure.contexts[0]?.id;
    const nextCaptureProjectId = nextStructure.projects.find(
      (project) =>
        project.id === captureProjectId &&
        project.context_id === nextCaptureContextId,
    )?.id ?? nextStructure.projects.find(
      (project) => project.context_id === nextCaptureContextId,
    )?.id;
    setCaptureContextId(nextCaptureContextId);
    setCaptureProjectId(nextCaptureProjectId);
  }

  return (
    <main className={appShellLayoutClassName}>
      <header className="flex items-start justify-between gap-6 max-[510px]:flex-col">
        <div>
          <p className="eyebrow">AI Mission Manager</p>
          <h1>{activeTabDetails.label}</h1>
          <p className="subtitle">{activeTabDetails.description}</p>
        </div>
        <div className="flex items-start gap-3.5 max-[510px]:w-full max-[510px]:flex-wrap">
          <Button
            type="button"
            variant="outline"
            size="sm"
            className={`h-[34px] gap-1.5 px-2.5 text-xs ${warmOutlineButtonClass}`}
            aria-label={themeToggleLabel}
            aria-pressed={isDarkTheme}
            onClick={() => {
              const nextTheme = isDarkTheme ? "light" : "dark";
              themePreferenceRef.current = nextTheme;
              setTheme(nextTheme);
            }}
          >
            {isDarkTheme ? <Sun aria-hidden="true" /> : <Moon aria-hidden="true" />}
            <span>{themeModeLabel}</span>
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-auto min-h-8 gap-2 px-2 text-xs text-[var(--subtle)] hover:bg-[var(--surface-warm)] hover:text-[var(--accent-strong)] max-[510px]:whitespace-normal"
            aria-label="Runtime and provider health"
            aria-expanded={showHealthDetails}
            onClick={() => setShowHealthDetails((current) => !current)}
          >
            <span
              aria-hidden="true"
              className={`size-2 shrink-0 rounded-full ring-4 ${
                runtimeState === "available"
                  ? "bg-[var(--success)] ring-[rgb(var(--success-rgb)/0.14)]"
                  : runtimeState === "unavailable"
                    ? "bg-[var(--danger)] ring-[rgb(var(--danger-rgb)/0.14)]"
                    : "bg-[var(--warning)] ring-[rgb(var(--warning-rgb)/0.14)]"
              }`}
            />
            <span>Runtime: {healthStatus ? dependencyStateLabel(healthStatus.runtime.state) : "Checking"}</span>
            <span className="text-[var(--muted-light)]">·</span>
            <span>GitHub: {healthStatus ? dependencyStateLabel(healthStatus.provider.state) : "Checking"}</span>
          </Button>
        </div>
      </header>

      <TabNavigation activeTab={activeTab} />

      {showHealthDetails && setupState?.completed && healthStatus && (
        <HealthDetails
          health={healthStatus}
          isCheckingDependencies={isCheckingDependencies}
          onCheckDependencies={() => void refreshHealthStatus(null)}
        />
      )}

      {setupState && !setupState.completed && (
        <SetupWizard
          contextName={setupContextName}
          provider={setupProvider}
          health={healthStatus}
          isSaving={isSaving}
          isCheckingDependencies={isCheckingDependencies}
          onContextNameChange={setSetupContextName}
          onProviderChange={setSetupProvider}
          onCheckDependencies={() => void refreshHealthStatus(setupProvider)}
          onSubmit={handleCompleteSetup}
        />
      )}

      {error && <ErrorAlert message={error} />}

      {terminalRequest && (
        <EmbeddedTerminal
          key={`${terminalRequest.worksetId}-${terminalRequest.pane.paneId}`}
          worksetId={terminalRequest.worksetId}
          initialPane={terminalRequest.pane}
          onClose={() => setTerminalRequest(undefined)}
        />
      )}

      {activeTab === "activity" && (
        <section className="activity-page" aria-labelledby="activity-heading">
          <section className="activity-section audit-record">
            <div className="section-heading">
              <div>
                <p className="eyebrow">Append-only record</p>
                <h2 id="activity-heading">Recent actions</h2>
              </div>
              <span className="item-count">{auditHistory.length} entries</span>
            </div>
            {auditHistory.length === 0 ? (
              <EmptyState>No actions have been recorded yet.</EmptyState>
            ) : (
              <ol className="audit-record-list">
                {auditHistory.slice(0, 50).map((entry) => (
                  <li key={entry.id}>
                    <span>{auditActionLabel(entry.action)}</span>
                    <time dateTime={new Date(entry.recorded_at * 1000).toISOString()}>
                      {new Date(entry.recorded_at * 1000).toLocaleString()}
                    </time>
                  </li>
                ))}
              </ol>
            )}
          </section>

          <section className="activity-section observed-activity" aria-labelledby="observed-activity-heading">
            <div className="section-heading">
              <div>
                <p className="eyebrow">Observed External Objects</p>
                <h2 id="observed-activity-heading">Activity</h2>
              </div>
              <span className="item-count">{observedActivities.length} observations</span>
            </div>
            {observedActivities.length === 0 ? (
              <EmptyState>No External Object changes have been observed yet.</EmptyState>
            ) : (
              <div className="observed-activity-list">
                {observedActivities.slice(0, 50).map((entry) => (
                  <ObservedActivityCard key={entry.activity.id} entry={entry} />
                ))}
              </div>
            )}
          </section>
        </section>
      )}

      {activeTab === "work" && (
        <WorkPage
          onChanged={updateHomeAfterEdit}
          onOpenTerminal={(worksetId, pane) => setTerminalRequest({ worksetId, pane })}
        />
      )}

      {activeTab === "structure" && (
        <section className="organise-card" aria-labelledby="organise-heading">
          <div className="section-heading">
          <div>
            <p className="eyebrow">Organisation</p>
            <h2 id="organise-heading">Contexts and Projects</h2>
          </div>
          <span className="key-hint">Your boundaries</span>
        </div>
        <div className="organisation-grid">
          <div>
            <h3>Contexts</h3>
            <form className="compact-form" onSubmit={handleCreateContext}>
              <label>
                <span>New Context</span>
                <input
                  value={contextName}
                  onChange={(event) => setContextName(event.target.value)}
                  placeholder="Work"
                  disabled={isSaving}
                />
              </label>
              <button type="submit" disabled={isSaving || !contextName.trim()}>
                Add Context
              </button>
            </form>
            <ul className="entity-list">
              {contexts.map((context) => (
                <li className="entity-row" key={context.id}>
                  <div>
                    <span>{context.name}</span>
                    <span className="entity-meta">
                      {projects.filter((project) => project.context_id === context.id).length}{" "}
                      Projects
                    </span>
                  </div>
                  <div className="entity-actions">
                    <button
                      type="button"
                      className="secondary-button"
                      disabled={isSaving}
                      onClick={() => void handlePrepareContextDeletion(context.id)}
                    >
                      Review deletion
                    </button>
                  </div>
                  {parentDeletionPreview?.plan.contextId === context.id && (
                    <ParentDeletionPreviewCard
                      preview={parentDeletionPreview}
                      kind="Context"
                      disabled={isSaving}
                      onConfirm={() => void handleDeleteContext(context.id)}
                      onCancel={() => setParentDeletionPreview(undefined)}
                    />
                  )}
                </li>
              ))}
            </ul>
          </div>
            <div>
              <h3>Projects</h3>
            <form className="compact-form" onSubmit={handleCreateProject}>
              <label>
                <span>Context</span>
                <select
                  value={captureContextId ?? ""}
                  onChange={(event) =>
                    handleCaptureContextChange(Number(event.target.value))
                  }
                  disabled={isSaving || contexts.length === 0}
                >
                  {contexts.map((context) => (
                    <option value={context.id} key={context.id}>
                      {context.name}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                <span>New Project</span>
                <input
                  value={projectName}
                  onChange={(event) => setProjectName(event.target.value)}
                  placeholder="Billing"
                  disabled={isSaving}
                />
              </label>
              <label>
                <span>New Item starts as</span>
                <select
                  value={projectDefaultStatus}
                  onChange={(event) =>
                    setProjectDefaultStatus(event.target.value as ItemStatus)
                  }
                  disabled={isSaving}
                >
                  {itemStatuses.map((status) => (
                    <option value={status} key={status}>
                      {status}
                    </option>
                  ))}
                </select>
              </label>
              <button
                type="submit"
                disabled={isSaving || !projectName.trim() || !captureContextId}
              >
                Add Project
              </button>
              </form>
              <ul className="entity-list">
                {captureProjects.length === 0 ? (
                  <li>
                    <Empty className="items-start border-0 p-0 text-left">
                      <EmptyDescription>
                        No Projects remain in this Context. Create one above to add Items here.
                      </EmptyDescription>
                    </Empty>
                  </li>
                ) : (
                  captureProjects.map((project) => (
                    <li className="entity-row" key={project.id}>
                      <div>
                        <span>{project.name}</span>
                        <span className="entity-meta">
                          {allItems.filter((item) => item.item.project_id === project.id).length} Items ·
                          starts {project.defaults.item_status}
                        </span>
                      </div>
                      <div className="entity-actions">
                        <button
                          type="button"
                          className="secondary-button"
                          disabled={isSaving}
                          onClick={() => void handlePrepareProjectDeletion(project.id)}
                        >
                          Review deletion
                        </button>
                      </div>
                      {parentDeletionPreview?.plan.projectId === project.id && (
                        <ParentDeletionPreviewCard
                          preview={parentDeletionPreview}
                          kind="Project"
                          disabled={isSaving}
                          onConfirm={() => void handleDeleteProject(project.id)}
                          onCancel={() => setParentDeletionPreview(undefined)}
                        />
                      )}
                    </li>
                  ))
                )}
              </ul>
            </div>
            <div>
              <h3>Repositories</h3>
              <form className="compact-form" onSubmit={handleRegisterRepository}>
                <label>
                  <span>Project</span>
                  <select
                    value={captureProjectId ?? ""}
                    onChange={(event) => setCaptureProjectId(Number(event.target.value))}
                    disabled={isSaving || captureProjects.length === 0}
                  >
                    {captureProjects.map((project) => (
                      <option value={project.id} key={project.id}>
                        {project.name}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  <span>Directory name</span>
                  <input
                    value={repositoryName}
                    onChange={(event) => setRepositoryName(event.target.value)}
                    placeholder="service-a"
                    disabled={isSaving}
                  />
                </label>
                <label>
                  <span>Remote URL</span>
                  <input
                    value={repositoryRemoteUrl}
                    onChange={(event) => setRepositoryRemoteUrl(event.target.value)}
                    placeholder="git@github.com:acme/service-a.git"
                    disabled={isSaving}
                  />
                </label>
                <button
                  type="submit"
                  disabled={
                    isSaving ||
                    !captureProjectId ||
                    !repositoryName.trim() ||
                    !repositoryRemoteUrl.trim()
                  }
                >
                  Register Repository
                </button>
              </form>
              <ul className="entity-list">
                {repositories
                  .filter((repository) => repository.project_id === captureProjectId)
                  .map((repository) => (
                    <li className="entity-row" key={repository.id}>
                      <div>
                        <span>{repository.name}</span>
                        <span className="entity-meta">{repository.remote_url}</span>
                      </div>
                      <div className="entity-actions">
                        <button
                          type="button"
                          className="secondary-button"
                          disabled={isSaving}
                          onClick={() =>
                            void handlePrepareRepositoryDeletion(repository.id)
                          }
                        >
                          Review deletion
                        </button>
                      </div>
                      {repositoryDeletionPreview?.plan.repositoryId === repository.id && (
                        <div className="deletion-preview" role="alert">
                          <strong>Repository deletion preview</strong>
                          <p>
                            Deleting <b>{repository.name}</b> removes its local record.
                            Every referencing Workset must be included explicitly.
                          </p>
                          {repositoryDeletionPreview.worksets.length > 0 && (
                            <div className="deletion-worksets">
                              <span className="relationship-label">
                                Affected Workset directories
                              </span>
                              {repositoryDeletionPreview.worksets.map((workset) => (
                                <div className="deletion-workset" key={workset.worksetId}>
                                  <strong>
                                    {workset.branch} {workset.archived ? "· Archived" : ""}
                                  </strong>
                                  <code>{workset.rootDirectory}</code>
                                  {workset.safetyReport?.repositories.map((repository) => (
                                    <span key={repository.repository_id}>
                                      {repository.unpushed_commits_unknown
                                        ? `${repository.name}: unpushed commits could not be verified.`
                                        : repository.unpushed_commits.length > 0
                                          ? `${repository.name}: ${repository.unpushed_commits.length} unpushed commit(s).`
                                          : `${repository.name}: no unpushed commits.`}
                                      {repository.uncommitted_changes.length > 0
                                        ? ` ${repository.uncommitted_changes.length} uncommitted change(s).`
                                        : " No uncommitted changes."}
                                    </span>
                                  ))}
                                  {workset.blockers.map((blocker) => (
                                    <span key={blocker}>{blocker}</span>
                                  ))}
                                </div>
                              ))}
                            </div>
                          )}
                          {repositoryDeletionPreview.plan.worksets.length === 0 && (
                            <p>No Worksets reference this Repository.</p>
                          )}
                          {repositoryDeletionPreview.blockers.length > 0 && (
                            <div className="deletion-blockers">
                              <strong>Deletion blocked</strong>
                              {repositoryDeletionPreview.blockers.map((blocker) => (
                                <span key={blocker}>{blocker}</span>
                              ))}
                              <p>
                                Resolve each finding, then create a fresh preview.
                              </p>
                            </div>
                          )}
                          <div className="deletion-preview-actions">
                            <button
                              type="button"
                              className="danger-button"
                              disabled={
                                isSaving || repositoryDeletionPreview.blockers.length > 0
                              }
                              onClick={() =>
                                void handleDeleteRepository(repository.id)
                              }
                            >
                              Confirm logical deletion
                            </button>
                            <button
                              type="button"
                              className="text-button"
                              disabled={isSaving}
                              onClick={() => setRepositoryDeletionPreview(undefined)}
                            >
                              Cancel
                            </button>
                          </div>
                        </div>
                      )}
                    </li>
                  ))}
              </ul>
            </div>
            <div>
              <h3>Machines</h3>
              <form className="compact-form" onSubmit={handleRegisterMachine}>
                <label>
                  <span>Context</span>
                  <select
                    value={captureContextId ?? ""}
                    onChange={(event) =>
                      handleCaptureContextChange(Number(event.target.value))
                    }
                    disabled={isSaving || contexts.length === 0}
                  >
                    {contexts.map((context) => (
                      <option value={context.id} key={context.id}>
                        {context.name}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  <span>Name</span>
                  <input
                    value={machineName}
                    onChange={(event) => setMachineName(event.target.value)}
                    placeholder="Build Mac"
                    disabled={isSaving}
                  />
                </label>
                <label>
                  <span>Transport</span>
                  <select
                    value={machineKind}
                    onChange={(event) =>
                      setMachineKind(event.target.value as "local" | "ssh")
                    }
                    disabled={isSaving}
                  >
                    <option value="ssh">SSH remote</option>
                    <option value="local">Local</option>
                  </select>
                </label>
                <label>
                  <span>tmux socket</span>
                  <input
                    value={machineSocketName}
                    onChange={(event) => setMachineSocketName(event.target.value)}
                    placeholder="ai-mission-manager"
                    disabled={isSaving}
                  />
                </label>
                {machineKind === "ssh" && (
                  <>
                    <label>
                      <span>Host</span>
                      <input
                        value={machineHost}
                        onChange={(event) => setMachineHost(event.target.value)}
                        placeholder="build.example.com"
                        disabled={isSaving}
                      />
                    </label>
                    <label>
                      <span>User</span>
                      <input
                        value={machineUser}
                        onChange={(event) => setMachineUser(event.target.value)}
                        placeholder="runner"
                        disabled={isSaving}
                      />
                    </label>
                    <label>
                      <span>Port</span>
                      <input
                        type="number"
                        min="1"
                        value={machinePort}
                        onChange={(event) => setMachinePort(event.target.value)}
                        placeholder="22"
                        disabled={isSaving}
                      />
                    </label>
                    <label>
                      <span>Identity file</span>
                      <input
                        value={machineIdentityFile}
                        onChange={(event) => setMachineIdentityFile(event.target.value)}
                        placeholder="~/.ssh/mission"
                        disabled={isSaving}
                      />
                    </label>
                    <label>
                      <span>Known hosts file</span>
                      <input
                        value={machineKnownHostsFile}
                        onChange={(event) => setMachineKnownHostsFile(event.target.value)}
                        placeholder="~/.ssh/known_hosts"
                        disabled={isSaving}
                      />
                    </label>
                    <label>
                      <span>Host-key checking</span>
                      <select
                        value={machineStrictHostKeyChecking}
                        onChange={(event) =>
                          setMachineStrictHostKeyChecking(event.target.value)
                        }
                        disabled={isSaving}
                      >
                        <option value="yes">Strict</option>
                        <option value="accept-new">Accept new</option>
                        <option value="no">Disabled</option>
                      </select>
                    </label>
                  </>
                )}
                <button
                  type="submit"
                  disabled={
                    isSaving ||
                    !captureContextId ||
                    !machineName.trim() ||
                    !machineSocketName.trim() ||
                    (machineKind === "ssh" && !machineHost.trim())
                  }
                >
                  Register Machine
                </button>
              </form>
              <ul className="entity-list">
                {machines
                  .filter((machine) => machine.context_id === captureContextId)
                  .map((machine) => (
                    <li className="entity-row" key={machine.id}>
                      <div>
                        <span>
                          {machine.name} ·{" "}
                          {machine.transport.kind === "ssh" ? "SSH" : "Local"}
                        </span>
                        <span className="entity-meta">
                          Last observed: {machine.last_observed}
                          {machine.last_observed_at
                            ? " · " +
                              new Date(machine.last_observed_at * 1000).toLocaleString()
                            : ""}
                        </span>
                      </div>
                      <div className="entity-actions">
                        <button
                          type="button"
                          className="text-button"
                          disabled={isSaving}
                          onClick={() => void handleCheckMachine(machine.id)}
                        >
                          Check
                        </button>
                        <button
                          type="button"
                          className="secondary-button"
                          disabled={isSaving}
                          onClick={() => void handlePrepareMachineDeletion(machine.id)}
                        >
                          Review deletion
                        </button>
                      </div>
                      {machineDeletionPreview?.plan.machineId === machine.id && (
                        <div className="deletion-preview" role="alert">
                          <strong>Machine deletion preview</strong>
                          <p>
                            Deleting <b>{machine.name}</b> removes the Machine record and
                            every finished Run that points to it. Panes are owned by the
                            Terminal Runtime and are not parsed or removed here.
                          </p>
                          {machineDeletionPreview.plan.runs.length === 0 ? (
                            <p>No Runs reference this Machine.</p>
                          ) : (
                            <div className="deletion-worksets">
                              <span className="relationship-label">Runs to remove</span>
                              {machineDeletionPreview.plan.runs.map((run) => (
                                <div className="deletion-workset" key={run.id}>
                                  <strong>
                                    Run #{run.id} · {run.itemIdentifier} · {run.state}
                                  </strong>
                                  <span>{run.itemTitle} · Workset #{run.worksetId}</span>
                                  <span>Pane state: {run.paneStatus}</span>
                                  {run.state === "finished" && (
                                    <button
                                      type="button"
                                      className="text-button"
                                      disabled={isSaving}
                                      onClick={() =>
                                        void handleDeleteFinishedRunFromStructure(run.id)
                                      }
                                    >
                                      Delete finished Run
                                    </button>
                                  )}
                                </div>
                              ))}
                            </div>
                          )}
                          {machineDeletionPreview.blockers.length > 0 && (
                            <div className="deletion-blockers">
                              <strong>Deletion blocked</strong>
                              {machineDeletionPreview.blockers.map((blocker) => (
                                <span key={blocker}>{blocker}</span>
                              ))}
                              <p>Stop each active Run, then create a fresh preview.</p>
                            </div>
                          )}
                          <div className="deletion-preview-actions">
                            <button
                              type="button"
                              className="danger-button"
                              disabled={
                                isSaving || machineDeletionPreview.blockers.length > 0
                              }
                              onClick={() => void handleDeleteMachine(machine.id)}
                            >
                              Confirm and delete Machine
                            </button>
                            <button
                              type="button"
                              className="text-button"
                              disabled={isSaving}
                              onClick={() => setMachineDeletionPreview(undefined)}
                            >
                              Cancel
                            </button>
                          </div>
                        </div>
                      )}
                    </li>
                  ))}
              </ul>
            </div>
          </div>
        <form className="attention-default-form" onSubmit={saveAttentionDefault}>
          <div>
            <h3>Attention defaults</h3>
            <p className="column-hint">
              Choose which changes interrupt Links in a Context by External Object type.
            </p>
          </div>
          <label>
            <span>Context</span>
            <select
              value={captureContextId ?? ""}
              onChange={(event) =>
                handleCaptureContextChange(Number(event.target.value))
              }
              disabled={isSaving || contexts.length === 0}
            >
              {contexts.map((context) => (
                <option value={context.id} key={context.id}>
                  {context.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            <span>External Object type</span>
            <select
              value={attentionObjectKind}
              onChange={(event) =>
                setAttentionObjectKind(event.target.value as ExternalObjectKind)
              }
              disabled={isSaving}
            >
              <option value="issue">Issue</option>
              <option value="pull_request">Pull request</option>
              <option value="generic">Generic link</option>
            </select>
          </label>
          <div className="attention-default-options">
            {(["title", "state", "metadata"] as const).map((kind) => (
              <label key={kind}>
                <input
                  type="checkbox"
                  checked={attentionDefaultPolicy[kind]}
                  onChange={(event) =>
                    setAttentionDefaultPolicy((current) => ({
                      ...current,
                      [kind]: event.target.checked,
                    }))
                  }
                  disabled={isSaving}
                />
                {kind[0].toUpperCase() + kind.slice(1)} changes
              </label>
            ))}
          </div>
          <button type="submit" disabled={isSaving || !captureContextId}>
            Save attention defaults
          </button>
        </form>
        <section className="danger-zone" aria-labelledby="danger-zone-heading">
          <div className="section-heading">
            <div>
              <p className="eyebrow">Danger zone</p>
              <h3 id="danger-zone-heading">Reset all local data</h3>
              <p className="column-hint">
                Remove the local working model, cached External Objects, and audit history, then
                choose separately what happens to every Workset directory. Setup and dependency
                configuration remain available.
              </p>
            </div>
            <button
              type="button"
              className="danger-button"
              disabled={isSaving}
              onClick={() => void handlePrepareResetLocalData()}
            >
              Review reset impact
            </button>
          </div>
          {resetLocalDataPreview && (
            <ResetLocalDataPreviewCard
              preview={resetLocalDataPreview}
              disabled={isSaving}
              onConfirm={() => void handleResetLocalData()}
              onCancel={() => setResetLocalDataPreview(undefined)}
            />
          )}
        </section>
        </section>
      )}
    </main>
  );
}

function EmptyState({ children }: { children: ReactNode }) {
  return (
    <Empty className="items-start border-0 p-0 py-8 text-left">
      <EmptyDescription>{children}</EmptyDescription>
    </Empty>
  );
}

function ErrorAlert({ message }: { message: string }) {
  return (
    <Alert
      variant="destructive"
      className="mt-4 border-[var(--danger-border)] bg-[var(--danger-surface)]"
    >
      <AlertDescription className="text-[var(--danger)]">{message}</AlertDescription>
    </Alert>
  );
}

function TabNavigation({ activeTab }: { activeTab: AppTab }) {
  return (
    <nav className="mt-8 border-b border-[var(--line)]" aria-label="Home sections">
      <div className="flex gap-6 overflow-x-auto" aria-label="Home sections">
        {appTabs.map((tab) => (
          <Button
            asChild
            key={tab.id}
            variant="ghost"
            size="sm"
            className={`h-auto shrink-0 rounded-none border-b-2 px-0.5 py-3 text-xs font-bold uppercase tracking-[0.08em] hover:bg-transparent hover:text-[var(--accent-strong)] ${
              activeTab === tab.id
                ? "border-[var(--accent)] text-[var(--ink)]"
                : "border-transparent text-[var(--muted)]"
            }`}
          >
            <Link to={tab.path} aria-current={activeTab === tab.id ? "page" : undefined}>
              {tab.label}
            </Link>
          </Button>
        ))}
      </div>
    </nav>
  );
}

function ResetLocalDataPreviewCard({
  preview,
  disabled,
  onConfirm,
  onCancel,
}: {
  preview: ResetLocalDataPreview;
  disabled: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const { summary } = preview.plan;
  const counts: [number, string][] = [
    [summary.contextCount, "Contexts"],
    [summary.projectCount, "Projects"],
    [summary.repositoryCount, "Repositories"],
    [summary.itemCount, "Items"],
    [summary.worksetCount, "Worksets"],
    [summary.machineCount, "Machines"],
    [summary.runCount, "Runs"],
    [summary.reminderCount, "reminders"],
    [summary.relationshipCount, "relationships"],
    [summary.linkCount, "Links"],
    [summary.externalObjectCount, "External Objects"],
    [summary.snapshotCount, "snapshots"],
    [summary.activityCount, "Activity records"],
    [summary.attentionDefaultCount, "attention defaults"],
    [preview.auditEntryCount, "prior audit entries"],
  ];

  return (
    <div className="deletion-preview reset-preview" role="alert">
      <strong>Reset impact preview</strong>
      <p>
        This removes only Mission Manager&apos;s local working model. Provider-owned Issues,
        pull requests, and other external objects are never deleted. All listed Workset roots
        must pass the same safety checks used by ordinary deletion before any optional physical
        cleanup is staged.
      </p>
      <div className="parent-deletion-summary">
        {counts.map(([count, label]) => (
          <span key={label}><strong>{count}</strong> {label}</span>
        ))}
      </div>
      {preview.plan.affectedRecords.length > 0 && (
        <div className="deletion-worksets">
          <span className="relationship-label">Records to reset</span>
          {preview.plan.affectedRecords.map((record) => (
            <span key={`${record.kind}-${record.id}`}>
              <strong>{record.kind} #{record.id}</strong> · {record.label}
            </span>
          ))}
        </div>
      )}
      {preview.worksets.length > 0 ? (
        <div className="deletion-worksets">
          <span className="relationship-label">Workset roots to stage and remove</span>
          {preview.worksets.map((workset) => (
            <div className="deletion-workset" key={workset.worksetId}>
              <strong>
                Workset #{workset.worksetId} · {workset.branch} {workset.archived ? "· Archived" : ""}
              </strong>
              <code>{workset.rootDirectory}</code>
              {workset.safetyReport?.repositories.map((repository) => (
                <span key={repository.repository_id}>
                  {repository.unpushed_commits_unknown
                    ? `${repository.name}: unpushed commits could not be verified.`
                    : repository.unpushed_commits.length > 0
                      ? `${repository.name}: ${repository.unpushed_commits.length} unpushed commit(s).`
                      : `${repository.name}: no unpushed commits.`}
                  {repository.uncommitted_changes.length > 0
                    ? ` ${repository.uncommitted_changes.length} uncommitted change(s).`
                    : " No uncommitted changes."}
                </span>
              ))}
              {workset.blockers.map((blocker) => <span key={blocker}>{blocker}</span>)}
            </div>
          ))}
        </div>
      ) : (
        <p>No Workset roots are registered. The local records will still be reset.</p>
      )}
      {preview.blockers.length > 0 && (
        <div className="deletion-blockers">
          <strong>Reset blocked</strong>
          {preview.blockers.map((blocker) => <span key={blocker}>{blocker}</span>)}
          <p>Resolve every finding, then create a fresh preview.</p>
        </div>
      )}
      <p className="column-hint">
        Confirmation requires typing <code>{preview.confirmationPhrase}</code> exactly.
      </p>
      <div className="deletion-preview-actions">
        <button
          type="button"
          className="danger-button"
          disabled={disabled || preview.blockers.length > 0}
          onClick={onConfirm}
        >
          Reset all local data
        </button>
        <button type="button" className="text-button" disabled={disabled} onClick={onCancel}>
          Cancel
        </button>
      </div>
    </div>
  );
}

function ParentDeletionPreviewCard({
  preview,
  kind,
  disabled,
  onConfirm,
  onCancel,
}: {
  preview: ParentDeletionPreview;
  kind: "Project" | "Context";
  disabled: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  const { plan } = preview;
  const physicalCleanupBlockers = preview.worksets.flatMap((workset) =>
    workset.blockers.map((blocker) => `Workset #${workset.worksetId}: ${blocker}`),
  );
  return (
    <div className="deletion-preview parent-deletion-preview" role="alert">
      <strong>{kind} deletion preview</strong>
      <p>
        Deleting <b>{plan.name}</b> removes the complete local dependency graph below. Provider-owned
        Issues and pull requests are never deleted.
      </p>
      <div className="parent-deletion-summary">
        <span><strong>{plan.projects.length}</strong> Projects</span>
        <span><strong>{plan.items.length}</strong> Items</span>
        <span><strong>{plan.repositories.length}</strong> Repositories</span>
        <span><strong>{plan.machines.length}</strong> Machines</span>
        <span><strong>{plan.worksets.length}</strong> Worksets</span>
        <span><strong>{plan.runs.length}</strong> Runs</span>
        <span><strong>{plan.reminderCount}</strong> reminders</span>
        <span><strong>{plan.relationshipCount}</strong> relationships</span>
        <span><strong>{plan.linkIds.length}</strong> Links</span>
        <span><strong>{plan.attentionDefaults.length}</strong> attention defaults</span>
        <span><strong>{plan.orphanedExternalObjectIds.length}</strong> orphaned External Objects</span>
      </div>
      {plan.projects.length > 0 && (
        <div className="deletion-worksets">
          <span className="relationship-label">Projects</span>
          {plan.projects.map((project) => <span key={project.id}>#{project.id} · {project.name}</span>)}
        </div>
      )}
      {plan.items.length > 0 && (
        <div className="deletion-worksets">
          <span className="relationship-label">Items</span>
          {plan.items.map((item) => <span key={item.id}>{item.humanIdentifier} · {item.title}</span>)}
        </div>
      )}
      {plan.repositories.length > 0 && (
        <div className="deletion-worksets">
          <span className="relationship-label">Repositories</span>
          {plan.repositories.map((repository) => <span key={repository.id}>#{repository.id} · {repository.name}</span>)}
        </div>
      )}
      {plan.machines.length > 0 && (
        <div className="deletion-worksets">
          <span className="relationship-label">Machines</span>
          {plan.machines.map((machine) => <span key={machine.id}>#{machine.id} · {machine.name}</span>)}
        </div>
      )}
      {plan.worksets.length > 0 && (
        <div className="deletion-worksets">
          <span className="relationship-label">Worksets and physical roots</span>
          {plan.worksets.map((workset) => (
            <div className="deletion-workset" key={workset.id}>
              <strong>#{workset.id} · {workset.branch} {workset.archived ? "· Archived" : ""}</strong>
              <code>{workset.rootDirectory}</code>
            </div>
          ))}
        </div>
      )}
      {plan.runs.length > 0 && (
        <div className="deletion-worksets">
          <span className="relationship-label">Runs</span>
          {plan.runs.map((run) => (
            <span key={run.id}>
              #{run.id} · {run.itemIdentifier} · {run.state} · Workset #{run.worksetId}
            </span>
          ))}
        </div>
      )}
      {plan.linkIds.length > 0 && (
        <p className="column-hint">Links removed: {plan.linkIds.map((linkId) => `#${linkId}`).join(", ")}.</p>
      )}
      {plan.orphanedExternalObjectIds.length > 0 && (
        <p className="column-hint">
          Eligible local External Objects: {plan.orphanedExternalObjectIds.map((id) => `#${id}`).join(", ")}.
          Their {plan.orphanedSnapshotCount} snapshot(s) and {plan.orphanedActivityCount} Activity record(s) will also be removed.
        </p>
      )}
      {physicalCleanupBlockers.length > 0 && (
        <div className="deletion-blockers deletion-warnings">
          <strong>Physical cleanup unavailable</strong>
          <p>
            The Project or Context records can still be deleted, but these Workset directories
            will be kept on disk until their safety findings are resolved.
          </p>
          {physicalCleanupBlockers.map((blocker) => <span key={blocker}>{blocker}</span>)}
        </div>
      )}
      {preview.blockers.length > 0 && (
        <div className="deletion-blockers">
          <strong>Deletion blocked</strong>
          {preview.blockers.map((blocker) => <span key={blocker}>{blocker}</span>)}
          <p>Resolve every finding, then create a fresh preview.</p>
        </div>
      )}
      <div className="deletion-preview-actions">
        <button
          type="button"
          className="danger-button"
          disabled={disabled || preview.blockers.length > 0}
          onClick={onConfirm}
        >
          Confirm and delete {kind}
        </button>
        <button type="button" className="text-button" disabled={disabled} onClick={onCancel}>
          Cancel
        </button>
      </div>
    </div>
  );
}

function SetupWizard({
  contextName,
  provider,
  health,
  isSaving,
  isCheckingDependencies,
  onContextNameChange,
  onProviderChange,
  onCheckDependencies,
  onSubmit,
}: {
  contextName: string;
  provider: ProviderChoice;
  health: HealthStatus | undefined;
  isSaving: boolean;
  isCheckingDependencies: boolean;
  onContextNameChange: (value: string) => void;
  onProviderChange: (value: ProviderChoice) => void;
  onCheckDependencies: () => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
}) {
  return (
    <section className="mt-8" aria-labelledby="setup-heading">
      <Card className="gap-0 rounded-2xl border border-[var(--setup-border)] bg-[var(--surface-setup)] p-6 shadow-none ring-0">
        <div className="section-heading">
          <div>
            <p className="eyebrow">First run</p>
            <h2 id="setup-heading">Make the app ready for your work</h2>
            <p className="setup-intro">
              Choose your first Context, decide whether to connect GitHub, and check the tools
              already installed on this Machine. Mission Manager never installs or authenticates
              anything on your behalf.
            </p>
          </div>
          <span className="item-count">Three quick checks</span>
        </div>
        <form className="setup-form" onSubmit={onSubmit}>
        <div className="setup-step">
          <span className="setup-step-number">1</span>
          <div>
            <h3>Create a Context</h3>
            <p className="column-hint">A Context keeps its Items, providers, and Machines together.</p>
            <label>
              <span>Context name</span>
              <input
                value={contextName}
                onChange={(event) => onContextNameChange(event.target.value)}
                placeholder="Personal"
                disabled={isSaving}
                autoFocus
              />
            </label>
          </div>
        </div>
        <div className="setup-step">
          <span className="setup-step-number">2</span>
          <div>
            <h3>Choose a provider</h3>
            <p className="column-hint">You can keep local work here and connect a provider later.</p>
            <div className="setup-choice-list">
              <label className="setup-choice">
                <input
                  type="radio"
                  name="provider"
                  value="github"
                  checked={provider === "github"}
                  onChange={() => onProviderChange("github")}
                  disabled={isSaving}
                />
                <span><strong>GitHub</strong><small>Use the installed `gh` CLI for Issues and pull requests.</small></span>
              </label>
              <label className="setup-choice">
                <input
                  type="radio"
                  name="provider"
                  value="none"
                  checked={provider === "none"}
                  onChange={() => onProviderChange("none")}
                  disabled={isSaving}
                />
                <span><strong>No provider yet</strong><small>Local Items and Runs remain available.</small></span>
              </label>
            </div>
          </div>
        </div>
        <div className="setup-step setup-dependencies">
          <span className="setup-step-number">3</span>
          <div>
            <div className="setup-dependency-heading">
              <div>
                <h3>Check dependencies</h3>
                <p className="column-hint">Missing or unauthenticated tools do not stop the rest of the app.</p>
              </div>
              <Button
                type="button"
                variant="outline"
                size="sm"
                className={warmOutlineButtonClass}
                onClick={onCheckDependencies}
                disabled={isCheckingDependencies || isSaving}
              >
                {isCheckingDependencies ? "Checking…" : "Check again"}
              </Button>
            </div>
            <DependencyList health={health} />
          </div>
        </div>
        <div className="setup-actions">
          <p className="column-hint">You can revisit tool setup from the health indicator at any time.</p>
          <Button type="submit" disabled={isSaving || !contextName.trim()}>
            {isSaving ? "Saving setup…" : "Finish setup"}
          </Button>
        </div>
        </form>
      </Card>
    </section>
  );
}

function HealthDetails({
  health,
  isCheckingDependencies,
  onCheckDependencies,
}: {
  health: HealthStatus;
  isCheckingDependencies: boolean;
  onCheckDependencies: () => void;
}) {
  return (
    <section className="mt-6" aria-label="Tool health details">
      <Card className="gap-0 rounded-[14px] border border-[var(--line)] bg-[rgb(var(--surface-rgb)/0.72)] p-5 shadow-none ring-0">
        <div className="flex items-start justify-between gap-4 border-b border-[var(--line-soft)] pb-3 max-[760px]:flex-col max-[760px]:items-stretch">
          <div>
            <p className="eyebrow">Tool health</p>
            <h2 className="text-xl">Runtime and provider status</h2>
          </div>
          <Button
            type="button"
            variant="outline"
            size="sm"
            className={warmOutlineButtonClass}
            onClick={onCheckDependencies}
            disabled={isCheckingDependencies}
          >
            {isCheckingDependencies ? "Checking…" : "Check again"}
          </Button>
        </div>
        <div className="mt-1">
          <DependencyList health={health} />
        </div>
      </Card>
    </section>
  );
}

function DependencyList({ health }: { health: HealthStatus | undefined }) {
  const dependencies = health
    ? [health.runtime, health.provider, ...health.agents]
    : [];

  return (
    <ul className="dependency-list">
      {dependencies.length === 0 ? (
        <li className="dependency-row"><span>Checking installed tools…</span></li>
      ) : (
        dependencies.map((dependency) => (
          <li className="dependency-row" key={dependency.key}>
            <Badge
              variant={dependency.state === "available" ? "secondary" : "outline"}
              className={`dependency-state dependency-${dependency.state}`}
            >
              {dependencyStateLabel(dependency.state)}
            </Badge>
            <span className="dependency-copy">
              <strong>{dependency.label}</strong>
              <span>{dependency.message}</span>
              {dependency.executablePath && <code>{dependency.executablePath}</code>}
              {dependency.action && <small>{dependency.action}</small>}
            </span>
          </li>
        ))
      )}
    </ul>
  );
}

function dependencyStateLabel(state: DependencyState): string {
  switch (state) {
    case "available":
      return "Ready";
    case "missing":
      return "Missing";
    case "unauthenticated":
      return "Needs login";
    case "notConfigured":
      return "Not selected";
    case "unavailable":
      return "Unavailable";
  }
}

function EmbeddedTerminal({
  worksetId,
  initialPane,
  onClose,
}: {
  worksetId: number;
  initialPane: PaneTab;
  onClose: () => void;
}) {
  const terminalContainerRef = useRef<HTMLDivElement>(null);
  const attachPaneRef = useRef<((pane: PaneTab) => Promise<void>) | undefined>(undefined);
  const activePaneRef = useRef(initialPane);
  const attachedRef = useRef(false);
  const terminalId = `workset-${worksetId}`;
  const [activePane, setActivePane] = useState(initialPane);
  const [panes, setPanes] = useState<PaneTab[]>([initialPane]);
  const [status, setStatus] = useState("Attaching…");
  const [terminalError, setTerminalError] = useState<string>();

  useEffect(() => {
    const container = terminalContainerRef.current;
    if (!container) return;

    let disposed = false;
    const terminal = new Terminal({
      cursorBlink: true,
      fontFamily: "SFMono-Regular, Menlo, Monaco, Consolas, monospace",
      fontSize: 13,
      theme: {
        background: "#201c19",
        foreground: "#f7f0e6",
        cursor: "#efb28e",
      },
    });
    const fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.open(container);
    fitAddon.fit();

    const inputDisposable = terminal.onData((data) => {
      if (!attachedRef.current) return;
      void terminalRuntimeAdapter.input(
        terminalId,
        Array.from(new TextEncoder().encode(data)),
      ).catch((inputError) => {
        if (!disposed) setTerminalError(errorMessage(inputError));
      });
    });
    const resizeTerminal = () => {
      fitAddon.fit();
      if (!attachedRef.current || terminal.cols < 1 || terminal.rows < 1) return;
      void terminalRuntimeAdapter.resize(terminalId, terminal.cols, terminal.rows).catch((resizeError) => {
        if (!disposed) setTerminalError(errorMessage(resizeError));
      });
    };
    const resizeObserver = new ResizeObserver(resizeTerminal);
    resizeObserver.observe(container);
    const unlisteners: (() => void)[] = [];

    const attachPane = async (pane: PaneTab) => {
      activePaneRef.current = pane;
      setActivePane(pane);
      attachedRef.current = false;
      setStatus(`Attaching ${pane.label}…`);
      setTerminalError(undefined);
      const attachment = await terminalRuntimeAdapter.open(
        worksetId,
        terminalId,
        pane.sessionName,
        pane.paneId,
      );
      if (disposed) return;
      setPanes(attachment.panes);
      const attachedPane =
        attachment.panes.find((candidate) => candidate.paneId === attachment.paneId) ??
        pane;
      activePaneRef.current = attachedPane;
      setActivePane(attachedPane);
      terminal.reset();
      terminal.write(Uint8Array.from(attachment.snapshot));
      attachedRef.current = true;
      setStatus("Connected · closing this view leaves the Run running");
      resizeTerminal();
    };
    attachPaneRef.current = attachPane;

    const start = async () => {
      unlisteners.push(
        await terminalRuntimeAdapter.listen<TerminalOutputEvent>("terminal-output", (event) => {
          const payload = event.payload;
          if (
            payload.terminalId === terminalId &&
            payload.paneId === activePaneRef.current.paneId
          ) {
            terminal.write(Uint8Array.from(payload.data));
          }
        }),
        await terminalRuntimeAdapter.listen<TerminalExitEvent>("terminal-exit", (event) => {
          if (
            event.payload.terminalId === terminalId &&
            event.payload.paneId === activePaneRef.current.paneId &&
            !disposed
          ) {
            attachedRef.current = false;
            setStatus("Pane connection closed; the Run was left untouched");
          }
        }),
      );
      await attachPane(initialPane);
    };
    void start().catch((attachError) => {
      if (!disposed) {
        attachedRef.current = false;
        setStatus("Could not attach");
        setTerminalError(errorMessage(attachError));
      }
    });

    return () => {
      disposed = true;
      attachedRef.current = false;
      attachPaneRef.current = undefined;
      inputDisposable.dispose();
      resizeObserver.disconnect();
      terminal.dispose();
      unlisteners.forEach((unlisten) => unlisten());
      void terminalRuntimeAdapter.close(terminalId).catch(() => undefined);
    };
  }, [initialPane, terminalId, worksetId]);

  return (
    <section className="embedded-terminal" aria-labelledby="embedded-terminal-heading">
      <div className="embedded-terminal-heading">
        <div>
          <p className="eyebrow">Embedded terminal</p>
          <h2 id="embedded-terminal-heading">{activePane.label}</h2>
          <p className="embedded-terminal-status">{status}</p>
        </div>
        <button type="button" className="secondary-button" onClick={onClose}>
          Close view
        </button>
      </div>
      <div className="terminal-tabs" role="tablist" aria-label="Panes in this Workset">
        {panes.map((pane) => (
          <button
            type="button"
            role="tab"
            aria-selected={pane.paneId === activePane.paneId}
            className={pane.paneId === activePane.paneId ? "terminal-tab active" : "terminal-tab"}
            key={`${pane.sessionName}-${pane.paneId}`}
            disabled={!pane.available}
            onClick={() => void attachPaneRef.current?.(pane)}
          >
            {pane.label}
            <small>{pane.currentCommand || pane.currentPath || "unavailable"}</small>
          </button>
        ))}
      </div>
      <div className="terminal-surface" ref={terminalContainerRef} />
      {terminalError && <ErrorAlert message={terminalError} />}
    </section>
  );
}


function ObservedActivityCard({ entry }: { entry: ObservedActivity }) {
  const titleChange = entry.activity.changes.find((change) => change.kind === "title");
  const stateChange = entry.activity.changes.find((change) => change.kind === "state");
  const title = titleChange?.current ?? titleChange?.previous ?? entry.object.external_key;
  const state = stateChange?.current ?? stateChange?.previous;

  return (
    <details className="observed-activity-card">
      <summary>
        <div>
          <strong>{title}</strong>
          <span>
            Observed at this change · {externalObjectKindLabel(entry.object.kind)} · {entry.object.external_key}
            {state ? ` · observed state ${state}` : ""}
          </span>
        </div>
        <time dateTime={new Date(entry.activity.observed_at * 1000).toISOString()}>
          {new Date(entry.activity.observed_at * 1000).toLocaleString()}
        </time>
      </summary>
      <div className="observed-activity-details">
        <a href={entry.object.canonical_url} target="_blank" rel="noreferrer">
          {entry.object.canonical_url}
        </a>
        <ul>
          {entry.activity.changes.map((change, index) => (
            <li key={`${change.kind}-${change.key ?? ""}-${index}`}>
              {externalChangeDescription(change)}
            </li>
          ))}
        </ul>
      </div>
    </details>
  );
}

function externalChangeDescription(change: ExternalChange): string {
  const label =
    change.kind === "title"
      ? "Title"
      : change.kind === "state"
        ? "State"
        : `Metadata ${change.key ?? "value"}`;
  if (change.previous !== null && change.current !== null) {
    return `${label} changed from ${change.previous} to ${change.current}`;
  }
  if (change.current !== null) return `${label} added as ${change.current}`;
  if (change.previous !== null) return `${label} removed (was ${change.previous})`;
  return `${label} changed`;
}


function auditActionLabel(action: AuditAction): string {
  switch (action.action) {
    case "itemCreated":
      return `Created Item #${action.item_id}`;
    case "itemStatusChanged":
      return `Moved Item #${action.item_id} from ${action.from} to ${action.to}`;
    case "itemNotesChanged":
      return `Updated notes on Item #${action.item_id}`;
    case "itemRemindersChanged":
      return `Updated reminders on Item #${action.item_id}`;
    case "itemDeleted":
      return `Deleted Item #${
        action.summary && "itemId" in action.summary
          ? action.summary.itemId
          : action.item_id
      }${
        action.summary && "itemId" in action.summary
          ? ` · ${itemDeletionSummary(action.summary)}`
          : ""
      }`;
    case "projectDeleted":
      return `Deleted Project #${
        action.summary && "projectId" in action.summary
          ? action.summary.projectId
          : action.project_id
      }${
        action.summary && "projectId" in action.summary
          ? ` · ${parentDeletionSummary(action.summary)}`
          : ""
      }`;
    case "contextDeleted":
      return `Deleted Context #${
        action.summary && "contextId" in action.summary
          ? action.summary.contextId
          : action.context_id
      }${
        action.summary && "contextId" in action.summary
          ? ` · ${parentDeletionSummary(action.summary)}`
          : ""
      }`;
    case "itemRelationChanged":
      return `Updated the relationship between Items #${action.from_item_id} and #${action.to_item_id}`;
    case "worksetCreated":
      return `Created Workset #${action.workset_id}`;
    case "worksetArchived":
      return `${action.archived ? "Archived" : "Restored"} Workset #${action.workset_id}`;
    case "worksetRemoved":
      return `Removed Workset #${action.workset_id} · ${countLabel(
        action.repository_count,
        "Repository",
      )}`;
    case "worksetUpdated":
      return `Updated Workset #${action.workset_id}`;
    case "runCreated":
      return `Created Run #${action.run_id}`;
    case "runStopped":
      return `Stopped Run #${action.run_id}`;
    case "runDeleted":
      return `Deleted Run #${action.run_id} · no local descendants`;
    case "runStateChanged":
      return `Run #${action.run_id} changed from ${action.from} to ${action.to}`;
    case "runPaneStatusChanged":
      return `Run #${action.run_id} Pane changed from ${action.from} to ${action.to}`;
    case "externalObjectCreated":
      return `Added External Object #${action.external_object_id}`;
    case "externalObjectRefreshed":
      return `Refreshed External Object #${action.external_object_id}`;
    case "linkCreated":
      return `Linked External Object through Link #${action.link_id}`;
    case "linkUpdated":
      return `Updated Link #${action.link_id}`;
    case "linkDeleted":
      return `Removed Link #${action.link_id} · ${
        action.external_object_deleted === true
          ? `orphaned External Object #${action.external_object_id ?? "?"} removed`
          : action.external_object_deleted === false
            ? "shared External Object retained"
            : "External Object cascade details unavailable"
      }`;
    case "externalObjectDeleted":
      return `Removed External Object #${action.external_object_id} locally · ${externalObjectDeletionSummary(action)}`;
    case "contextCreated":
      return `Created Context #${action.context_id}`;
    case "projectCreated":
      return `Created Project #${action.project_id}`;
    case "repositoryRegistered":
      return `Registered Repository #${action.repository_id}`;
    case "repositoryDeleted":
      return `Deleted Repository #${action.repository_id} · ${countLabel(
        action.workset_count,
        "Workset",
      )}`;
    case "machineRegistered":
      return `Registered Machine #${action.machine_id}`;
    case "machineObserved":
      return `Observed Machine #${action.machine_id} as ${action.observation}`;
    case "machineDeleted":
      return `Deleted Machine #${action.machine_id} · ${countLabel(action.run_count, "Run")}`;
    case "contextAttentionDefaultChanged":
      return `Updated attention defaults for Context #${action.context_id}`;
    case "resetBoundary":
      return `Reset local data · new Personal Context #${action.context_id}, Default Project #${action.project_id}`;
    default:
      return "Recorded action";
  }
}

function itemDeletionSummary(summary: ItemDeletionResult["summary"]): string {
  return cascadeCounts([
    [summary.reminderCount, "reminder"],
    [summary.relationshipCount, "relationship"],
    [summary.worksetCount, "Workset"],
    [summary.runCount, "Run"],
    [summary.linkCount, "Link"],
    [summary.externalObjectCount, "orphaned External Object"],
    [summary.snapshotCount, "snapshot"],
    [summary.activityCount, "Activity record"],
  ]);
}

function externalObjectDeletionSummary(action: AuditAction): string {
  if (
    action.link_count === null ||
    action.link_count === undefined ||
    action.snapshot_count === null ||
    action.snapshot_count === undefined ||
    action.activity_count === null ||
    action.activity_count === undefined
  ) {
    return "cascade details unavailable";
  }
  return cascadeCounts([
    [action.link_count ?? 0, "Link"],
    [action.snapshot_count ?? 0, "snapshot"],
    [action.activity_count ?? 0, "Activity record"],
  ]);
}

function parentDeletionSummary(summary: ParentDeletionResult["summary"]): string {
  return cascadeCounts([
    [summary.projectCount, "Project"],
    [summary.itemCount, "Item"],
    [summary.repositoryCount, "Repository"],
    [summary.machineCount, "Machine"],
    [summary.worksetCount, "Workset"],
    [summary.runCount, "Run"],
    [summary.reminderCount, "reminder"],
    [summary.relationshipCount, "relationship"],
    [summary.linkCount, "Link"],
    [summary.attentionDefaultCount, "attention default"],
    [summary.externalObjectCount, "orphaned External Object"],
    [summary.snapshotCount, "snapshot"],
    [summary.activityCount, "Activity record"],
  ]);
}

function cascadeCounts(counts: [number, string][]): string {
  const nonEmpty = counts
    .filter(([count]) => count > 0)
    .map(([count, label]) => `${count} ${label}${count === 1 ? "" : "s"}`);
  return nonEmpty.length > 0 ? nonEmpty.join(", ") : "no local descendants";
}

function countLabel(count: number | null | undefined, label: string): string {
  if (count === null || count === undefined) return `${label} count unavailable`;
  if (!count) return `no ${label.toLowerCase()} records`;
  return `${count} ${label}${count === 1 ? "" : "s"}`;
}
