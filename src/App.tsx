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
  reminder_at: string | null;
};

type ItemRelationKind = "Blocks" | "BlockedBy" | "RelatedTo";

type ItemRelation = {
  from_item_id: number;
  to_item_id: number;
  kind: ItemRelationKind;
};

type ItemView = {
  item: Item;
  context_name: string;
  project_name: string;
  relationships: ItemRelation[];
};

type HomeView = {
  needs_attention: ItemView[];
  running: ItemView[];
  waiting: ItemView[];
  due: ItemView[];
  completed: ItemView[];
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

  async function loadAppState() {
    setIsLoading(true);
    try {
      const [loadedContexts, loadedProjects] = await Promise.all([
        invoke<Context[]>("list_contexts"),
        invoke<Project[]>("list_projects"),
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
      const loadedHome = await invoke<HomeView>("get_home", {
        contextId: contextFilterId ?? null,
        now: currentMinute(),
      });

      setContexts(loadedContexts);
      setProjects(loadedProjects);
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
  const [reminderAt, setReminderAt] = useState(view.item.reminder_at ?? "");
  const [relationKind, setRelationKind] = useState<ItemRelationKind>("Blocks");
  const [targetItemId, setTargetItemId] = useState<number>();
  const [isSaving, setIsSaving] = useState(false);

  useEffect(() => {
    setNotes(view.item.notes);
    setReminderAt(view.item.reminder_at ?? "");
  }, [view.item.notes, view.item.reminder_at]);

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
          <span>Reminder</span>
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
          disabled={isSaving || reminderAt === (view.item.reminder_at ?? "")}
          onClick={() =>
            void saveItem(() =>
              invoke("set_item_reminder", {
                itemId: view.item.id,
                reminderAt: reminderAt || null,
              }),
            )
          }
        >
          Set reminder
        </button>
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
