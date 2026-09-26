import {
  type Dispatch,
  type FormEvent,
  type SetStateAction,
  useEffect,
  useState,
} from "react";
import { PencilIcon, XIcon } from "lucide-react";
import { cn } from "cn";
import { Badge } from "../../../components/ui/badge";
import { Button } from "../../../components/ui/button";
import {
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "../../../components/ui/dialog";
import { Input } from "../../../components/ui/input";
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "../../../components/ui/tabs";
import type { PaneTab } from "../../../runtime/terminal-types";
import type {
  Context,
  GrillAgentCatalog,
  ItemView,
  Machine,
  Repository,
} from "../../../runtime/types";
import { ItemActionsMenu } from "../item-actions-menu";
import {
  type ItemDetailTab,
  type ItemForm,
  type ItemIntent,
  displayItemIdentifier,
  itemSignals,
  itemSpecs,
} from "../item-signals";
import { useItemCommands } from "../use-item-commands";
import { workActions } from "../work-mutations";
import { LinksTab } from "./LinksTab";
import { OverviewTab } from "./OverviewTab";
import { RepositoriesTab } from "./RepositoriesTab";
import { type GrillAnswerDrafts, RunsTab } from "./RunsTab";
import { SpecTab } from "./SpecTab";
import { useFormIntent } from "./shared";

const headerForms = ["rename"] as const;

const tabLabels: Record<ItemDetailTab, string> = {
  overview: "Overview",
  spec: "Spec",
  runs: "Runs",
  repositories: "Repositories",
  links: "Links",
};

/**
 * Everything about one Item, as the content of a large modal Dialog. Forms
 * render inline in the tab that owns their data; only confirmations open
 * dialogs on top of it.
 */
export function ItemDetailPanel({
  view,
  allItems,
  repositories,
  machines,
  contexts,
  grillModelCatalog,
  tab,
  intent,
  grillDrafts,
  onGrillDraftsChange,
  onTabChange,
  onOpenForm,
  onClose,
  onChanged,
  onOpenTerminal,
  onNotesDirtyChange,
}: {
  view: ItemView;
  allItems: ItemView[];
  repositories: Repository[];
  machines: Machine[];
  contexts: Context[];
  grillModelCatalog: GrillAgentCatalog[];
  tab: ItemDetailTab;
  intent: ItemIntent | undefined;
  grillDrafts: GrillAnswerDrafts;
  onGrillDraftsChange: Dispatch<SetStateAction<GrillAnswerDrafts>>;
  onTabChange: (tab: ItemDetailTab) => void;
  onOpenForm: (form: ItemForm) => void;
  onClose: () => void;
  onChanged: () => Promise<void>;
  onOpenTerminal: (runId: number, pane: PaneTab) => void;
  onNotesDirtyChange: (dirty: boolean) => void;
}) {
  const commands = useItemCommands(onChanged);
  const { isSaving, saveItem } = commands;
  const [isRenaming, setIsRenaming] = useState(false);
  const [focusedQueueRun, setFocusedQueueRun] = useState<{
    runId: number;
    request: number;
  }>();
  const [titleDraft, setTitleDraft] = useState(view.item.title);
  const displayIdentifier = displayItemIdentifier(view.item.human_identifier);
  const signals = itemSignals(view);
  const runsNeedAttention = signals.grillWaiting || signals.runActive;
  const specs = itemSpecs(view);
  // The Spec tab exists only while the Item has a spec.
  const visibleTabs = (Object.keys(tabLabels) as ItemDetailTab[]).filter(
    (value) => value !== "spec" || specs.length > 0,
  );
  const activeTab = visibleTabs.includes(tab) ? tab : "overview";

  useEffect(() => {
    if (!isRenaming) setTitleDraft(view.item.title);
  }, [isRenaming, view.item.title]);

  useFormIntent(intent, headerForms, () => setIsRenaming(true));

  async function handleRename(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const nextTitle = titleDraft.trim();
    if (!nextTitle || nextTitle === view.item.title) return;
    const result = await saveItem(workActions.setItemTitle(view.item.id, nextTitle));
    if (result) setIsRenaming(false);
  }

  function openQueueRun(runId: number) {
    setFocusedQueueRun((current) => ({ runId, request: (current?.request ?? 0) + 1 }));
    onTabChange("runs");
  }

  return (
    <DialogContent
      showCloseButton={false}
      className="flex h-[min(90vh,960px)] w-[calc(100%-2rem)] flex-col gap-0 overflow-hidden p-0 sm:max-w-5xl"
      onInteractOutside={(event) => {
        // A toast's action (like "Answer" on a Grill) must not close the Item.
        const target = event.target as Element | null;
        if (target?.closest("[data-sonner-toaster]")) event.preventDefault();
      }}
      onEscapeKeyDown={(event) => {
        // Escape first leaves title editing; the next one closes the Item.
        if (!isRenaming) return;
        event.preventDefault();
        setIsRenaming(false);
      }}
    >
      <header className="grid gap-2 border-b border-border/70 p-4">
        <div className="flex items-start justify-between gap-2">
          <span className="flex flex-wrap items-center gap-2">
            <Badge variant="outline">{displayIdentifier}</Badge>
            <Badge variant="secondary">{view.item.status}</Badge>
          </span>
          <span className="flex items-center gap-1">
            <ItemActionsMenu
              view={view}
              commands={commands}
              onOpenForm={onOpenForm}
            />
            <Button
              type="button"
              size="icon-sm"
              variant="ghost"
              aria-label="Close Item"
              title="Close (Esc)"
              onClick={onClose}
            >
              <XIcon aria-hidden="true" />
            </Button>
          </span>
        </div>
        {isRenaming ? (
          <form
            className="flex flex-wrap items-center gap-2"
            onSubmit={(event) => void handleRename(event)}
          >
            <DialogTitle className="sr-only">{view.item.title}</DialogTitle>
            <Input
              autoFocus
              aria-label="Item title"
              className="min-w-48 flex-1"
              value={titleDraft}
              onChange={(event) => setTitleDraft(event.target.value)}
              disabled={isSaving}
            />
            <Button
              type="submit"
              size="sm"
              disabled={
                isSaving ||
                !titleDraft.trim() ||
                titleDraft.trim() === view.item.title
              }
            >
              {isSaving ? "Saving…" : "Save"}
            </Button>
            <Button
              type="button"
              size="sm"
              variant="ghost"
              disabled={isSaving}
              onClick={() => setIsRenaming(false)}
            >
              Cancel
            </Button>
          </form>
        ) : (
          <div className="group/title flex items-start gap-1">
            <DialogTitle className="text-lg leading-snug">
              {view.item.title}
            </DialogTitle>
            <Button
              type="button"
              size="icon-xs"
              variant="ghost"
              aria-label="Rename title"
              title="Rename title"
              className="opacity-0 group-hover/title:opacity-100 focus-visible:opacity-100"
              disabled={isSaving}
              onClick={() => setIsRenaming(true)}
            >
              <PencilIcon aria-hidden="true" />
            </Button>
          </div>
        )}
        <DialogDescription>
          {view.context_name} <span>·</span> {view.project_name}
        </DialogDescription>
      </header>
      <Tabs
        value={activeTab}
        onValueChange={(next) => onTabChange(next as ItemDetailTab)}
        className="min-h-0 flex-1 gap-0"
      >
        <TabsList className="mx-4 mt-3">
          {visibleTabs.map((value) => (
            <TabsTrigger value={value} key={value}>
              {tabLabels[value]}
              {value === "runs" && runsNeedAttention && (
                <span
                  className={cn(
                    "size-1.5 rounded-full",
                    signals.grillWaiting ? "bg-amber-500" : "bg-primary",
                  )}
                  aria-label={
                    signals.grillWaiting
                      ? "Grill waiting for answers"
                      : "Run active"
                  }
                />
              )}
            </TabsTrigger>
          ))}
        </TabsList>
        <div className="min-h-0 flex-1 overflow-y-auto p-4">
          {/* Tabs stay mounted so drafts survive switching between them. */}
          <TabsContent value="overview" forceMount className="data-[state=inactive]:hidden">
            <OverviewTab
              view={view}
              allItems={allItems}
              commands={commands}
              intent={intent}
              onNotesDirtyChange={onNotesDirtyChange}
            />
          </TabsContent>
          {specs.length > 0 && (
            // Not kept mounted: it reads GitHub, so it loads only when opened.
            <TabsContent value="spec">
              <SpecTab view={view} specs={specs} repositories={repositories} contexts={contexts} modelCatalog={grillModelCatalog} commands={commands} onOpenRun={openQueueRun} />
            </TabsContent>
          )}
          <TabsContent value="runs" forceMount className="data-[state=inactive]:hidden">
            <RunsTab
              view={view}
              repositories={repositories}
              machines={machines}
              contexts={contexts}
              grillModelCatalog={grillModelCatalog}
              commands={commands}
              intent={intent}
              grillDrafts={grillDrafts}
              onGrillDraftsChange={onGrillDraftsChange}
              onOpenTerminal={onOpenTerminal}
              focusedRunRequest={focusedQueueRun}
            />
          </TabsContent>
          <TabsContent value="repositories" forceMount className="data-[state=inactive]:hidden">
            <RepositoriesTab
              view={view}
              repositories={repositories}
              machines={machines}
              contexts={contexts}
              commands={commands}
            />
          </TabsContent>
          <TabsContent value="links" forceMount className="data-[state=inactive]:hidden">
            <LinksTab
              view={view}
              repositories={repositories}
              commands={commands}
              intent={intent}
            />
          </TabsContent>
        </div>
      </Tabs>
      {commands.confirmationDialog}
    </DialogContent>
  );
}
