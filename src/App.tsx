import {
  FormEvent,
  useEffect,
  useMemo,
  useState,
} from "react";
import { invoke } from "@tauri-apps/api/core";

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
  kind: "external_change" | "review" | "reminder";
  link_id: number;
  reminder_id: number | null;
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
  context_name: string;
  project_name: string;
  relationships: ItemRelation[];
  links: ExternalLinkView[];
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
  const [attentionObjectKind, setAttentionObjectKind] =
    useState<ExternalObjectKind>("pull_request");
  const [attentionDefaultPolicy, setAttentionDefaultPolicy] =
    useState<ExternalChangePolicy>({ title: true, state: true, metadata: true });
  const [searchQuery, setSearchQuery] = useState("");
  const [error, setError] = useState<string>();
  const [isLoading, setIsLoading] = useState(true);
  const [isSaving, setIsSaving] = useState(false);

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

  async function loadAppState() {
    setIsLoading(true);
    try {
      const [loadedContexts, loadedProjects, loadedAttentionDefaults] = await Promise.all([
        invoke<Context[]>("list_contexts"),
        invoke<Project[]>("list_projects"),
        invoke<ContextAttentionDefault[]>("list_context_attention_defaults"),
      ]);
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
    await refreshHome();
    await refreshSearch();
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
                      key={`${entry.kind}-${entry.link_id}-${entry.reminder_id ?? ""}`}
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
              onChanged={updateHomeAfterEdit}
            />
            <HomeColumn
              title="Running"
              hint="Active"
              items={home.running}
              allItems={allItems}
              onChanged={updateHomeAfterEdit}
            />
            <HomeColumn
              title="Waiting"
              hint="Waiting"
              items={home.waiting}
              allItems={allItems}
              onChanged={updateHomeAfterEdit}
            />
            <HomeColumn
              title="Due"
              hint="Reminder reached"
              items={home.due}
              allItems={allItems}
              onChanged={updateHomeAfterEdit}
            />
            <HomeColumn
              title="Completed"
              hint="Done"
              items={home.completed}
              allItems={allItems}
              onChanged={updateHomeAfterEdit}
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

function HomeColumn({
  title,
  hint,
  items,
  allItems,
  onChanged,
}: {
  title: string;
  hint: string;
  items: ItemView[];
  allItems: ItemView[];
  onChanged: () => Promise<void>;
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
              onChanged={onChanged}
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
  onChanged,
}: {
  view: ItemView;
  allItems: ItemView[];
  onChanged: () => Promise<void>;
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
  const [isSaving, setIsSaving] = useState(false);

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
  return `${entry.activities.length} change${entry.activities.length === 1 ? "" : "s"}`;
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

function flattenHome(view: HomeView): ItemView[] {
  return [
    ...view.needs_attention,
    ...view.running,
    ...view.waiting,
    ...view.due,
    ...view.completed,
  ];
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
