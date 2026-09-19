import { FormEvent, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type Context = {
  id: number;
  name: string;
};

type Item = {
  id: number;
  human_identifier: string;
  title: string;
  context_id: number;
  status: "Inbox" | "Active" | "Waiting" | "Done";
};

export function App() {
  const [contexts, setContexts] = useState<Context[]>([]);
  const [items, setItems] = useState<Item[]>([]);
  const [title, setTitle] = useState("");
  const [contextId, setContextId] = useState<number | undefined>();
  const [error, setError] = useState<string>();
  const [isLoading, setIsLoading] = useState(true);
  const [isSaving, setIsSaving] = useState(false);

  useEffect(() => {
    void loadAppState();
  }, []);

  async function loadAppState() {
    setIsLoading(true);
    try {
      const [loadedContexts, loadedItems] = await Promise.all([
        invoke<Context[]>("list_contexts"),
        invoke<Item[]>("list_inbox_items"),
      ]);
      setContexts(loadedContexts);
      setContextId((current) => current ?? loadedContexts[0]?.id);
      setItems(loadedItems);
      setError(undefined);
    } catch (loadError) {
      setError(errorMessage(loadError));
    } finally {
      setIsLoading(false);
    }
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!contextId) {
      setError("Choose a Context before creating an Item.");
      return;
    }

    setIsSaving(true);
    try {
      const item = await invoke<Item>("create_item", {
        title,
        contextId,
      });
      setItems((current) => [...current, item]);
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

      <section className="capture-card" aria-labelledby="capture-heading">
        <div className="section-heading">
          <div>
            <p className="eyebrow">New Item</p>
            <h2 id="capture-heading">What needs a place?</h2>
          </div>
          <span className="key-hint">Title + Context</span>
        </div>
        <form className="capture-form" onSubmit={handleSubmit}>
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
              onChange={(event) => setContextId(Number(event.target.value))}
              disabled={isSaving || contexts.length === 0}
            >
              {contexts.map((context) => (
                <option value={context.id} key={context.id}>
                  {context.name}
                </option>
              ))}
            </select>
          </label>
          <button type="submit" disabled={isSaving || !title.trim() || !contextId}>
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
            {items.map((item) => (
              <li className="item-row" key={item.id}>
                <span className="item-identifier">{item.human_identifier}</span>
                <span className="item-title">{item.title}</span>
                <span className="item-context">
                  {contexts.find((context) => context.id === item.context_id)?.name ?? "Unknown Context"}
                </span>
              </li>
            ))}
          </ul>
        )}
      </section>
    </main>
  );
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}
