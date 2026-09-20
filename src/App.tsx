import {
  FormEvent,
  useRef,
  useEffect,
  useMemo,
  useState,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";

type Context = {
  id: number;
  name: string;
};

type ItemStatus = "Inbox" | "Active" | "Waiting" | "Done";

type Project = {
  id: number;
  context_id: number;
  name: string;
  defaults: {
    item_status: ItemStatus;
  };
};

type Repository = {
  id: number;
  project_id: number;
  name: string;
  remote_url: string;
};

type WorksetRepository = {
  repository_id: number;
  branch_override: string | null;
  base_branch_override: string | null;
  current_branch: string;
  is_dirty: boolean;
};

type Workset = {
  id: number;
  item_id: number;
  root_directory: string;
  branch: string;
  archived: boolean;
  repositories: WorksetRepository[];
};

type WorksetRepositoryInput = {
  repositoryId: number;
  branchOverride: string | null;
  baseBranchOverride: string | null;
};

type MachineTransport =
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

type Machine = {
  id: number;
  context_id: number;
  name: string;
  socket_name: string;
  transport: MachineTransport;
  last_observed: "unknown" | "available" | "offline";
  last_observed_at: number | null;
};

type AgentKind = "claude" | "codex";
type ExecutionProfile = "investigate" | "implement" | "review" | "custom";
type RunState = "unknown" | "working" | "blocked" | "finished";

type Run = {
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
};

type PaneTab = {
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

type TerminalAttachment = {
  terminalId: string;
  sessionName: string;
  paneId: string;
  snapshot: number[];
  panes: PaneTab[];
};

type TerminalOutputEvent = {
  terminalId: string;
  paneId: string;
  data: number[];
};

type TerminalExitEvent = {
  terminalId: string;
  paneId: string;
  code: number | null;
};

type RunPromptSelection = {
  includeObjective: boolean;
  includeNotes: boolean;
  externalObjectIds: number[];
};

type Item = {
  id: number;
  human_identifier: string;
  title: string;
  project_id: number;
  status: ItemStatus;
  notes: string;
  reminders: { id: number; remind_at: string }[];
};

type ItemRelationKind = "Blocks" | "BlockedBy" | "RelatedTo";

type ItemRelation = {
  from_item_id: number;
  to_item_id: number;
  kind: ItemRelationKind;
};

type ExternalObject = {
  id: number;
  provider: "github" | "generic";
  kind: "issue" | "pull_request" | "generic";
  external_key: string;
  canonical_url: string;
};

type ExternalObjectKind = ExternalObject["kind"];

type ExternalMetadata = {
  key: string;
  value: string;
};

type ExternalSnapshot = {
  external_object_id: number;
  title: string;
  state: string;
  metadata: ExternalMetadata[];
  fetched_at: number;
};

type ExternalChangeKind = "title" | "state" | "metadata";

type ExternalChange = {
  kind: ExternalChangeKind;
  key: string | null;
  previous: string | null;
  current: string | null;
};

type Activity = {
  id: number;
  external_object_id: number;
  observed_at: number;
  changes: ExternalChange[];
};

type ExternalChangePolicy = {
  title: boolean;
  state: boolean;
  metadata: boolean;
};

type AttentionEntry = {
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

type ExternalLink = {
  id: number;
  item_id: number;
  external_object_id: number;
  reviewed_activity_id: number;
  attention_policy: ExternalChangePolicy | null;
  watch_until: string | null;
  review_at: string | null;
};

type ExternalLinkView = {
  link: ExternalLink;
  object: ExternalObject;
  snapshot: ExternalSnapshot | null;
  attention_policy: ExternalChangePolicy;
  attention_entry: AttentionEntry | null;
};

type ExternalLinkAction = {
  link: ExternalLinkView;
  warning: string | null;
};

type ItemView = {
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

type RepositoryRemovalReport = {
  repository_id: number;
  name: string;
  path: string;
  current_branch: string;
  unpushed_commits: string[];
  unpushed_commits_unknown: boolean;
  uncommitted_changes: string[];
};

type WorksetRemovalReport = {
  workset_id: number;
  root_directory: string;
  repositories: RepositoryRemovalReport[];
};

type HomeView = {
  needs_attention: ItemView[];
  attention_entries: AttentionEntry[];
  running: ItemView[];
  waiting: ItemView[];
  due: ItemView[];
  completed: ItemView[];
};

type PollResult = {
  refreshed: number;
  failures: { external_object_id: number; error: string }[];
};

type ContextAttentionDefault = {
  context_id: number;
  object_kind: ExternalObjectKind;
  policy: ExternalChangePolicy;
};

const itemStatuses: ItemStatus[] = ["Inbox", "Active", "Waiting", "Done"];
const relationKinds: ItemRelationKind[] = [
  "Blocks",
  "BlockedBy",
  "RelatedTo",
];

export function App() {
  const [contexts, setContexts] = useState<Context[]>([]);
  const [projects, setProjects] = useState<Project[]>([]);
  const [repositories, setRepositories] = useState<Repository[]>([]);
  const [machines, setMachines] = useState<Machine[]>([]);
  const [attentionDefaults, setAttentionDefaults] = useState<
    ContextAttentionDefault[]
  >([]);
  const [home, setHome] = useState<HomeView>();
  const [searchResults, setSearchResults] = useState<ItemView[]>([]);
  const [contextFilterId, setContextFilterId] = useState<number>();
  const [captureContextId, setCaptureContextId] = useState<number>();
  const [captureProjectId, setCaptureProjectId] = useState<number>();
  const [title, setTitle] = useState("");
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
  const [attentionObjectKind, setAttentionObjectKind] =
    useState<ExternalObjectKind>("pull_request");
  const [attentionDefaultPolicy, setAttentionDefaultPolicy] =
    useState<ExternalChangePolicy>({ title: true, state: true, metadata: true });
  const [searchQuery, setSearchQuery] = useState("");
  const [error, setError] = useState<string>();
  const [isLoading, setIsLoading] = useState(true);
  const [isSaving, setIsSaving] = useState(false);
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

  useEffect(() => {
    void loadAppState();
  }, []);

  useEffect(() => {
    if (!home || !searchQuery.trim()) {
      setSearchResults([]);
      return;
    }
    void refreshSearch();
  }, [searchQuery, contextFilterId]);

  useEffect(() => {
    const interval = window.setInterval(() => {
      void pollExternalObjects(false);
    }, 5 * 60 * 1000);
    return () => window.clearInterval(interval);
  }, [contextFilterId, searchQuery]);

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    let disposed = false;
    void listen("run-state-changed", () => {
      void refreshHome();
      if (searchQuery.trim()) {
        void refreshSearch();
      }
    }).then((cleanup) => {
      if (disposed) {
        cleanup();
      } else {
        unlisten = cleanup;
      }
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [contextFilterId, searchQuery]);

  useEffect(() => {
    const interval = window.setInterval(() => {
      void refreshHome().catch(() => undefined);
      if (searchQuery.trim()) {
        void refreshSearch().catch(() => undefined);
      }
    }, 3_000);
    return () => window.clearInterval(interval);
  }, [contextFilterId, searchQuery]);

  async function loadAppState() {
    setIsLoading(true);
    try {
      const [loadedContexts, loadedProjects, loadedAttentionDefaults, loadedMachines] = await Promise.all([
        invoke<Context[]>("list_contexts"),
        invoke<Project[]>("list_projects"),
        invoke<ContextAttentionDefault[]>("list_context_attention_defaults"),
        invoke<Machine[]>("list_machines"),
      ]);
      const loadedRepositories = await invoke<Repository[]>("list_repositories");
      const nextCaptureContextId =
        loadedContexts.find((context) => context.id === captureContextId)?.id ??
        loadedContexts[0]?.id;
      const nextCaptureProjectId =
        loadedProjects.find(
          (project) =>
            project.id === captureProjectId &&
            project.context_id === nextCaptureContextId,
        )?.id ??
        loadedProjects.find(
          (project) => project.context_id === nextCaptureContextId,
        )?.id;
      await invoke<PollResult>("poll_external_objects");
      const loadedHome = await invoke<HomeView>("get_home", {
        contextId: contextFilterId ?? null,
        now: currentMinute(),
      });

      setContexts(loadedContexts);
      setProjects(loadedProjects);
      setRepositories(loadedRepositories);
      setMachines(loadedMachines);
      setAttentionDefaults(loadedAttentionDefaults);
      setCaptureContextId(nextCaptureContextId);
      setCaptureProjectId(nextCaptureProjectId);
      setHome(loadedHome);
      setError(undefined);
      if (searchQuery.trim()) {
        await refreshSearch();
      }
    } catch (loadError) {
      setError(errorMessage(loadError));
    } finally {
      setIsLoading(false);
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

  async function refreshHome() {
    const loadedHome = await invoke<HomeView>("get_home", {
      contextId: contextFilterId ?? null,
      now: currentMinute(),
    });
    setHome(loadedHome);
  }

  async function refreshSearch() {
    if (!searchQuery.trim()) {
      setSearchResults([]);
      return;
    }
    const results = await invoke<ItemView[]>("search_items_command", {
      query: searchQuery,
      contextId: null,
    });
    setSearchResults(results);
  }

  async function pollExternalObjects(showErrors: boolean) {
    try {
      const result = await invoke<PollResult>("poll_external_objects");
      if (result.refreshed > 0) {
        await refreshHome();
        await refreshSearch();
      }
      if (showErrors && result.failures.length > 0) {
        setError(`Some External Objects could not be refreshed (${result.failures.length}).`);
      }
    } catch (pollError) {
      if (showErrors) {
        setError(errorMessage(pollError));
      }
    }
  }

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
      const context = await invoke<Context>("create_context", {
        name: contextName,
      });
      const loadedProjects = await invoke<Project[]>("list_projects");
      setContexts((current) => [...current, context]);
      setProjects(loadedProjects);
      setCaptureContextId(context.id);
      setCaptureProjectId(
        loadedProjects.find((project) => project.context_id === context.id)?.id,
      );
      setContextName("");
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
      const project = await invoke<Project>("create_project", {
        name: projectName,
        contextId: captureContextId,
        defaultItemStatus: projectDefaultStatus,
      });
      setProjects((current) => [...current, project]);
      setCaptureProjectId(project.id);
      setProjectName("");
      setProjectDefaultStatus("Inbox");
      setError(undefined);
    } catch (saveError) {
      setError(errorMessage(saveError));
    } finally {
      setIsSaving(false);
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
      const repository = await invoke<Repository>("register_repository", {
        projectId: captureProjectId,
        name: repositoryName,
        remoteUrl: repositoryRemoteUrl,
      });
      setRepositories((current) => [...current, repository]);
      setRepositoryName("");
      setRepositoryRemoteUrl("");
      setError(undefined);
    } catch (saveError) {
      setError(errorMessage(saveError));
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
      const machine = await invoke<Machine>("register_machine", {
        contextId: captureContextId,
        name: machineName,
        socketName: machineSocketName,
        transport,
      });
      setMachines((current) => [...current, machine]);
      setMachineName("");
      setMachineHost("");
      setMachineUser("");
      setMachinePort("");
      setMachineIdentityFile("");
      setMachineKnownHostsFile("");
      setError(undefined);
    } catch (saveError) {
      setError(errorMessage(saveError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleCheckMachine(machineId: number) {
    try {
      const checked = await invoke<Machine>("check_machine", { machineId });
      setMachines((current) =>
        current.map((machine) => (machine.id === checked.id ? checked : machine)),
      );
    } catch (checkError) {
      setError(errorMessage(checkError));
    }
  }

  async function saveAttentionDefault(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!captureContextId) return;
    setIsSaving(true);
    try {
      const saved = await invoke<ContextAttentionDefault>(
        "set_context_attention_default",
        {
          contextId: captureContextId,
          objectKind: attentionObjectKind,
          policy: attentionDefaultPolicy,
        },
      );
      setAttentionDefaults((current) => [
        ...current.filter(
          (attentionDefault) =>
            attentionDefault.context_id !== saved.context_id ||
            attentionDefault.object_kind !== saved.object_kind,
        ),
        saved,
      ]);
      await updateHomeAfterEdit();
      setError(undefined);
    } catch (saveError) {
      setError(errorMessage(saveError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleCreateItem(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!captureContextId || !captureProjectId) {
      setError("Choose a Context and Project before creating an Item.");
      return;
    }

    setIsSaving(true);
    try {
      await invoke<Item>("create_item", {
        title,
        contextId: captureContextId,
        projectId: captureProjectId,
      });
      setTitle("");
      setError(undefined);
      await refreshHome();
    } catch (saveError) {
      setError(errorMessage(saveError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleContextFilterChange(value: string) {
    const nextContextFilterId = value === "all" ? undefined : Number(value);
    setContextFilterId(nextContextFilterId);
    setIsLoading(true);
    try {
      const loadedHome = await invoke<HomeView>("get_home", {
        contextId: nextContextFilterId ?? null,
        now: currentMinute(),
      });
      setHome(loadedHome);
      setError(undefined);
    } catch (loadError) {
      setError(errorMessage(loadError));
    } finally {
      setIsLoading(false);
    }
  }

  async function updateHomeAfterEdit() {
    const [loadedRepositories, loadedMachines] = await Promise.all([
      invoke<Repository[]>("list_repositories"),
      invoke<Machine[]>("list_machines"),
      refreshHome(),
      refreshSearch(),
    ]);
    setRepositories(loadedRepositories);
    setMachines(loadedMachines);
  }

  return (
    <main className="app-shell">
      <header className="app-header">
        <div>
          <p className="eyebrow">AI Mission Manager</p>
          <h1>Home</h1>
          <p className="subtitle">
            One calm view of what needs your attention, what is moving, and what
            is waiting.
          </p>
        </div>
        <div className="status-mark" aria-label="Local database connected">
          <span className="status-dot" /> Local
        </div>
      </header>

      <section className="home-controls" aria-label="Home view controls">
        <label>
          <span>Context</span>
          <select
            value={contextFilterId ?? "all"}
            onChange={(event) => void handleContextFilterChange(event.target.value)}
          >
            <option value="all">All Contexts</option>
            {contexts.map((context) => (
              <option value={context.id} key={context.id}>
                {context.name}
              </option>
            ))}
          </select>
        </label>
        <label className="search-field">
          <span>Search every Context</span>
          <input
            value={searchQuery}
            onChange={(event) => setSearchQuery(event.target.value)}
            placeholder="Search Items, notes, or identifiers"
          />
        </label>
        <button
          type="button"
          className="secondary-button"
          onClick={() => void pollExternalObjects(true)}
        >
          Refresh linked objects
        </button>
      </section>

      {error && <p className="error-message" role="alert">{error}</p>}

      {terminalRequest && (
        <EmbeddedTerminal
          key={`${terminalRequest.worksetId}-${terminalRequest.pane.paneId}`}
          worksetId={terminalRequest.worksetId}
          initialPane={terminalRequest.pane}
          onClose={() => setTerminalRequest(undefined)}
        />
      )}

      {searchQuery.trim() && (
        <section className="search-section" aria-labelledby="search-heading">
          <div className="section-heading">
            <div>
              <p className="eyebrow">Every Context</p>
              <h2 id="search-heading">Search results</h2>
            </div>
            <span className="item-count">{searchResults.length} matches</span>
          </div>
          {searchResults.length === 0 ? (
            <p className="empty-state">No Items match that search.</p>
          ) : (
            <div className="search-results">
              {searchResults.map((view) => (
                <SearchResult key={view.item.id} view={view} />
              ))}
            </div>
          )}
        </section>
      )}

      <section className="home-board" aria-labelledby="board-heading">
        <div className="section-heading board-heading">
          <div>
            <p className="eyebrow">Your work</p>
            <h2 id="board-heading">
              {contextFilterId
                ? contexts.find((context) => context.id === contextFilterId)?.name
                : "All Items"}
            </h2>
          </div>
          <span className="key-hint">Statuses are yours to move</span>
        </div>
        {isLoading || !home ? (
          <p className="empty-state">Loading your home view…</p>
        ) : (
          <>
            {home.attention_entries.length > 0 && (
              <section className="attention-entries" aria-labelledby="attention-heading">
                <div className="column-heading">
                  <div>
                    <h3 id="attention-heading">Review changes</h3>
                    <span className="column-hint">
                      Explicitly mark a Link reviewed when you have handled it.
                    </span>
                  </div>
                  <span className="column-count">{home.attention_entries.length}</span>
                </div>
                <div className="attention-entry-list">
                  {home.attention_entries.map((entry) => (
                    <AttentionEntryCard
                      key={`${entry.kind}-${entry.link_id}-${entry.reminder_id ?? ""}-${entry.run_id ?? ""}`}
                      entry={entry}
                      item={allItems.find((candidate) => candidate.item.id === entry.item_id)}
                      onMarkedReviewed={updateHomeAfterEdit}
                    />
                  ))}
                </div>
              </section>
            )}
            <div className="home-columns">
            <HomeColumn
              title="Needs Attention"
              hint="Unstarted or due"
              items={home.needs_attention}
              allItems={allItems}
              repositories={repositories}
              machines={machines}
              onChanged={updateHomeAfterEdit}
              onOpenTerminal={(worksetId, pane) => setTerminalRequest({ worksetId, pane })}
            />
            <HomeColumn
              title="Running"
              hint="Active"
              items={home.running}
              allItems={allItems}
              repositories={repositories}
              machines={machines}
              onChanged={updateHomeAfterEdit}
              onOpenTerminal={(worksetId, pane) => setTerminalRequest({ worksetId, pane })}
            />
            <HomeColumn
              title="Waiting"
              hint="Waiting"
              items={home.waiting}
              allItems={allItems}
              repositories={repositories}
              machines={machines}
              onChanged={updateHomeAfterEdit}
              onOpenTerminal={(worksetId, pane) => setTerminalRequest({ worksetId, pane })}
            />
            <HomeColumn
              title="Due"
              hint="Reminder reached"
              items={home.due}
              allItems={allItems}
              repositories={repositories}
              machines={machines}
              onChanged={updateHomeAfterEdit}
              onOpenTerminal={(worksetId, pane) => setTerminalRequest({ worksetId, pane })}
            />
            <HomeColumn
              title="Completed"
              hint="Done"
              items={home.completed}
              allItems={allItems}
              repositories={repositories}
              machines={machines}
              onChanged={updateHomeAfterEdit}
              onOpenTerminal={(worksetId, pane) => setTerminalRequest({ worksetId, pane })}
            />
            </div>
          </>
        )}
      </section>

      <section className="capture-card" aria-labelledby="capture-heading">
        <div className="section-heading">
          <div>
            <p className="eyebrow">New Item</p>
            <h2 id="capture-heading">Give the next decision a place</h2>
          </div>
          <span className="key-hint">Title + Context + Project</span>
        </div>
        <form className="capture-form" onSubmit={handleCreateItem}>
          <label>
            <span>Title</span>
            <input
              autoFocus
              value={title}
              onChange={(event) => setTitle(event.target.value)}
              placeholder="Investigate slow invoice import"
              disabled={isSaving}
            />
          </label>
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
          <button
            type="submit"
            disabled={
              isSaving || !title.trim() || !captureContextId || !captureProjectId
            }
          >
            {isSaving ? "Saving…" : "Add Item"}
          </button>
        </form>
      </section>

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
                  <span>{context.name}</span>
                  <span className="entity-meta">
                    {projects.filter((project) => project.context_id === context.id).length}{" "}
                    Projects
                  </span>
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
                      <span>{repository.name}</span>
                      <span className="entity-meta">{repository.remote_url}</span>
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
                        <button
                          type="button"
                          className="text-button"
                          onClick={() => void handleCheckMachine(machine.id)}
                        >
                          Check
                        </button>
                      </span>
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
      </section>
    </main>
  );
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
      void invoke("terminal_input", {
        terminalId,
        input: Array.from(new TextEncoder().encode(data)),
      }).catch((inputError) => {
        if (!disposed) setTerminalError(errorMessage(inputError));
      });
    });
    const resizeTerminal = () => {
      fitAddon.fit();
      if (!attachedRef.current || terminal.cols < 1 || terminal.rows < 1) return;
      void invoke("terminal_resize", {
        terminalId,
        columns: terminal.cols,
        rows: terminal.rows,
      }).catch((resizeError) => {
        if (!disposed) setTerminalError(errorMessage(resizeError));
      });
    };
    const resizeObserver = new ResizeObserver(resizeTerminal);
    resizeObserver.observe(container);
    const unlisteners: UnlistenFn[] = [];

    const attachPane = async (pane: PaneTab) => {
      activePaneRef.current = pane;
      setActivePane(pane);
      attachedRef.current = false;
      setStatus(`Attaching ${pane.label}…`);
      setTerminalError(undefined);
      const attachment = await invoke<TerminalAttachment>("open_terminal", {
        worksetId,
        terminalId,
        sessionName: pane.sessionName,
        paneId: pane.paneId,
      });
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
        await listen<TerminalOutputEvent>("terminal-output", (event) => {
          const payload = event.payload;
          if (
            payload.terminalId === terminalId &&
            payload.paneId === activePaneRef.current.paneId
          ) {
            terminal.write(Uint8Array.from(payload.data));
          }
        }),
        await listen<TerminalExitEvent>("terminal-exit", (event) => {
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
      void invoke("close_terminal", { terminalId }).catch(() => undefined);
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
      {terminalError && <p className="error-message">{terminalError}</p>}
    </section>
  );
}

function HomeColumn({
  title,
  hint,
  items,
  allItems,
  repositories,
  machines,
  onChanged,
  onOpenTerminal,
}: {
  title: string;
  hint: string;
  items: ItemView[];
  allItems: ItemView[];
  repositories: Repository[];
  machines: Machine[];
  onChanged: () => Promise<void>;
  onOpenTerminal: (worksetId: number, pane: PaneTab) => void;
}) {
  return (
    <section className="home-column" aria-labelledby={`${title}-heading`}>
      <div className="column-heading">
        <div>
          <h3 id={`${title}-heading`}>{title}</h3>
          <span className="column-hint">{hint}</span>
        </div>
        <span className="column-count">{items.length}</span>
      </div>
      {items.length === 0 ? (
        <p className="column-empty">Nothing here.</p>
      ) : (
        <div className="column-items">
          {items.map((view) => (
            <ItemCard
              key={view.item.id}
              view={view}
              allItems={allItems}
              repositories={repositories}
              machines={machines}
              onChanged={onChanged}
              onOpenTerminal={onOpenTerminal}
            />
          ))}
        </div>
      )}
    </section>
  );
}

function ItemCard({
  view,
  allItems,
  repositories,
  machines,
  onChanged,
  onOpenTerminal,
}: {
  view: ItemView;
  allItems: ItemView[];
  repositories: Repository[];
  machines: Machine[];
  onChanged: () => Promise<void>;
  onOpenTerminal: (worksetId: number, pane: PaneTab) => void;
}) {
  const [notes, setNotes] = useState(view.item.notes);
  const [reminderAt, setReminderAt] = useState("");
  const [relationKind, setRelationKind] = useState<ItemRelationKind>("Blocks");
  const [targetItemId, setTargetItemId] = useState<number>();
  const [externalUrl, setExternalUrl] = useState("");
  const [isIssuePreviewOpen, setIsIssuePreviewOpen] = useState(false);
  const [issueRepository, setIssueRepository] = useState("");
  const [issueTitle, setIssueTitle] = useState(view.item.title);
  const [issueBody, setIssueBody] = useState(view.item.notes);
  const [worksetRoot, setWorksetRoot] = useState("");
  const [worksetBranch, setWorksetBranch] = useState("");
  const [attachWorksetRoot, setAttachWorksetRoot] = useState("");
  const [selectedRepositoryIds, setSelectedRepositoryIds] = useState<number[]>([]);
  const [worksetBranchOverrides, setWorksetBranchOverrides] = useState<
    Record<number, string>
  >({});
  const [worksetBaseBranchOverrides, setWorksetBaseBranchOverrides] = useState<
    Record<number, string>
  >({});
  const [additionalRepositoryId, setAdditionalRepositoryId] = useState<number>();
  const [additionalBranchOverride, setAdditionalBranchOverride] = useState("");
  const [additionalBaseBranchOverride, setAdditionalBaseBranchOverride] =
    useState("");
  const [removalReport, setRemovalReport] = useState<WorksetRemovalReport>();
  const [runPreviewWorksetId, setRunPreviewWorksetId] = useState<number>();
  const [runMachineId, setRunMachineId] = useState<number>();
  const [runAgent, setRunAgent] = useState<AgentKind>("claude");
  const [runProfile, setRunProfile] = useState<ExecutionProfile>("implement");
  const [includeRunObjective, setIncludeRunObjective] = useState(true);
  const [includeRunNotes, setIncludeRunNotes] = useState(false);
  const [selectedRunExternalObjectIds, setSelectedRunExternalObjectIds] =
    useState<number[]>([]);
  const [runCustomPrompt, setRunCustomPrompt] = useState("");
  const [runPrompt, setRunPrompt] = useState("");
  const [runPromptNeedsCompose, setRunPromptNeedsCompose] = useState(false);
  const [isSaving, setIsSaving] = useState(false);

  const itemRepositories = repositories.filter(
    (repository) => repository.project_id === view.item.project_id,
  );

  useEffect(() => {
    setNotes(view.item.notes);
  }, [view.item.notes]);

  async function saveItem(update: () => Promise<unknown>) {
    setIsSaving(true);
    try {
      await update();
      await onChanged();
    } catch (saveError) {
      window.alert(errorMessage(saveError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleRelation(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!targetItemId) return;
    await saveItem(() =>
      invoke("set_item_relation", {
        fromItemId: view.item.id,
        toItemId: targetItemId,
        kind: relationKind,
      }),
    );
    setTargetItemId(undefined);
  }

  async function handleExternalLink(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!externalUrl.trim()) return;
    setIsSaving(true);
    try {
      const result = await invoke<ExternalLinkAction>("link_external_object", {
        itemId: view.item.id,
        url: externalUrl,
      });
      setExternalUrl("");
      await onChanged();
      if (result.warning) {
        window.alert(result.warning);
      }
    } catch (linkError) {
      window.alert(errorMessage(linkError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleCreateWorkset(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!worksetRoot.trim() || !worksetBranch.trim() || selectedRepositoryIds.length === 0) {
      return;
    }
    await saveItem(async () => {
      const selected: WorksetRepositoryInput[] = selectedRepositoryIds.map(
        (repositoryId) => ({
          repositoryId,
          branchOverride: worksetBranchOverrides[repositoryId]?.trim() || null,
          baseBranchOverride:
            worksetBaseBranchOverrides[repositoryId]?.trim() || null,
        }),
      );
      await invoke<Workset>("create_workset", {
        itemId: view.item.id,
        rootDirectory: worksetRoot,
        branch: worksetBranch,
        repositories: selected,
      });
      setWorksetRoot("");
      setWorksetBranch("");
      setSelectedRepositoryIds([]);
      setWorksetBranchOverrides({});
      setWorksetBaseBranchOverrides({});
    });
  }

  async function handleAttachWorkset(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!attachWorksetRoot.trim()) return;
    await saveItem(async () => {
      await invoke<Workset>("attach_workset", {
        itemId: view.item.id,
        rootDirectory: attachWorksetRoot,
      });
      setAttachWorksetRoot("");
    });
  }

  async function handleAddRepositoryToWorkset(worksetId: number) {
    if (!additionalRepositoryId) return;
    await saveItem(async () => {
      await invoke<Workset>("add_repository_to_workset", {
        worksetId,
        repositoryId: additionalRepositoryId,
        branchOverride: additionalBranchOverride.trim() || null,
        baseBranchOverride: additionalBaseBranchOverride.trim() || null,
      });
      setAdditionalRepositoryId(undefined);
      setAdditionalBranchOverride("");
      setAdditionalBaseBranchOverride("");
    });
  }

  async function handleSetWorksetArchived(worksetId: number, archived: boolean) {
    await saveItem(() =>
      invoke("set_workset_archived", { worksetId, archived }),
    );
    if (removalReport?.workset_id === worksetId) {
      setRemovalReport(undefined);
    }
  }

  async function handlePrepareRemoval(worksetId: number) {
    setIsSaving(true);
    try {
      const report = await invoke<WorksetRemovalReport>("prepare_workset_removal", {
        worksetId,
      });
      setRemovalReport(report);
    } catch (reportError) {
      window.alert(errorMessage(reportError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleRemoveWorkset(worksetId: number) {
    if (removalReport?.workset_id !== worksetId) return;
    if (!window.confirm("Remove this Workset and its directory from disk?")) return;
    await saveItem(async () => {
      await invoke("remove_workset", { worksetId, confirmed: true });
      setRemovalReport(undefined);
    });
  }

  function runPromptSelection(): RunPromptSelection {
    return {
      includeObjective: includeRunObjective,
      includeNotes: includeRunNotes,
      externalObjectIds: selectedRunExternalObjectIds,
    };
  }

  async function composeRunPromptPreview() {
    if (!runPreviewWorksetId) return;
    setIsSaving(true);
    try {
      const composed = await invoke<string>("compose_run_prompt", {
        itemId: view.item.id,
        executionProfile: runProfile,
        promptSelection: runPromptSelection(),
        customPrompt: runProfile === "custom" ? runCustomPrompt : null,
      });
      setRunPrompt(composed);
      setRunPromptNeedsCompose(false);
    } catch (composeError) {
      window.alert(errorMessage(composeError));
    } finally {
      setIsSaving(false);
    }
  }

  async function openRunPreview(workset: Workset) {
    setRunPreviewWorksetId(workset.id);
    setRunMachineId(undefined);
    setRunAgent("claude");
    setRunProfile("implement");
    setIncludeRunObjective(true);
    setIncludeRunNotes(Boolean(view.item.notes.trim()));
    setSelectedRunExternalObjectIds([]);
    setRunCustomPrompt("");
    setIsSaving(true);
    try {
      const composed = await invoke<string>("compose_run_prompt", {
        itemId: view.item.id,
        executionProfile: "implement",
        promptSelection: {
          includeObjective: true,
          includeNotes: Boolean(view.item.notes.trim()),
          externalObjectIds: [],
        },
        customPrompt: null,
      });
      setRunPrompt(composed);
      setRunPromptNeedsCompose(false);
    } catch (composeError) {
      setRunPreviewWorksetId(undefined);
      window.alert(errorMessage(composeError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleStartRun(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!runPreviewWorksetId || !runPrompt.trim() || runPromptNeedsCompose) return;
    await saveItem(async () => {
      await invoke<Run>("start_run", {
        itemId: view.item.id,
        worksetId: runPreviewWorksetId,
        machineId: runMachineId ?? null,
        agent: runAgent,
        executionProfile: runProfile,
        prompt: runPrompt,
        promptSelection: runPromptSelection(),
      });
      setRunPreviewWorksetId(undefined);
      setRunPrompt("");
    });
  }

  async function refreshExternalObject(externalObjectId: number) {
    setIsSaving(true);
    try {
      await invoke("refresh_external_object", { externalObjectId });
      await onChanged();
    } catch (refreshError) {
      window.alert(errorMessage(refreshError));
    } finally {
      setIsSaving(false);
    }
  }

  function openIssuePreview() {
    setIssueTitle(view.item.title);
    setIssueBody(view.item.notes);
    setIsIssuePreviewOpen(true);
  }

  async function handleCreateIssue(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!issueRepository.trim() || !issueTitle.trim()) return;
    setIsSaving(true);
    try {
      const result = await invoke<ExternalLinkAction>("create_github_issue", {
        itemId: view.item.id,
        repository: issueRepository,
        title: issueTitle,
        body: issueBody,
      });
      setIsIssuePreviewOpen(false);
      setIssueRepository("");
      await onChanged();
      if (result.warning) {
        window.alert(result.warning);
      }
    } catch (createError) {
      window.alert(errorMessage(createError));
    } finally {
      setIsSaving(false);
    }
  }

  const visibleTargets = allItems.filter(
    (candidate) =>
      candidate.item.id !== view.item.id &&
      candidate.context_name === view.context_name,
  );
  const itemMachines = machines.filter(
    (machine) => machine.context_id === view.context_id,
  );

  function renderRemovalFinding(
    label: string,
    values: string[],
    emptyMessage: string,
  ) {
    return (
      <p>
        {values.length === 0 ? emptyMessage : `${label}: ${values.join("; ")}`}
      </p>
    );
  }

  function renderWorksetCard(workset: Workset, archived: boolean) {
    const selectedIds = new Set(
      workset.repositories.map((selected) => selected.repository_id),
    );
    const availableRepositories = itemRepositories.filter(
      (repository) => !selectedIds.has(repository.id),
    );
    const report = removalReport?.workset_id === workset.id ? removalReport : undefined;

    return (
      <article className={`workset-card${archived ? " archived-workset-card" : ""}`} key={workset.id}>
        <div className="workset-heading">
          <div>
            <strong>{workset.branch}</strong>
            {archived && <span className="archived-label">Archived</span>}
          </div>
          <span>{workset.root_directory}</span>
        </div>
        <div className="workset-repositories">
          {workset.repositories.map((selected) => (
            <span className="relationship-chip" key={selected.repository_id}>
              <strong>{repositoryName(repositories, selected.repository_id)}</strong>
              <span>
                {selected.current_branch} · {selected.is_dirty ? "uncommitted changes" : "clean"}
              </span>
            </span>
          ))}
        </div>
        <div className="workset-actions">
          {!archived && (
            <button
              type="button"
              disabled={isSaving}
              onClick={() => void openRunPreview(workset)}
            >
              Start Run
            </button>
          )}
          <button
            type="button"
            className="secondary-button"
            disabled={isSaving}
            onClick={() => void handleSetWorksetArchived(workset.id, !archived)}
          >
            {archived ? "Restore" : "Archive"}
          </button>
          <button
            type="button"
            className="secondary-button"
            disabled={isSaving}
            onClick={() => void handlePrepareRemoval(workset.id)}
          >
            Review removal
          </button>
        </div>
        {report && (
          <div className="removal-report" role="alert">
            <strong>Removal safety report</strong>
            <p>{report.root_directory} will be removed from disk.</p>
            {report.repositories.map((repository) => (
              <div className="removal-repository" key={repository.repository_id}>
                <strong>{repository.name}</strong>
                <span>{repository.current_branch} · {repository.path}</span>
                {repository.unpushed_commits_unknown ? (
                  <p>Unpushed commits could not be verified: no upstream branch is configured.</p>
                ) : (
                  renderRemovalFinding(
                    "Unpushed commits",
                    repository.unpushed_commits,
                    "No unpushed commits.",
                  )
                )}
                {renderRemovalFinding(
                  "Uncommitted changes",
                  repository.uncommitted_changes,
                  "No uncommitted changes.",
                )}
              </div>
            ))}
            <div className="workset-actions">
              <button
                type="button"
                disabled={isSaving}
                onClick={() => void handleRemoveWorkset(workset.id)}
              >
                Confirm and remove
              </button>
              <button
                type="button"
                className="text-button"
                disabled={isSaving}
                onClick={() => setRemovalReport(undefined)}
              >
                Keep Workset
              </button>
            </div>
          </div>
        )}
        {runPreviewWorksetId === workset.id && (
          <form className="run-preview" onSubmit={handleStartRun}>
            <div className="run-preview-heading">
              <div>
                <strong>Confirm Run</strong>
                <p>
                  Context: {view.context_name} · Project: {view.project_name} · Workset: {workset.branch}
                </p>
              </div>
              <span className="run-machine">
                Machine:{" "}
                {itemMachines.find((machine) => machine.id === runMachineId)?.name ??
                  "Local Mac"}
              </span>
            </div>
            <p className="run-working-directory">
              Working directory: <code>{workset.root_directory}</code>
            </p>
            <div className="run-options">
              <label>
                <span>Machine</span>
                <select
                  value={runMachineId ?? ""}
                  onChange={(event) =>
                    setRunMachineId(Number(event.target.value) || undefined)
                  }
                  disabled={isSaving}
                >
                  <option value="">Local Mac (default)</option>
                  {itemMachines.map((machine) => (
                    <option value={machine.id} key={machine.id}>
                      {machine.name} · {machine.last_observed}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                <span>Agent</span>
                <select
                  value={runAgent}
                  onChange={(event) => setRunAgent(event.target.value as AgentKind)}
                  disabled={isSaving}
                >
                  <option value="claude">Claude Code</option>
                  <option value="codex">Codex</option>
                </select>
              </label>
              <label>
                <span>Execution Profile</span>
                <select
                  value={runProfile}
                  onChange={(event) => {
                    setRunProfile(event.target.value as ExecutionProfile);
                    setRunPromptNeedsCompose(true);
                  }}
                  disabled={isSaving}
                >
                  <option value="investigate">Investigate</option>
                  <option value="implement">Implement</option>
                  <option value="review">Review</option>
                  <option value="custom">Custom prompt</option>
                </select>
              </label>
            </div>
            {runProfile === "custom" && (
              <label>
                <span>Custom prompt source</span>
                <textarea
                  value={runCustomPrompt}
                  onChange={(event) => {
                    setRunCustomPrompt(event.target.value);
                    setRunPromptNeedsCompose(true);
                  }}
                  rows={3}
                  placeholder="Tell the agent exactly what to do"
                  disabled={isSaving}
                />
              </label>
            )}
            <div className="run-content-selection">
              <span className="relationship-label">Include explicitly selected content</span>
              <label>
                <input
                  type="checkbox"
                  checked={includeRunObjective}
                  onChange={(event) => {
                    setIncludeRunObjective(event.target.checked);
                    setRunPromptNeedsCompose(true);
                  }}
                  disabled={isSaving}
                />
                Item objective
              </label>
              {view.item.notes.trim() && (
                <label>
                  <input
                    type="checkbox"
                    checked={includeRunNotes}
                    onChange={(event) => {
                      setIncludeRunNotes(event.target.checked);
                      setRunPromptNeedsCompose(true);
                    }}
                    disabled={isSaving}
                  />
                  Item notes
                </label>
              )}
              {view.links.map((link) => (
                <label key={link.object.id}>
                  <input
                    type="checkbox"
                    checked={selectedRunExternalObjectIds.includes(link.object.id)}
                    onChange={(event) => {
                      setSelectedRunExternalObjectIds((current) =>
                        event.target.checked
                          ? [...current, link.object.id]
                          : current.filter((id) => id !== link.object.id),
                      );
                      setRunPromptNeedsCompose(true);
                    }}
                    disabled={isSaving}
                  />
                  {link.snapshot?.title ?? link.object.canonical_url}
                </label>
              ))}
            </div>
            <label>
              <span>Editable composed prompt</span>
              <textarea
                value={runPrompt}
                onChange={(event) => setRunPrompt(event.target.value)}
                rows={7}
                disabled={isSaving}
              />
            </label>
            <div className="run-preview-actions">
              <button
                type="button"
                className="secondary-button"
                disabled={isSaving}
                onClick={() => void composeRunPromptPreview()}
              >
                Compose from selection
              </button>
              <button
                type="submit"
                disabled={isSaving || !runPrompt.trim() || runPromptNeedsCompose}
              >
                {isSaving
                  ? "Starting…"
                  : runPromptNeedsCompose
                    ? "Compose before starting"
                    : "Confirm and start Run"}
              </button>
              <button
                type="button"
                className="text-button"
                disabled={isSaving}
                onClick={() => setRunPreviewWorksetId(undefined)}
              >
                Cancel
              </button>
            </div>
          </form>
        )}
        {!archived && availableRepositories.length > 0 && (
          <div className="workset-add-repository">
            <select
              aria-label={`Repository to add to Workset ${workset.id}`}
              value={additionalRepositoryId ?? ""}
              onChange={(event) =>
                setAdditionalRepositoryId(Number(event.target.value) || undefined)
              }
              disabled={isSaving}
            >
              <option value="">Add a Repository</option>
              {availableRepositories.map((repository) => (
                <option value={repository.id} key={repository.id}>
                  {repository.name}
                </option>
              ))}
            </select>
            <input
              aria-label="Added Repository branch override"
              value={additionalBranchOverride}
              onChange={(event) => setAdditionalBranchOverride(event.target.value)}
              placeholder="Branch override (optional)"
              disabled={isSaving}
            />
            <input
              aria-label="Added Repository base branch override"
              value={additionalBaseBranchOverride}
              onChange={(event) =>
                setAdditionalBaseBranchOverride(event.target.value)
              }
              placeholder="Base branch override (optional)"
              disabled={isSaving}
            />
            <button
              type="button"
              className="secondary-button"
              disabled={isSaving || !additionalRepositoryId}
              onClick={() => void handleAddRepositoryToWorkset(workset.id)}
            >
              Add
            </button>
          </div>
        )}
      </article>
    );
  }

  return (
    <article className="item-card">
      <div className="item-card-heading">
        <span className="item-identifier">{view.item.human_identifier}</span>
        <select
          aria-label={`Status for ${view.item.human_identifier}`}
          value={view.item.status}
          onChange={(event) =>
            void saveItem(() =>
              invoke("set_item_status", {
                itemId: view.item.id,
                status: event.target.value,
              }),
            )
          }
          disabled={isSaving}
        >
          {itemStatuses.map((status) => (
            <option value={status} key={status}>
              {status}
            </option>
          ))}
        </select>
      </div>
      <h4>{view.item.title}</h4>
      <p className="item-context">
        {view.context_name} <span>·</span> {view.project_name}
      </p>
      <label className="card-notes">
        <span>Notes</span>
        <textarea
          value={notes}
          onChange={(event) => setNotes(event.target.value)}
          placeholder="Add a useful handoff note"
          rows={3}
          disabled={isSaving}
        />
      </label>
      <div className="card-actions">
        <button
          type="button"
          className="secondary-button"
          disabled={isSaving || notes === view.item.notes}
          onClick={() =>
            void saveItem(() =>
              invoke("set_item_notes", { itemId: view.item.id, notes }),
            )
          }
        >
          Save notes
        </button>
        <label className="reminder-field">
          <span>New reminder</span>
          <input
            type="datetime-local"
            value={reminderAt}
            onChange={(event) => setReminderAt(event.target.value)}
            disabled={isSaving}
          />
        </label>
        <button
          type="button"
          className="secondary-button"
          disabled={isSaving || !reminderAt}
          onClick={() =>
            void saveItem(() =>
              invoke("add_item_reminder", {
                itemId: view.item.id,
                remindAt: reminderAt,
              }),
            )
          }
        >
          Add reminder
        </button>
      </div>
      {view.item.reminders.length > 0 && (
        <div className="reminder-list">
          <span className="relationship-label">Reminders</span>
          {view.item.reminders.map((reminder) => (
            <span className="reminder-chip" key={reminder.id}>
              {reminder.remind_at}
              <button
                type="button"
                className="text-button"
                disabled={isSaving}
                onClick={() =>
                  void saveItem(() =>
                    invoke("remove_item_reminder", {
                      itemId: view.item.id,
                      reminderId: reminder.id,
                    }),
                  )
                }
              >
                Remove
              </button>
            </span>
          ))}
        </div>
      )}
      {view.runs.length > 0 && (
        <div className="run-history">
          <span className="relationship-label">Run history</span>
          {view.runs.map((run) => (
            <article className="run-history-card" key={run.id}>
              <div>
                <strong>
                  Run #{run.id} · {run.agent === "claude" ? "Claude Code" : "Codex"}
                </strong>
                <span>
                  {run.execution_profile} ·{" "}
                  {machines.find((machine) => machine.id === run.machine_id)?.name ??
                    "Machine #" + run.machine_id}{" "}
                  · {runStateLabel(run.state)}
                </span>
              </div>
              <code>{run.working_directory}</code>
              <span>
                Session {run.session_name} · Pane {run.pane_id}
              </span>
              {findWorkset(view, run.workset_id) && (
                <div className="run-history-actions">
                  <button
                    type="button"
                    className="secondary-button"
                    onClick={() =>
                      onOpenTerminal(run.workset_id, paneTabForRun(run))
                    }
                  >
                    Open embedded terminal
                  </button>
                  <button
                    type="button"
                    className="secondary-button"
                    disabled={isSaving}
                    onClick={() =>
                      void saveItem(() =>
                        invoke("open_external_terminal", { runId: run.id }),
                      )
                    }
                  >
                    Open in Terminal
                  </button>
                </div>
              )}
            </article>
          ))}
        </div>
      )}
      <div className="worksets">
        <span className="relationship-label">Worksets</span>
        {view.worksets.map((workset) => renderWorksetCard(workset, false))}
        {view.archived_worksets.length > 0 && (
          <div className="archived-worksets">
            <span className="relationship-label">Archived Worksets</span>
            {view.archived_worksets.map((workset) => renderWorksetCard(workset, true))}
          </div>
        )}
        <form className="workset-form" onSubmit={handleCreateWorkset}>
          <strong>Create a Workset</strong>
          <label>
            <span>Root directory</span>
            <input
              value={worksetRoot}
              onChange={(event) => setWorksetRoot(event.target.value)}
              placeholder="/Users/me/worksets/PLAT-847"
              disabled={isSaving}
            />
          </label>
          <label>
            <span>Logical branch</span>
            <input
              value={worksetBranch}
              onChange={(event) => setWorksetBranch(event.target.value)}
              placeholder="feature/PLAT-847"
              disabled={isSaving}
            />
          </label>
          <span className="relationship-label">Select repositories</span>
          {itemRepositories.length === 0 ? (
            <span className="relationship-empty">
              Register a Repository under this Project first.
            </span>
          ) : (
            <div className="workset-selection-list">
              {itemRepositories.map((repository) => {
                const selected = selectedRepositoryIds.includes(repository.id);
                return (
                  <div className="workset-selection" key={repository.id}>
                    <label>
                      <input
                        type="checkbox"
                        checked={selected}
                        onChange={(event) =>
                          setSelectedRepositoryIds((current) =>
                            event.target.checked
                              ? [...current, repository.id]
                              : current.filter((id) => id !== repository.id),
                          )
                        }
                        disabled={isSaving}
                      />
                      {repository.name}
                    </label>
                    {selected && (
                      <div className="workset-overrides">
                        <input
                          aria-label={`${repository.name} branch override`}
                          value={worksetBranchOverrides[repository.id] ?? ""}
                          onChange={(event) =>
                            setWorksetBranchOverrides((current) => ({
                              ...current,
                              [repository.id]: event.target.value,
                            }))
                          }
                          placeholder="Branch override (optional)"
                          disabled={isSaving}
                        />
                        <input
                          aria-label={`${repository.name} base branch override`}
                          value={worksetBaseBranchOverrides[repository.id] ?? ""}
                          onChange={(event) =>
                            setWorksetBaseBranchOverrides((current) => ({
                              ...current,
                              [repository.id]: event.target.value,
                            }))
                          }
                          placeholder="Base branch override (optional)"
                          disabled={isSaving}
                        />
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          )}
          <button
            type="submit"
            disabled={
              isSaving ||
              !worksetRoot.trim() ||
              !worksetBranch.trim() ||
              selectedRepositoryIds.length === 0
            }
          >
            {isSaving ? "Checking out…" : "Create Workset"}
          </button>
        </form>
        <form className="workset-form" onSubmit={handleAttachWorkset}>
          <strong>Attach an Existing Workset</strong>
          <label>
            <span>Existing root directory</span>
            <input
              value={attachWorksetRoot}
              onChange={(event) => setAttachWorksetRoot(event.target.value)}
              placeholder="/Users/me/worksets/PLAT-847"
              disabled={isSaving}
            />
          </label>
          <p className="workset-help">
            Inspects direct child repositories, branches, and uncommitted changes without modifying Git.
          </p>
          <button type="submit" disabled={isSaving || !attachWorksetRoot.trim()}>
            {isSaving ? "Inspecting…" : "Attach Workset"}
          </button>
        </form>
      </div>
      <div className="external-links">
        <span className="relationship-label">External Links</span>
        {view.links.map((externalLink) => (
          <ExternalLinkCard
            key={externalLink.link.id}
            externalLink={externalLink}
            isSaving={isSaving}
            onRefresh={() => refreshExternalObject(externalLink.object.id)}
            onSavePolicy={(policy) =>
              saveItem(() =>
                invoke("set_link_attention_policy", {
                  linkId: externalLink.link.id,
                  policy,
                }),
              )
            }
            onMarkReviewed={() =>
              saveItem(() =>
                invoke("mark_link_reviewed", {
                  linkId: externalLink.link.id,
                }),
              )
            }
            onSaveWatchUntil={(watchUntil) =>
              saveItem(() =>
                invoke("set_link_watch_until", {
                  linkId: externalLink.link.id,
                  watchUntil,
                }),
              )
            }
            onSaveReviewAt={(reviewAt) =>
              saveItem(() =>
                invoke("set_link_review_at", {
                  linkId: externalLink.link.id,
                  reviewAt,
                }),
              )
            }
            onClearReviewAt={() =>
              saveItem(() =>
                invoke("clear_link_review_at", {
                  linkId: externalLink.link.id,
                }),
              )
            }
            onAddComment={(body) =>
              saveItem(() =>
                invoke("add_external_comment", {
                  linkId: externalLink.link.id,
                  body,
                }),
              )
            }
          />
        ))}
        <form className="external-link-form" onSubmit={handleExternalLink}>
          <input
            aria-label={`External URL for ${view.item.human_identifier}`}
            value={externalUrl}
            onChange={(event) => setExternalUrl(event.target.value)}
            placeholder="Paste a GitHub issue, pull request, or URL"
            disabled={isSaving}
          />
          <button
            type="submit"
            className="secondary-button"
            disabled={isSaving || !externalUrl.trim()}
          >
            Add link
          </button>
        </form>
        {!isIssuePreviewOpen ? (
          <button
            type="button"
            className="secondary-button"
            disabled={isSaving}
            onClick={openIssuePreview}
          >
            Create GitHub Issue
          </button>
        ) : (
          <form className="issue-preview" onSubmit={handleCreateIssue}>
            <div>
              <strong>Preview GitHub Issue</strong>
              <p>
                Nothing is sent until you confirm. The existing Item will remain
                unchanged and the created Issue will be linked to it.
              </p>
            </div>
            <label>
              <span>Repository</span>
              <input
                value={issueRepository}
                onChange={(event) => setIssueRepository(event.target.value)}
                placeholder="owner/repository"
                disabled={isSaving}
              />
            </label>
            <label>
              <span>Public title</span>
              <input
                value={issueTitle}
                onChange={(event) => setIssueTitle(event.target.value)}
                disabled={isSaving}
              />
            </label>
            <label>
              <span>Public body</span>
              <textarea
                value={issueBody}
                onChange={(event) => setIssueBody(event.target.value)}
                rows={4}
                placeholder="Optional public context"
                disabled={isSaving}
              />
            </label>
            <div className="issue-preview-actions">
              <button
                type="submit"
                disabled={isSaving || !issueRepository.trim() || !issueTitle.trim()}
              >
                {isSaving ? "Creating…" : "Confirm and create Issue"}
              </button>
              <button
                type="button"
                className="text-button"
                disabled={isSaving}
                onClick={() => setIsIssuePreviewOpen(false)}
              >
                Cancel
              </button>
            </div>
          </form>
        )}
      </div>
      <div className="relationship-list">
        <span className="relationship-label">Relationships</span>
        {view.relationships.length === 0 ? (
          <span className="relationship-empty">None yet</span>
        ) : (
          view.relationships.map((relation) => {
            const otherId =
              relation.from_item_id === view.item.id
                ? relation.to_item_id
                : relation.from_item_id;
            const other = allItems.find((candidate) => candidate.item.id === otherId);
            return (
              <span
                className="relationship-chip"
                key={`${relation.from_item_id}-${relation.to_item_id}-${relation.kind}`}
              >
                {relationshipLabel(relation, view.item.id)}{" "}
                {other?.item.human_identifier ?? `MC-${otherId}`}
              </span>
            );
          })
        )}
      </div>
      {visibleTargets.length > 0 && (
        <form className="relationship-form" onSubmit={handleRelation}>
          <select
            aria-label="Relationship kind"
            value={relationKind}
            onChange={(event) => setRelationKind(event.target.value as ItemRelationKind)}
            disabled={isSaving}
          >
            {relationKinds.map((kind) => (
              <option value={kind} key={kind}>
                {relationKindLabel(kind)}
              </option>
            ))}
          </select>
          <select
            aria-label="Related Item"
            value={targetItemId ?? ""}
            onChange={(event) => setTargetItemId(Number(event.target.value))}
            disabled={isSaving}
          >
            <option value="">Choose an Item</option>
            {visibleTargets.map((candidate) => (
              <option value={candidate.item.id} key={candidate.item.id}>
                {candidate.item.human_identifier} · {candidate.item.title}
              </option>
            ))}
          </select>
          <button
            type="submit"
            className="secondary-button"
            disabled={isSaving || !targetItemId}
          >
            Link
          </button>
        </form>
      )}
    </article>
  );
}

function ExternalLinkCard({
  externalLink,
  isSaving,
  onRefresh,
  onSavePolicy,
  onMarkReviewed,
  onSaveWatchUntil,
  onSaveReviewAt,
  onClearReviewAt,
  onAddComment,
}: {
  externalLink: ExternalLinkView;
  isSaving: boolean;
  onRefresh: () => Promise<void>;
  onSavePolicy: (policy: ExternalChangePolicy | null) => Promise<void>;
  onMarkReviewed: () => Promise<void>;
  onSaveWatchUntil: (watchUntil: string | null) => Promise<void>;
  onSaveReviewAt: (reviewAt: string | null) => Promise<void>;
  onClearReviewAt: () => Promise<void>;
  onAddComment: (body: string) => Promise<void>;
}) {
  const { object, snapshot } = externalLink;
  const [policy, setPolicy] = useState(externalLink.attention_policy);
  const [watchUntil, setWatchUntil] = useState(externalLink.link.watch_until ?? "");
  const [reviewAt, setReviewAt] = useState(externalLink.link.review_at ?? "");
  const [comment, setComment] = useState("");
  const [isCommenting, setIsCommenting] = useState(false);
  const reviewDateReached =
    externalLink.link.review_at !== null &&
    externalLink.link.review_at <= currentMinute();

  useEffect(() => {
    setPolicy(externalLink.attention_policy);
    setWatchUntil(externalLink.link.watch_until ?? "");
    setReviewAt(externalLink.link.review_at ?? "");
  }, [
    externalLink.attention_policy,
    externalLink.link.review_at,
    externalLink.link.watch_until,
  ]);

  async function handleComment(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!comment.trim()) return;
    setIsCommenting(true);
    try {
      await onAddComment(comment);
      setComment("");
    } catch (commentError) {
      window.alert(errorMessage(commentError));
    } finally {
      setIsCommenting(false);
    }
  }

  return (
    <article className="external-link-card">
      <div className="external-link-heading">
        <div>
          <strong>{snapshot?.title ?? object.canonical_url}</strong>
          <span className="external-link-kind">
            {externalObjectKindLabel(object.kind)} · {snapshot?.state ?? "Not fetched"}
          </span>
        </div>
        {object.provider === "github" && (
          <button
            type="button"
            className="secondary-button"
            disabled={isSaving}
            onClick={() => void onRefresh()}
          >
            Refresh
          </button>
        )}
      </div>
      <a href={object.canonical_url} target="_blank" rel="noreferrer">
        {object.canonical_url}
      </a>
      {snapshot ? (
        <>
          <div className="external-metadata">
            {snapshot.metadata.map((metadata) => (
              <span key={`${metadata.key}-${metadata.value}`}>
                {metadata.key}: {metadata.value}
              </span>
            ))}
          </div>
          <p className="external-age">Fetched {formatSnapshotAge(snapshot.fetched_at)}</p>
        </>
      ) : (
        <p className="external-age">No snapshot yet</p>
      )}
      {object.provider === "github" && object.kind !== "generic" && (
        <form className="comment-form" onSubmit={handleComment}>
          <label>
            <span>Comment on GitHub</span>
            <textarea
              value={comment}
              onChange={(event) => setComment(event.target.value)}
              rows={2}
              placeholder="Write a short public reply"
              disabled={isSaving || isCommenting}
            />
          </label>
          <button
            type="submit"
            className="secondary-button"
            disabled={isSaving || isCommenting || !comment.trim()}
          >
            {isCommenting ? "Posting…" : "Add comment"}
          </button>
        </form>
      )}
      {reviewDateReached && (
        <div className="link-attention">
          <strong>Review date reached</strong>
          <p>Review scheduled for {externalLink.link.review_at}</p>
          <button
            type="button"
            className="secondary-button"
            disabled={isSaving}
            onClick={() => void onClearReviewAt()}
          >
            Clear review date
          </button>
        </div>
      )}
      {externalLink.attention_entry && (
        <div className="link-attention">
          <strong>
            {externalLink.attention_entry.kind === "review"
              ? "Review date reached"
              : "Needs review"}
          </strong>
          <p>{externalLink.attention_entry.summary}</p>
          {externalLink.attention_entry.kind === "review" ? (
            <button
              type="button"
              className="secondary-button"
              disabled={isSaving}
              onClick={() => void onClearReviewAt()}
            >
              Clear review date
            </button>
          ) : (
            <button
              type="button"
              className="secondary-button"
              disabled={isSaving}
              onClick={() => void onMarkReviewed()}
            >
              Mark changes reviewed
            </button>
          )}
        </div>
      )}
      <div className="watch-schedule">
        <span className="relationship-label">Watch schedule</span>
        <label>
          <span>Watch until</span>
          <input
            type="datetime-local"
            value={watchUntil}
            onChange={(event) => setWatchUntil(event.target.value)}
            disabled={isSaving}
          />
        </label>
        <button
          type="button"
          className="secondary-button"
          disabled={isSaving}
          onClick={() => void onSaveWatchUntil(watchUntil || null)}
        >
          Save watch period
        </button>
        <label>
          <span>Review at</span>
          <input
            type="datetime-local"
            value={reviewAt}
            onChange={(event) => setReviewAt(event.target.value)}
            disabled={isSaving}
          />
        </label>
        <button
          type="button"
          className="secondary-button"
          disabled={isSaving}
          onClick={() => void onSaveReviewAt(reviewAt || null)}
        >
          Save review date
        </button>
      </div>
      <div className="attention-policy">
        <span className="relationship-label">Attention for this Link</span>
        <label>
          <input
            type="checkbox"
            checked={policy.title}
            onChange={(event) =>
              setPolicy((current) => ({ ...current, title: event.target.checked }))
            }
            disabled={isSaving}
          />
          Title
        </label>
        <label>
          <input
            type="checkbox"
            checked={policy.state}
            onChange={(event) =>
              setPolicy((current) => ({ ...current, state: event.target.checked }))
            }
            disabled={isSaving}
          />
          State
        </label>
        <label>
          <input
            type="checkbox"
            checked={policy.metadata}
            onChange={(event) =>
              setPolicy((current) => ({ ...current, metadata: event.target.checked }))
            }
            disabled={isSaving}
          />
          Metadata
        </label>
        <button
          type="button"
          className="secondary-button"
          disabled={isSaving}
          onClick={() => void onSavePolicy(policy)}
        >
          Save Link policy
        </button>
        {externalLink.link.attention_policy && (
          <button
            type="button"
            className="text-button"
            disabled={isSaving}
            onClick={() => void onSavePolicy(null)}
          >
            Use Context default
          </button>
        )}
      </div>
    </article>
  );
}

function AttentionEntryCard({
  entry,
  item,
  onMarkedReviewed,
}: {
  entry: AttentionEntry;
  item: ItemView | undefined;
  onMarkedReviewed: () => Promise<void>;
}) {
  const [isSaving, setIsSaving] = useState(false);

  async function markReviewed() {
    setIsSaving(true);
    try {
      await invoke("mark_link_reviewed", { linkId: entry.link_id });
      await onMarkedReviewed();
    } catch (reviewError) {
      window.alert(errorMessage(reviewError));
    } finally {
      setIsSaving(false);
    }
  }

  return (
    <article className="attention-entry-card">
      <div>
        <strong>{entry.source_title}</strong>
        <span className="external-link-kind">
          {item?.item.human_identifier ?? "Item"} · {attentionEntryLabel(entry)}
        </span>
      </div>
      <p>{entry.summary}</p>
      {entry.kind === "reminder" && item && entry.reminder_id !== null ? (
        <button
          type="button"
          className="secondary-button"
          disabled={isSaving}
          onClick={() =>
            void saveReminder(item.item.id, entry.reminder_id as number)
          }
        >
          Dismiss reminder
        </button>
      ) : entry.kind === "review" ? (
        <button
          type="button"
          className="secondary-button"
          disabled={isSaving}
          onClick={() => void clearReviewDate()}
        >
          Clear review date
        </button>
      ) : entry.kind === "blocked_run" ? (
        <span className="attention-state">Open the Run to answer the agent.</span>
      ) : (
        <button
          type="button"
          className="secondary-button"
          disabled={isSaving}
          onClick={() => void markReviewed()}
        >
          Mark reviewed
        </button>
      )}
    </article>
  );

  async function saveReminder(itemId: number, reminderId: number) {
    setIsSaving(true);
    try {
      await invoke("remove_item_reminder", { itemId, reminderId });
      await onMarkedReviewed();
    } catch (dismissError) {
      window.alert(errorMessage(dismissError));
    } finally {
      setIsSaving(false);
    }
  }

  async function clearReviewDate() {
    setIsSaving(true);
    try {
      await invoke("clear_link_review_at", { linkId: entry.link_id });
      await onMarkedReviewed();
    } catch (clearError) {
      window.alert(errorMessage(clearError));
    } finally {
      setIsSaving(false);
    }
  }
}

function attentionEntryLabel(entry: AttentionEntry): string {
  if (entry.kind === "reminder") return "Reminder due";
  if (entry.kind === "review") return "Review date reached";
  if (entry.kind === "blocked_run") return "Run blocked";
  return `${entry.activities.length} change${entry.activities.length === 1 ? "" : "s"}`;
}

function runStateLabel(state: RunState): string {
  if (state === "working") return "Working";
  if (state === "blocked") return "Blocked";
  if (state === "finished") return "Finished";
  return "Unknown";
}

function SearchResult({ view }: { view: ItemView }) {
  return (
    <article className="search-result">
      <span className="item-identifier">{view.item.human_identifier}</span>
      <div>
        <h3>{view.item.title}</h3>
        <p>
          {view.context_name} <span>·</span> {view.project_name} <span>·</span>{" "}
          {view.item.status}
        </p>
      </div>
    </article>
  );
}

function externalObjectKindLabel(kind: ExternalObject["kind"]): string {
  if (kind === "pull_request") return "Pull request";
  if (kind === "issue") return "Issue";
  return "Link";
}

function formatSnapshotAge(fetchedAt: number): string {
  const ageSeconds = Math.max(0, Math.floor(Date.now() / 1000) - fetchedAt);
  if (ageSeconds < 60) return "just now";
  if (ageSeconds < 3600) return `${Math.floor(ageSeconds / 60)}m ago`;
  if (ageSeconds < 86400) return `${Math.floor(ageSeconds / 3600)}h ago`;
  return `${Math.floor(ageSeconds / 86400)}d ago`;
}

function relationshipLabel(relation: ItemRelation, currentItemId: number): string {
  if (relation.from_item_id === currentItemId) {
    return relationKindLabel(relation.kind);
  }
  if (relation.kind === "Blocks") return "blocked by";
  if (relation.kind === "BlockedBy") return "blocks";
  return "related to";
}

function relationKindLabel(kind: ItemRelationKind): string {
  if (kind === "BlockedBy") return "blocked by";
  if (kind === "RelatedTo") return "related to";
  return "blocks";
}

function repositoryName(repositories: Repository[], repositoryId: number): string {
  return (
    repositories.find((repository) => repository.id === repositoryId)?.name ??
    `Repository ${repositoryId}`
  );
}

function flattenHome(view: HomeView): ItemView[] {
  return [
    ...view.needs_attention,
    ...view.running,
    ...view.waiting,
    ...view.due,
    ...view.completed,
  ];
}

function findWorkset(view: ItemView, worksetId: number): Workset | undefined {
  return [...view.worksets, ...view.archived_worksets].find(
    (workset) => workset.id === worksetId,
  );
}

function paneTabForRun(run: Run): PaneTab {
  return {
    paneId: run.pane_id,
    sessionName: run.session_name,
    runId: run.id,
    label: `Run #${run.id}`,
    available: true,
    paneIndex: 0,
    pid: 0,
    columns: 0,
    rows: 0,
    title: "",
    currentCommand: "",
    currentPath: run.working_directory,
  };
}

function uniqueItems(items: ItemView[]): ItemView[] {
  return Array.from(new Map(items.map((item) => [item.item.id, item])).values());
}

function currentMinute(): string {
  return new Date(Date.now() - new Date().getTimezoneOffset() * 60_000)
    .toISOString()
    .slice(0, 16);
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
