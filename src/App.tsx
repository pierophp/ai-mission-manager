import {
  FormEvent,
  ReactNode,
  useRef,
  useEffect,
  useState,
} from "react";
import { Link, useRouter, useRouterState } from "@tanstack/react-router";
import { Moon, Sun } from "lucide-react";
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
import { EmbeddedTerminal } from "./components/terminal-runtime/EmbeddedTerminal";
import { useAppRuntime } from "./runtime/AppRuntimeProvider";
import { errorMessage } from "./runtime/errors";
import type {
  Run,
  RunPaneStatus,
  RunState,
} from "./runtime/execution-types";
import type { PaneTab } from "./runtime/terminal-types";
import { WorkPage } from "./features/work/WorkPage";
import { StructurePage } from "./features/structure/StructurePage";
import { externalObjectKindLabel } from "./features/work/work-utils";

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
    activity,
    error,
    isCheckingDependencies,
    setError,
    refreshHome,
    refreshSearch,
    refreshRunSuggestions,
    refreshActivity,
    refreshStructure,
    refreshHealthStatus,
    completeSetup,
    contextFilterId,
    searchQuery,
  } = useAppRuntime();
  const auditHistory = activity.audit_entries;
  const observedActivities = activity.activities;
  const [setupContextName, setSetupContextName] = useState("Personal");
  const [setupProvider, setSetupProvider] = useState<ProviderChoice>("github");
  const [isSaving, setIsSaving] = useState(false);
  const [showHealthDetails, setShowHealthDetails] = useState(false);
  const [theme, setTheme] = useState<Theme>(loadTheme);
  const themePreferenceRef = useRef<Theme | undefined>(loadStoredTheme());
  const [terminalRequest, setTerminalRequest] = useState<{
    worksetId: number;
    pane: PaneTab;
  }>();
  const router = useRouter();
  const pathname = useRouterState({
    select: (state) => state.location.pathname,
  });
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
    if (themePreferenceRef.current) saveTheme(theme);
  }, [theme]);

  useEffect(() => {
    if (structure.contexts[0] && (!setupContextName.trim() || setupContextName === "Personal")) {
      setSetupContextName(structure.contexts[0].name);
    }
  }, [setupContextName, structure.contexts]);

  useEffect(() => {
    if (setupState) setSetupProvider(setupState.completed ? setupState.provider : "github");
  }, [setupState]);

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

  async function updateHomeAfterEdit() {
    await Promise.all([
      refreshStructure(),
      refreshHome(contextFilterId),
      refreshSearch(searchQuery),
      refreshRunSuggestions(),
      refreshActivity(),
    ]);
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
        <StructurePage onResetComplete={() => setTerminalRequest(undefined)} />
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
