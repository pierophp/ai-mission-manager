import { type FormEvent, useEffect, useMemo, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate, useSearch } from "@tanstack/react-router";

import { Button } from "../../components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "../../components/ui/card";
import { Empty, EmptyDescription } from "../../components/ui/empty";
import { Input } from "../../components/ui/input";
import { NativeSelect, NativeSelectOption } from "../../components/ui/native-select";
import { Spinner } from "../../components/ui/spinner";
import { useAppShell } from "../../components/app-shell";
import { errorMessage } from "../../runtime/errors";
import {
  invalidateWorkQueries,
} from "../../runtime/query-invalidation";
import { usePollExternalObjects } from "../../runtime/RuntimeEventsBridge";
import { structureActions, useStructureCommand } from "../structure/structure-mutations";
import type { Context, Project, RunSuggestion } from "../../runtime/types";
import {
  AttentionEntryCard,
  HomeColumn,
  RunSuggestionCard,
  SearchResult,
} from "./components";
import type { WorkSearch } from "./work-search";
import { parseWorkSearch } from "./work-search";
import { flattenHome, uniqueItems } from "./work-utils";
import {
  useHomeQuery,
  useRunSuggestionsQuery,
  useSearchQuery,
} from "./work-queries";
import { useWorkCommand, workActions } from "./work-mutations";
import { useStructureData } from "../structure/structure-queries";

export function WorkPage() {
  const { openTerminal: onOpenTerminal } = useAppShell();
  const queryClient = useQueryClient();
  const [debouncedSearchQuery, setDebouncedSearchQuery] = useState("");
  const structure = useStructureData().data;
  const { contexts, projects, repositories, machines } = structure;
  const search = useSearch({ from: "/work" });
  const navigate = useNavigate({ from: "/work" });
  const normalizedSearch = parseWorkSearch(search);
  const contextFilterId = normalizedSearch.contextId;
  const searchQuery = normalizedSearch.q ?? "";
  const homeQuery = useHomeQuery(contextFilterId);
  const suggestionsQuery = useRunSuggestionsQuery();
  const searchResultsQuery = useSearchQuery(
    debouncedSearchQuery,
    contextFilterId,
  );
  const home = homeQuery.data;
  const runSuggestions = suggestionsQuery.data ?? [];
  const searchResults = searchResultsQuery.data ?? [];
  const isLoading = homeQuery.isPending;
  const pollExternalObjects = usePollExternalObjects();
  const workCommand = useWorkCommand();
  const structureCommand = useStructureCommand();
  const [captureContextId, setCaptureContextId] = useState<number>();
  const [captureProjectId, setCaptureProjectId] = useState<number>();
  const [title, setTitle] = useState("");
  const isSaving = workCommand.isPending || structureCommand.isPending;

  const captureProjects = projects.filter(
    (project) => project.context_id === captureContextId,
  );
  const allItems = useMemo(
    () => uniqueItems(home ? flattenHome(home) : []),
    [home],
  );
  const visibleSuggestions = runSuggestions.filter(
    (suggestion) =>
      contextFilterId === undefined || suggestion.contextId === contextFilterId,
  );

  useEffect(() => {
    if (
      search.contextId === normalizedSearch.contextId &&
      search.q === normalizedSearch.q
    ) {
      return;
    }
    void navigate({ search: normalizedSearch, replace: true });
  }, [
    navigate,
    normalizedSearch.contextId,
    normalizedSearch.q,
    search.contextId,
    search.q,
  ]);

  useEffect(() => {
    const timer = window.setTimeout(
      () => setDebouncedSearchQuery(searchQuery),
      250,
    );
    return () => window.clearTimeout(timer);
  }, [searchQuery]);

  useEffect(() => {
    if (contextFilterId === undefined || contexts.length === 0) return;
    if (contexts.some((context) => context.id === contextFilterId)) return;
    void navigate({
      search: (current) => ({ ...current, contextId: undefined }),
      replace: true,
    });
  }, [contextFilterId, contexts, navigate]);

  useEffect(() => {
    const nextContextId =
      contexts.find((context) => context.id === captureContextId)?.id ??
      contexts[0]?.id;
    const nextProjectId =
      projects.find(
        (project) =>
          project.id === captureProjectId &&
          project.context_id === nextContextId,
      )?.id ?? projects.find((project) => project.context_id === nextContextId)?.id;

    if (nextContextId !== captureContextId) setCaptureContextId(nextContextId);
    if (nextProjectId !== captureProjectId) setCaptureProjectId(nextProjectId);
  }, [captureContextId, captureProjectId, contexts, projects]);

  function updateSearch(updates: Partial<WorkSearch>) {
    void navigate({
      search: (current) => ({ ...current, ...updates }),
      replace: true,
    });
  }

  function handleContextChange(value: string) {
    updateSearch({
      contextId: value === "all" ? undefined : Number(value),
    });
  }

  async function handleCreateItem(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!title.trim() || !captureContextId || !captureProjectId) return;

    try {
      await structureCommand.execute(
        structureActions.createItem(title, captureContextId, captureProjectId),
      );
      setTitle("");
    } catch (createError) {
      window.alert(errorMessage(createError));
    }
  }

  async function handleAttachRun(suggestion: RunSuggestion) {
    try {
      await workCommand.execute(workActions.attachRun(suggestion));
    } catch (attachError) {
      window.alert(errorMessage(attachError));
    }
  }

  return (
    <div className="mt-6 space-y-6">
      <Card className="overflow-visible border-border/70 bg-card/80 shadow-sm">
        <CardContent className="flex flex-wrap items-end gap-4 p-4">
          <label className="grid min-w-48 flex-1 gap-1.5 text-sm font-medium">
            <span>Context</span>
            <NativeSelect
              className="w-full"
              value={contextFilterId ?? "all"}
              onChange={(event) => handleContextChange(event.target.value)}
            >
              <NativeSelectOption value="all">All Contexts</NativeSelectOption>
              {contexts.map((context) => (
                <NativeSelectOption value={context.id} key={context.id}>
                  {context.name}
                </NativeSelectOption>
              ))}
            </NativeSelect>
          </label>
          <label className="grid min-w-64 flex-[2] gap-1.5 text-sm font-medium">
            <span>Search every Context</span>
            <Input
              value={searchQuery}
              onChange={(event) =>
                updateSearch({ q: event.target.value.trim() ? event.target.value : undefined })
              }
              placeholder="Search Items, notes, or identifiers"
            />
          </label>
          <Button
            type="button"
            variant="outline"
            onClick={() =>
              void pollExternalObjects.mutateAsync().catch((pollError) =>
                window.alert(errorMessage(pollError)),
              )
            }
            disabled={pollExternalObjects.isPending}
          >
            {pollExternalObjects.isPending
              ? "Refreshing linked objects…"
              : "Refresh linked objects"}
          </Button>
        </CardContent>
      </Card>

      {visibleSuggestions.length > 0 && (
        <Card>
          <CardHeader className="border-b border-border/70">
            <CardTitle>Untracked agents</CardTitle>
            <CardDescription>Found outside the app · approval required</CardDescription>
          </CardHeader>
          <CardContent className="grid gap-3 p-4">
            {visibleSuggestions.map((suggestion) => (
              <RunSuggestionCard
                key={`${suggestion.machineId}-${suggestion.sessionName}-${suggestion.paneId}`}
                suggestion={suggestion}
                disabled={isSaving}
                onAttach={handleAttachRun}
              />
            ))}
          </CardContent>
        </Card>
      )}

      {searchQuery.trim() && (
        <Card>
          <CardHeader className="border-b border-border/70">
            <CardTitle>Search results</CardTitle>
            <CardDescription>Every Context · {searchResults.length} matches</CardDescription>
          </CardHeader>
          <CardContent className="grid gap-3 p-4">
            {searchResults.length === 0 ? (
              <Empty className="border-0 p-4">
                <EmptyDescription>No Items match that search.</EmptyDescription>
              </Empty>
            ) : (
              searchResults.map((view) => <SearchResult key={view.item.id} view={view} />)
            )}
          </CardContent>
        </Card>
      )}

      <Card>
        <CardHeader className="border-b border-border/70">
          <CardTitle>
            {contextFilterId
              ? contexts.find((context) => context.id === contextFilterId)?.name ??
                "Filtered Items"
              : "All Items"}
          </CardTitle>
          <CardDescription>Needs Attention is a cross-cutting projection; statuses remain yours to move.</CardDescription>
        </CardHeader>
        {isLoading || !home ? (
          <Empty className="min-h-48 border-0">
            <Spinner />
            <EmptyDescription>Loading your Work view…</EmptyDescription>
          </Empty>
        ) : (
          <CardContent className="space-y-5 p-4">
            {home.attention_entries.length > 0 && (
              <section className="rounded-lg border border-amber-500/30 bg-amber-500/5 p-4" aria-labelledby="attention-heading">
                <div className="mb-3 flex items-start justify-between gap-4">
                  <div>
                    <h3 id="attention-heading" className="font-heading text-base font-medium">
                      Needs Attention
                    </h3>
                    <p className="mt-1 text-sm text-muted-foreground">
                      Review changes, reminders, and blocked Runs without changing the Item state.
                    </p>
                  </div>
                  <span className="rounded-full bg-muted px-2 py-1 text-xs font-medium">
                    {home.attention_entries.length}
                  </span>
                </div>
                <div className="grid gap-3">
                  {home.attention_entries.map((entry) => (
                    <AttentionEntryCard
                      key={`${entry.kind}-${entry.link_id}-${entry.reminder_id ?? ""}-${entry.run_id ?? ""}`}
                      entry={entry}
                      item={allItems.find((candidate) => candidate.item.id === entry.item_id)}
                      onMarkedReviewed={() => invalidateWorkQueries(queryClient)}
                    />
                  ))}
                </div>
              </section>
            )}
            <div className="grid gap-4 xl:grid-cols-5">
              <HomeColumn
                title="Needs Attention"
                hint="Unstarted or due"
                items={home.needs_attention}
                allItems={allItems}
                repositories={repositories}
                machines={machines}
                onChanged={() => invalidateWorkQueries(queryClient)}
                onOpenTerminal={onOpenTerminal}
              />
              <HomeColumn
                title="Running"
                hint="Active"
                items={home.running}
                allItems={allItems}
                repositories={repositories}
                machines={machines}
                onChanged={() => invalidateWorkQueries(queryClient)}
                onOpenTerminal={onOpenTerminal}
              />
              <HomeColumn
                title="Waiting"
                hint="Waiting"
                items={home.waiting}
                allItems={allItems}
                repositories={repositories}
                machines={machines}
                onChanged={() => invalidateWorkQueries(queryClient)}
                onOpenTerminal={onOpenTerminal}
              />
              <HomeColumn
                title="Due"
                hint="Reminder reached"
                items={home.due}
                allItems={allItems}
                repositories={repositories}
                machines={machines}
                onChanged={() => invalidateWorkQueries(queryClient)}
                onOpenTerminal={onOpenTerminal}
              />
              <HomeColumn
                title="Completed"
                hint="Done"
                items={home.completed}
                allItems={allItems}
                repositories={repositories}
                machines={machines}
                onChanged={() => invalidateWorkQueries(queryClient)}
                onOpenTerminal={onOpenTerminal}
              />
            </div>
          </CardContent>
        )}
      </Card>

      <Card>
        <CardHeader className="border-b border-border/70">
          <CardTitle>Give the next decision a place</CardTitle>
          <CardDescription>New Item · Title + Context + Project</CardDescription>
        </CardHeader>
        <CardContent className="p-4">
          <form className="grid gap-4 md:grid-cols-[2fr_1fr_1fr_auto] md:items-end" onSubmit={handleCreateItem}>
            <label className="grid gap-1.5 text-sm font-medium">
              <span>Title</span>
              <Input
                autoFocus
                value={title}
                onChange={(event) => setTitle(event.target.value)}
                placeholder="Investigate slow invoice import"
                disabled={isSaving}
              />
            </label>
            <label className="grid gap-1.5 text-sm font-medium">
              <span>Context</span>
              <NativeSelect
                className="w-full"
                value={captureContextId ?? ""}
                onChange={(event) => {
                  const nextContextId = Number(event.target.value);
                  setCaptureContextId(nextContextId);
                  setCaptureProjectId(
                    projects.find((project) => project.context_id === nextContextId)?.id,
                  );
                }}
                disabled={isSaving || contexts.length === 0}
              >
                {contexts.map((context: Context) => (
                  <NativeSelectOption value={context.id} key={context.id}>
                    {context.name}
                  </NativeSelectOption>
                ))}
              </NativeSelect>
            </label>
            <label className="grid gap-1.5 text-sm font-medium">
              <span>Project</span>
              <NativeSelect
                className="w-full"
                value={captureProjectId ?? ""}
                onChange={(event) => setCaptureProjectId(Number(event.target.value))}
                disabled={isSaving || captureProjects.length === 0}
              >
                {captureProjects.map((project: Project) => (
                  <NativeSelectOption value={project.id} key={project.id}>
                    {project.name}
                  </NativeSelectOption>
                ))}
              </NativeSelect>
            </label>
            <Button type="submit" disabled={isSaving || !title.trim() || !captureContextId || !captureProjectId}>
              {isSaving ? "Saving…" : "Add Item"}
            </Button>
          </form>
        </CardContent>
      </Card>
    </div>
  );
}
