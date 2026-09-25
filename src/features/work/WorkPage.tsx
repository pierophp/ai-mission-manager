import {
  type FormEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useNavigate, useSearch } from "@tanstack/react-router";
import { Plus } from "lucide-react";
import { toast } from "sonner";

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
import { ConfirmationDialog } from "../../components/ui/confirmation-dialog";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../../components/ui/dialog";
import { useAppShell } from "../../components/app-shell";
import { errorMessage } from "../../runtime/errors";
import {
  invalidateRunQueries,
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
import { ItemDetailPanel } from "./item-detail/ItemDetailPanel";
import type { GrillAnswerDrafts } from "./item-detail/RunsTab";
import {
  type ItemDetailTab,
  type ItemForm,
  type ItemIntent,
  defaultItemTab,
  displayItemIdentifier,
  grillQuestionKey,
  isGrillWaitingForAnswers,
  tabForForm,
} from "./item-signals";
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

type UntrackedAgentAction = {
  action: "stop" | "delete";
  suggestion: RunSuggestion;
};

export function WorkPage() {
  const { openTerminal: onOpenTerminal } = useAppShell();
  const queryClient = useQueryClient();
  const [debouncedSearchQuery, setDebouncedSearchQuery] = useState("");
  const structure = useStructureData().data;
  const { contexts, projects, repositories, machines, grillModelCatalog } = structure;
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
  const [isCreateItemOpen, setIsCreateItemOpen] = useState(false);
  const [untrackedAgentAction, setUntrackedAgentAction] =
    useState<UntrackedAgentAction>();
  const [itemIntent, setItemIntent] = useState<ItemIntent>();
  const [grillDrafts, setGrillDrafts] = useState<GrillAnswerDrafts>({});
  const [pendingDiscard, setPendingDiscard] = useState<() => void>();
  const notesDirtyRef = useRef(false);
  const intentNonce = useRef(0);
  const isSaving = workCommand.isPending || structureCommand.isPending;
  const selectedItemId = normalizedSearch.item;

  const captureProjects = projects.filter(
    (project) => project.context_id === captureContextId,
  );
  const allItems = useMemo(
    () => uniqueItems(home ? flattenHome(home) : []),
    [home],
  );
  const selectedView =
    selectedItemId === undefined
      ? undefined
      : (allItems.find((candidate) => candidate.item.id === selectedItemId) ??
        searchResults.find((candidate) => candidate.item.id === selectedItemId));
  const selectedTab: ItemDetailTab =
    normalizedSearch.tab ?? (selectedView ? defaultItemTab(selectedView) : "overview");
  const visibleSuggestions = runSuggestions.filter(
    (suggestion) =>
      contextFilterId === undefined || suggestion.contextId === contextFilterId,
  );

  useEffect(() => {
    if (
      search.contextId === normalizedSearch.contextId &&
      search.q === normalizedSearch.q &&
      search.item === normalizedSearch.item &&
      search.tab === normalizedSearch.tab
    ) {
      return;
    }
    void navigate({ search: normalizedSearch, replace: true });
  }, [
    navigate,
    normalizedSearch.contextId,
    normalizedSearch.q,
    normalizedSearch.item,
    normalizedSearch.tab,
    search.contextId,
    search.q,
    search.item,
    search.tab,
  ]);

  // A link that names an Item but no tab settles on the default tab once, so
  // the tab never switches by itself while the Item stays open.
  useEffect(() => {
    if (!selectedView || normalizedSearch.tab !== undefined) return;
    void navigate({
      search: (current) => ({ ...current, tab: defaultItemTab(selectedView) }),
      replace: true,
    });
  }, [navigate, normalizedSearch.tab, selectedView]);

  // Close the panel when its Item is gone (deleted, or never existed).
  const searchSettled =
    !searchQuery.trim() ||
    (debouncedSearchQuery === searchQuery && !searchResultsQuery.isFetching);
  useEffect(() => {
    if (selectedItemId === undefined || selectedView) return;
    if (!home || homeQuery.isFetching || !searchSettled) return;
    void navigate({
      search: (current) => ({ ...current, item: undefined, tab: undefined }),
      replace: true,
    });
  }, [home, homeQuery.isFetching, navigate, searchSettled, selectedItemId, selectedView]);

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

  const refreshWork = useCallback(
    () => invalidateWorkQueries(queryClient),
    [queryClient],
  );

  function updateSearch(updates: Partial<WorkSearch>) {
    void navigate({
      search: (current) => ({ ...current, ...updates }),
      replace: true,
    });
  }

  /** Run a navigation that would drop unsaved Notes only after confirming. */
  function guardNotes(proceed: () => void) {
    if (notesDirtyRef.current) {
      setPendingDiscard(() => proceed);
      return;
    }
    proceed();
  }

  function openItem(itemId: number, form?: ItemForm, tab?: ItemDetailTab) {
    const view =
      allItems.find((candidate) => candidate.item.id === itemId) ??
      searchResults.find((candidate) => candidate.item.id === itemId);
    const formTab = form ? tabForForm(form) : undefined;
    const nextTab =
      tab ??
      formTab ??
      (itemId === selectedItemId
        ? selectedTab
        : view
          ? defaultItemTab(view)
          : undefined);
    const show = () => {
      intentNonce.current += 1;
      setItemIntent(form ? { itemId, form, nonce: intentNonce.current } : undefined);
      void navigate({
        search: (current) => ({ ...current, item: itemId, tab: nextTab }),
      });
    };
    if (itemId === selectedItemId) show();
    else guardNotes(show);
  }

  function closeItem() {
    guardNotes(() => {
      setItemIntent(undefined);
      void navigate({
        search: (current) => ({ ...current, item: undefined, tab: undefined }),
      });
    });
  }

  function changeTab(tab: ItemDetailTab) {
    void navigate({
      search: (current) => ({ ...current, tab }),
      replace: true,
    });
  }

  // Tell the user when a Grill starts waiting on them, without taking focus.
  // What is already waiting when the view loads is shown by the card badges.
  const openItemRef = useRef(openItem);
  openItemRef.current = openItem;
  const openPanelRef = useRef({ itemId: selectedItemId, tab: selectedTab });
  openPanelRef.current = { itemId: selectedItemId, tab: selectedTab };
  const seenGrillQuestions = useRef<{ contextId?: number; keys: Set<string> }>(
    undefined,
  );
  useEffect(() => {
    if (!home) return;
    const waiting = allItems.flatMap((view) =>
      view.runs
        .filter(isGrillWaitingForAnswers)
        .map((run) => ({ view, run, key: grillQuestionKey(run) })),
    );
    const seen = seenGrillQuestions.current;
    const isBaseline = !seen || seen.contextId !== contextFilterId;
    seenGrillQuestions.current = {
      contextId: contextFilterId,
      keys: new Set([
        ...(isBaseline ? [] : seen.keys),
        ...waiting.map(({ key }) => key),
      ]),
    };
    if (isBaseline) return;
    for (const { view, run, key } of waiting) {
      if (seen.keys.has(key)) continue;
      const open = openPanelRef.current;
      if (open.itemId === view.item.id && open.tab === "runs") continue;
      toast(
        `Grill questions for ${displayItemIdentifier(view.item.human_identifier)}`,
        {
          description: `Run #${run.id} · ${view.item.title}`,
          duration: 15_000,
          action: {
            label: "Answer",
            onClick: () => openItemRef.current(view.item.id, undefined, "runs"),
          },
        },
      );
    }
  }, [allItems, contextFilterId, home]);

  const handleNotesDirtyChange = useCallback((dirty: boolean) => {
    notesDirtyRef.current = dirty;
  }, []);

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
      setIsCreateItemOpen(false);
    } catch (createError) {
      window.alert(errorMessage(createError));
    }
  }

  async function handleAttachRun(suggestion: RunSuggestion) {
    try {
      await workCommand.execute(workActions.attachRun(suggestion), false);
      await invalidateRunQueries(queryClient);
    } catch (attachError) {
      window.alert(errorMessage(attachError));
    }
  }

  function requestUntrackedAgentAction(
    action: UntrackedAgentAction["action"],
    suggestion: RunSuggestion,
  ) {
    setUntrackedAgentAction({ action, suggestion });
  }

  async function confirmUntrackedAgentAction() {
    if (!untrackedAgentAction) return;

    const { action, suggestion } = untrackedAgentAction;
    try {
      await workCommand.execute(
        action === "stop"
          ? workActions.stopUntrackedAgent(suggestion)
          : workActions.deleteUntrackedAgent(suggestion),
        false,
      );
      setUntrackedAgentAction(undefined);
      await invalidateRunQueries(queryClient);
    } catch (actionError) {
      window.alert(errorMessage(actionError));
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
          <Button type="button" onClick={() => setIsCreateItemOpen(true)}>
            <Plus />
            Add Item
          </Button>
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
                onStop={(target) =>
                  requestUntrackedAgentAction("stop", target)
                }
                onDelete={(target) =>
                  requestUntrackedAgentAction("delete", target)
                }
              />
            ))}
          </CardContent>
        </Card>
      )}

      {untrackedAgentAction && (
        <ConfirmationDialog
          open
          title={
            untrackedAgentAction.action === "stop"
              ? "Stop this agent?"
              : "Delete this agent Pane?"
          }
          description={
            untrackedAgentAction.action === "stop"
              ? `Send Ctrl+C to Pane ${untrackedAgentAction.suggestion.paneId} on ${untrackedAgentAction.suggestion.machineName}. The Pane stays open.`
              : `Close Pane ${untrackedAgentAction.suggestion.paneId} on ${untrackedAgentAction.suggestion.machineName}. If it is the session's last Pane, the session will close too.`
          }
          confirmLabel={
            untrackedAgentAction.action === "stop" ? "Stop agent" : "Delete Pane"
          }
          disabled={isSaving}
          onOpenChange={(open) => {
            if (!open && !isSaving) setUntrackedAgentAction(undefined);
          }}
          onConfirm={() => void confirmUntrackedAgentAction()}
        />
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
              searchResults.map((view) => (
                <SearchResult
                  key={view.item.id}
                  view={view}
                  onOpen={() => openItem(view.item.id)}
                />
              ))
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
                onOpenItem={openItem}
                onChanged={refreshWork}
              />
              <HomeColumn
                title="Running"
                hint="Active"
                items={home.running}
                onOpenItem={openItem}
                onChanged={refreshWork}
              />
              <HomeColumn
                title="Waiting"
                hint="Waiting"
                items={home.waiting}
                onOpenItem={openItem}
                onChanged={refreshWork}
              />
              <HomeColumn
                title="Due"
                hint="Reminder reached"
                items={home.due}
                onOpenItem={openItem}
                onChanged={refreshWork}
              />
              <HomeColumn
                title="Completed"
                hint="Done"
                items={home.completed}
                onOpenItem={openItem}
                onChanged={refreshWork}
              />
            </div>
          </CardContent>
        )}
      </Card>

      <Dialog open={isCreateItemOpen} onOpenChange={setIsCreateItemOpen}>
        <DialogContent className="sm:max-w-xl">
          <DialogHeader>
            <DialogTitle>Give the next decision a place</DialogTitle>
            <DialogDescription>Add an Item to a Context and Project.</DialogDescription>
          </DialogHeader>
          <form className="grid gap-4" onSubmit={handleCreateItem}>
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
            <DialogFooter>
              <DialogClose asChild>
                <Button type="button" variant="outline" disabled={isSaving}>
                  Cancel
                </Button>
              </DialogClose>
              <Button
                type="submit"
                disabled={isSaving || !title.trim() || !captureContextId || !captureProjectId}
              >
                {isSaving ? "Saving…" : "Add Item"}
              </Button>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      <Dialog
        open={Boolean(selectedView)}
        onOpenChange={(open) => {
          if (!open) closeItem();
        }}
      >
        {selectedView && (
          <ItemDetailPanel
            key={selectedView.item.id}
            view={selectedView}
            allItems={allItems}
            repositories={repositories}
            machines={machines}
            contexts={contexts}
            grillModelCatalog={grillModelCatalog}
            tab={selectedTab}
            intent={
              itemIntent?.itemId === selectedView.item.id ? itemIntent : undefined
            }
            grillDrafts={grillDrafts}
            onGrillDraftsChange={setGrillDrafts}
            onTabChange={changeTab}
            onOpenForm={(form) => openItem(selectedView.item.id, form)}
            onClose={closeItem}
            onChanged={refreshWork}
            onOpenTerminal={onOpenTerminal}
            onNotesDirtyChange={handleNotesDirtyChange}
          />
        )}
      </Dialog>

      {pendingDiscard && (
        <ConfirmationDialog
          open
          title="Discard unsaved Notes?"
          description="The Notes you changed on this Item have not been saved."
          confirmLabel="Discard changes"
          onOpenChange={(open) => {
            if (!open) setPendingDiscard(undefined);
          }}
          onConfirm={() => {
            const proceed = pendingDiscard;
            setPendingDiscard(undefined);
            notesDirtyRef.current = false;
            proceed();
          }}
        />
      )}
    </div>
  );
}
