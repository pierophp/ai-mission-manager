import { FormEvent, useEffect, useState } from "react";
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
};

const itemStatuses: ItemStatus[] = ["Inbox", "Active", "Waiting", "Done"];

export function App() {
  const [contexts, setContexts] = useState<Context[]>([]);
  const [projects, setProjects] = useState<Project[]>([]);
  const [items, setItems] = useState<Item[]>([]);
  const [contextId, setContextId] = useState<number>();
  const [projectId, setProjectId] = useState<number>();
  const [title, setTitle] = useState("");
  const [contextName, setContextName] = useState("");
  const [projectName, setProjectName] = useState("");
  const [projectDefaultStatus, setProjectDefaultStatus] =
    useState<ItemStatus>("Inbox");
  const [error, setError] = useState<string>();
  const [isLoading, setIsLoading] = useState(true);
  const [isSaving, setIsSaving] = useState(false);

  useEffect(() => {
    void loadAppState();
  }, []);

  const selectedContextProjects = projects.filter(
    (project) => project.context_id === contextId,
  );

  async function loadAppState() {
    setIsLoading(true);
    try {
      const [loadedContexts, loadedProjects, loadedItems] = await Promise.all([
        invoke<Context[]>("list_contexts"),
        invoke<Project[]>("list_projects"),
        invoke<Item[]>("list_inbox_items"),
      ]);
      const nextContextId =
        contexts.find((context) => context.id === contextId)?.id ??
        loadedContexts[0]?.id;
      const nextProjectId =
        loadedProjects.find(
          (project) =>
            project.id === projectId && project.context_id === nextContextId,
        )?.id ??
        loadedProjects.find((project) => project.context_id === nextContextId)
          ?.id;

      setContexts(loadedContexts);
      setProjects(loadedProjects);
      setContextId(nextContextId);
      setProjectId(nextProjectId);
      setItems(loadedItems);
      setError(undefined);
    } catch (loadError) {
      setError(errorMessage(loadError));
    } finally {
      setIsLoading(false);
    }
  }

  function handleContextChange(nextContextId: number) {
    setContextId(nextContextId);
    setProjectId(
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
      setContextId(context.id);
      setProjectId(
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
    if (!contextId) {
      setError("Choose a Context before creating a Project.");
      return;
    }

    setIsSaving(true);
    try {
      const project = await invoke<Project>("create_project", {
        name: projectName,
        contextId,
        defaultItemStatus: projectDefaultStatus,
      });
      setProjects((current) => [...current, project]);
      setProjectId(project.id);
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
    if (!contextId || !projectId) {
      setError("Choose a Context and Project before creating an Item.");
      return;
    }

    setIsSaving(true);
    try {
      const item = await invoke<Item>("create_item", {
        title,
        contextId,
        projectId,
      });
      if (item.status === "Inbox") {
        setItems((current) => [...current, item]);
      }
      setTitle("");
      setError(undefined);
    } catch (saveError) {
      setError(errorMessage(saveError));
    } finally {
      setIsSaving(false);
    }
  }

  return (
    <main className="app-shell">
      <header className="app-header">
        <div>
          <p className="eyebrow">AI Mission Manager</p>
          <h1>Inbox</h1>
          <p className="subtitle">Capture what you have decided to do.</p>
        </div>
        <div className="status-mark" aria-label="Local database connected">
          <span className="status-dot" /> Local
        </div>
      </header>

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
                    {projects.filter((project) => project.context_id === context.id).length} Projects
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
                  value={contextId ?? ""}
                  onChange={(event) =>
                    handleContextChange(Number(event.target.value))
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
              <button type="submit" disabled={isSaving || !projectName.trim() || !contextId}>
                Add Project
              </button>
            </form>
            <ul className="entity-list">
              {projects.map((project) => (
                <li className="entity-row" key={project.id}>
                  <span>{project.name}</span>
                  <span className="entity-meta">
                    {contexts.find((context) => context.id === project.context_id)?.name ??
                      "Unknown Context"} · {project.defaults.item_status}
                  </span>
                </li>
              ))}
            </ul>
          </div>
        </div>
      </section>

      <section className="capture-card" aria-labelledby="capture-heading">
        <div className="section-heading">
          <div>
            <p className="eyebrow">New Item</p>
            <h2 id="capture-heading">What needs a place?</h2>
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
              value={contextId ?? ""}
              onChange={(event) =>
                handleContextChange(Number(event.target.value))
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
              value={projectId ?? ""}
              onChange={(event) => setProjectId(Number(event.target.value))}
              disabled={isSaving || selectedContextProjects.length === 0}
            >
              {selectedContextProjects.map((project) => (
                <option value={project.id} key={project.id}>
                  {project.name}
                </option>
              ))}
            </select>
          </label>
          <button type="submit" disabled={isSaving || !title.trim() || !contextId || !projectId}>
            {isSaving ? "Saving…" : "Add to Inbox"}
          </button>
        </form>
        {error && <p className="error-message" role="alert">{error}</p>}
      </section>

      <section className="inbox-section" aria-labelledby="inbox-heading">
        <div className="section-heading inbox-heading">
          <div>
            <p className="eyebrow">Your work</p>
            <h2 id="inbox-heading">Inbox</h2>
          </div>
          <span className="item-count">{items.length} {items.length === 1 ? "Item" : "Items"}</span>
        </div>
        {isLoading ? (
          <p className="empty-state">Loading your Inbox…</p>
        ) : items.length === 0 ? (
          <p className="empty-state">Nothing here yet. Capture the next thing when you are ready.</p>
        ) : (
          <ul className="item-list">
            {items.map((item) => {
              const project = projects.find((candidate) => candidate.id === item.project_id);
              const context = contexts.find(
                (candidate) => candidate.id === project?.context_id,
              );
              return (
                <li className="item-row" key={item.id}>
                  <span className="item-identifier">{item.human_identifier}</span>
                  <span className="item-title">{item.title}</span>
                  <span className="item-context">
                    {context?.name ?? "Unknown Context"} · {project?.name ?? "Unknown Project"}
                  </span>
                </li>
              );
            })}
          </ul>
        )}
      </section>
    </main>
  );
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
