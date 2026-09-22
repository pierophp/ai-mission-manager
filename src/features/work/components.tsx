import { type FormEvent, type KeyboardEvent, useEffect, useState } from "react";
import { Alert, AlertDescription, AlertTitle } from "../../components/ui/alert";
import { Badge } from "../../components/ui/badge";
import { Button } from "../../components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "../../components/ui/card";
import { Checkbox } from "../../components/ui/checkbox";
import { ConfirmationDialog } from "../../components/ui/confirmation-dialog";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../../components/ui/dialog";
import { Input } from "../../components/ui/input";
import {
  NativeSelect,
  NativeSelectOption,
} from "../../components/ui/native-select";
import { Textarea } from "../../components/ui/textarea";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "../../components/ui/dropdown-menu";
import { EllipsisVerticalIcon } from "lucide-react";
import { currentMinute } from "../../runtime/time";
import { errorMessage } from "../../runtime/errors";
import type {
  AgentKind,
  AttentionEntry,
  Context,
  DirectRunPreview,
  ExecutionProfile,
  ExternalChangePolicy,
  ExternalLinkView,
  ExternalObjectDeletionPreview,
  ItemStatus,
  ItemDeletionPreview,
  ItemRelationKind,
  ItemView,
  Machine,
  GrillAnswer,
  GrillAgentCatalog,
  GrillConfiguration,
  Repository,
  Run,
  RunPromptSelection,
  RunState,
  RunSuggestion,
  Workspace,
  WorkspaceRemovalReport,
  WorkspaceRepositoryInput,
} from "../../runtime/types";
import type { PaneTab } from "../../runtime/terminal-types";
import type { GrillContinuationAction } from "../../runtime/execution-types";
import {
  externalObjectKindLabel,
  formatSnapshotAge,
  grillPhaseLabel,
  paneTabForRun,
  relationKindLabel,
  relationshipLabel,
  repositoryName,
} from "./work-utils";
import { type WorkAction, useWorkCommand, workActions } from "./work-mutations";

const itemStatuses: ItemStatus[] = ["Inbox", "Active", "Waiting", "Done"];
const relationKinds: ItemRelationKind[] = ["Blocks", "BlockedBy", "RelatedTo"];

function displayItemIdentifier(identifier: string): string {
  const match = /^MC-(\d+)$/.exec(identifier);
  return match ? `#${match[1]}` : identifier;
}

function isRunFinished(run: Run): boolean {
  return (
    run.state === "finished" &&
    (run.execution_profile !== "grill" || run.grill_phase === "finished")
  );
}

type WorkConfirmation = {
  title: string;
  description: string;
  confirmLabel: string;
  onConfirm: () => void;
};

export function HomeColumn({
  title,
  hint,
  items,
  allItems,
  repositories,
  machines,
  contexts,
  grillModelCatalog,
  onChanged,
  onOpenTerminal,
}: {
  title: string;
  hint: string;
  items: ItemView[];
  allItems: ItemView[];
  repositories: Repository[];
  machines: Machine[];
  contexts: Context[];
  grillModelCatalog: GrillAgentCatalog[];
  onChanged: () => Promise<void>;
  onOpenTerminal: (runId: number, pane: PaneTab) => void;
}) {
  return (
    <section className="min-w-0 space-y-3" aria-labelledby={`${title}-heading`}>
      <div className="flex items-start justify-between gap-3">
        <div>
          <h3
            id={`${title}-heading`}
            className="font-heading text-base font-medium normal-case tracking-normal text-foreground"
          >
            {title}
          </h3>
          <span className="text-xs text-muted-foreground">{hint}</span>
        </div>
        <Badge variant="secondary">{items.length}</Badge>
      </div>
      {items.length === 0 ? (
        <p className="rounded-lg border border-dashed p-4 text-sm text-muted-foreground">
          Nothing here.
        </p>
      ) : (
        <div className="grid gap-3">
          {items.map((view) => (
            <ItemCard
              key={view.item.id}
              view={view}
              allItems={allItems}
              repositories={repositories}
              machines={machines}
              contexts={contexts}
              grillModelCatalog={grillModelCatalog}
              onChanged={onChanged}
              onOpenTerminal={onOpenTerminal}
            />
          ))}
        </div>
      )}
    </section>
  );
}

export function ItemCard({
  view,
  allItems,
  repositories,
  machines,
  contexts,
  grillModelCatalog,
  onChanged,
  onOpenTerminal,
}: {
  view: ItemView;
  allItems: ItemView[];
  repositories: Repository[];
  machines: Machine[];
  contexts: Context[];
  grillModelCatalog: GrillAgentCatalog[];
  onChanged: () => Promise<void>;
  onOpenTerminal: (runId: number, pane: PaneTab) => void;
}) {
  const [notes, setNotes] = useState(view.item.notes);
  const [reminderAt, setReminderAt] = useState("");
  const [relationKind, setRelationKind] = useState<ItemRelationKind>("Blocks");
  const [targetItemId, setTargetItemId] = useState<number>();
  const [externalUrl, setExternalUrl] = useState("");
  const [isIssuePreviewOpen, setIsIssuePreviewOpen] = useState(false);
  const [isReminderDialogOpen, setIsReminderDialogOpen] = useState(false);
  const [isRenameDialogOpen, setIsRenameDialogOpen] = useState(false);
  const [titleDraft, setTitleDraft] = useState(view.item.title);
  const [issueRepository, setIssueRepository] = useState("");
  const [issueTitle, setIssueTitle] = useState(view.item.title);
  const [issueBody, setIssueBody] = useState(view.item.notes);
  const [workspaceRepositoryIds, setWorkspaceRepositoryIds] = useState<
    number[]
  >([]);
  const [workspaceBranches, setWorkspaceBranches] = useState<
    Record<number, string>
  >({});
  const [workspaceBaseBranches, setWorkspaceBaseBranches] = useState<
    Record<number, string>
  >({});
  const [deletionPreview, setDeletionPreview] = useState<ItemDeletionPreview>();
  const [confirmation, setConfirmation] = useState<WorkConfirmation>();
  const [externalObjectDeletionPreview, setExternalObjectDeletionPreview] =
    useState<ExternalObjectDeletionPreview>();
  const [runAgent, setRunAgent] = useState<AgentKind>("claude");
  const [runProfile, setRunProfile] = useState<ExecutionProfile>("implement");
  const [includeRunObjective, setIncludeRunObjective] = useState(true);
  const [includeRunNotes, setIncludeRunNotes] = useState(false);
  const [selectedRunExternalObjectIds, setSelectedRunExternalObjectIds] =
    useState<number[]>([]);
  const [runCustomPrompt, setRunCustomPrompt] = useState("");
  const [runPrompt, setRunPrompt] = useState("");
  const [runPromptNeedsCompose, setRunPromptNeedsCompose] = useState(false);
  const [directRunWorkspaceId, setDirectRunWorkspaceId] = useState<number>();
  const [directRunMachineId, setDirectRunMachineId] = useState<number>();
  const [directRunRepositoryId, setDirectRunRepositoryId] = useState<number>();
  const [directRunPreview, setDirectRunPreview] =
    useState<DirectRunPreview>();
  const [directRunDirtyConfirmed, setDirectRunDirtyConfirmed] =
    useState(false);
  const [directRunSharedConfirmed, setDirectRunSharedConfirmed] =
    useState(false);
  const [isGrillRunOpen, setIsGrillRunOpen] = useState(false);
  const [grillRunWorkspaceId, setGrillRunWorkspaceId] = useState<number>();
  const [grillRunMachineId, setGrillRunMachineId] = useState<number>();
  const [grillRunRepositoryId, setGrillRunRepositoryId] = useState<number>();
  const [grillRunPreview, setGrillRunPreview] = useState<DirectRunPreview>();
  const [grillRunPreviewError, setGrillRunPreviewError] = useState<string>();
  const [grillAgent, setGrillAgent] = useState<GrillConfiguration["agent"]>("claude");
  const [grillModel, setGrillModel] = useState("claude-sonnet-4-5");
  const [grillEffort, setGrillEffort] = useState("high");
  const [grillInitialPrompt, setGrillInitialPrompt] = useState("");
  const [grillPromptPreview, setGrillPromptPreview] = useState("");
  const [grillDirtyConfirmed, setGrillDirtyConfirmed] = useState(false);
  const [grillSharedConfirmed, setGrillSharedConfirmed] = useState(false);
  const [worktreeMachineId, setWorktreeMachineId] = useState<number>();
  const [worktreePathDrafts, setWorktreePathDrafts] = useState<
    Record<string, string>
  >({});
  const [worktreeRunWorkspaceId, setWorktreeRunWorkspaceId] = useState<number>();
  const [worktreeRunWorktreeId, setWorktreeRunWorktreeId] = useState<number>();
  const [worktreeRunPrompt, setWorktreeRunPrompt] = useState("");
  const [worktreeRunPromptNeedsCompose, setWorktreeRunPromptNeedsCompose] =
    useState(false);
  const [workspaceRemovalReport, setWorkspaceRemovalReport] =
    useState<WorkspaceRemovalReport>();
  const [workspaceRemovalSelection, setWorkspaceRemovalSelection] = useState<
    Record<number, boolean>
  >({});
  const [workspaceRemovalDestructive, setWorkspaceRemovalDestructive] =
    useState<Record<number, boolean>>({});
  const [isSaving, setIsSaving] = useState(false);
  const [isExpanded, setIsExpanded] = useState(false);
  const workCommand = useWorkCommand();

  const displayIdentifier = displayItemIdentifier(view.item.human_identifier);

  const itemRepositories = repositories.filter(
    (repository) => repository.project_id === view.item.project_id,
  );
  const activeGrillRun = view.runs.find(
    (run) => run.execution_profile === "grill" && !isRunFinished(run),
  );
  const selectedGrillCatalog = grillModelCatalog.find(
    (catalog) => catalog.agent === grillAgent,
  );
  const selectedGrillModel = selectedGrillCatalog?.models.find(
    (model) => model.id === grillModel,
  );

  useEffect(() => {
    setNotes(view.item.notes);
  }, [view.item.notes]);

  useEffect(() => {
    setTitleDraft(view.item.title);
  }, [view.item.title]);

  function openGrillStart() {
    const context = contexts.find((candidate) => candidate.id === view.context_id);
    const defaults = context?.grill_defaults;
    setIsExpanded(true);
    setIsGrillRunOpen(true);
    setGrillRunWorkspaceId(undefined);
    setGrillRunMachineId(undefined);
    setGrillRunRepositoryId(undefined);
    setGrillRunPreview(undefined);
    setGrillRunPreviewError(undefined);
    setGrillAgent(defaults?.agent ?? "claude");
    setGrillModel(defaults?.model ?? "claude-sonnet-4-5");
    setGrillEffort(defaults?.effort ?? "high");
    setGrillInitialPrompt("");
    setGrillPromptPreview("");
    setGrillDirtyConfirmed(false);
    setGrillSharedConfirmed(false);
  }

  async function refreshGrillRunPreview(
    workspaceId: number | undefined,
    machineId: number | undefined,
  ) {
    setGrillRunWorkspaceId(workspaceId);
    setGrillRunMachineId(machineId);
    setGrillRunRepositoryId(undefined);
    setGrillRunPreview(undefined);
    setGrillRunPreviewError(undefined);
    setGrillDirtyConfirmed(false);
    setGrillSharedConfirmed(false);
    if (!workspaceId) return;

    setIsSaving(true);
    try {
      const preview = await workCommand.execute(
        workActions.prepareGrillRun(view.item.id, workspaceId, machineId ?? null),
        false,
      );
      setGrillRunPreview(preview);
      setGrillRunRepositoryId(
        preview.checkoutDetails.length === 1
          ? preview.checkoutDetails[0].repositoryId
          : undefined,
      );
    } catch (previewError) {
      const message = errorMessage(previewError);
      setGrillRunPreviewError(message);
      window.alert(message);
    } finally {
      setIsSaving(false);
    }
  }

  async function composeGrillPromptPreview() {
    if (!grillInitialPrompt.trim() || !selectedGrillModel) return;
    setIsSaving(true);
    try {
      const prompt = await workCommand.execute(
        workActions.composeGrillPrompt(view.item.id, {
          agent: grillAgent,
          model: grillModel,
          effort: grillEffort,
        }, grillInitialPrompt),
        false,
      );
      setGrillPromptPreview(prompt);
    } catch (composeError) {
      window.alert(errorMessage(composeError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleStartGrillRun(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (
      !grillRunWorkspaceId ||
      !grillRunPreview ||
      !grillRunRepositoryId ||
      !selectedGrillModel ||
      !grillInitialPrompt.trim()
    ) {
      return;
    }
    const dirtyConfirmed =
      grillRunPreview.dirtyRepositoryIds.length === 0 || grillDirtyConfirmed;
    const sharedConfirmed =
      grillRunPreview.sharedPaths.length === 0 || grillSharedConfirmed;
    if (!dirtyConfirmed || !sharedConfirmed) return;

    const started = await saveItem(
      workActions.startGrillRun({
        itemId: view.item.id,
        workspaceId: grillRunWorkspaceId,
        primaryRepositoryId: grillRunRepositoryId,
        machineId: grillRunMachineId ?? null,
        configuration: {
          agent: grillAgent,
          model: grillModel,
          effort: grillEffort,
        },
        initialPrompt: grillInitialPrompt,
        expectedCheckouts: grillRunPreview.checkouts,
        allowDirty: grillRunPreview.dirtyRepositoryIds.length > 0,
        allowSharedCheckouts: grillRunPreview.sharedPaths.length > 0,
      }),
    );
    if (!started) return;
    setIsGrillRunOpen(false);
    setGrillRunWorkspaceId(undefined);
    setGrillRunPreview(undefined);
    setGrillPromptPreview("");
  }

  async function handleContinueGrill(
    run: Run,
    action: GrillContinuationAction,
  ) {
    await saveItem(workActions.continueGrill(run.id, action));
  }

  async function saveItem<TData>(
    update: WorkAction<TData>,
    invalidate = true,
  ): Promise<TData | undefined> {
    setIsSaving(true);
    try {
      const result = await workCommand.execute(update, invalidate);
      await onChanged();
      return result;
    } catch (saveError) {
      window.alert(errorMessage(saveError));
      return undefined;
    } finally {
      setIsSaving(false);
    }
  }

  async function handleRelation(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!targetItemId) return;
    await saveItem(
      workActions.setRelation(view.item.id, targetItemId, relationKind),
    );
    setTargetItemId(undefined);
  }

  async function handleExternalLink(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!externalUrl.trim()) return;
    try {
      setIsSaving(true);
      const result = await workCommand.execute(
        workActions.linkExternalObject(view.item.id, externalUrl),
      );
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

  async function handleCreateWorkspace(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (workspaceRepositoryIds.length === 0) return;

    const selected: WorkspaceRepositoryInput[] = workspaceRepositoryIds.map(
      (repositoryId) => ({
        repositoryId,
        branch: workspaceBranches[repositoryId]?.trim() ?? "",
        baseBranch: workspaceBaseBranches[repositoryId]?.trim() ?? "",
      }),
    );
    if (
      selected.some(
        (repository) => !repository.branch || !repository.baseBranch,
      )
    ) {
      return;
    }

    await saveItem(workActions.createWorkspace(view.item.id, selected));
    setWorkspaceRepositoryIds([]);
    setWorkspaceBranches({});
    setWorkspaceBaseBranches({});
  }

  function handleStopRun(run: Run) {
    setConfirmation({
      title: `Stop Run #${run.id}?`,
      description:
        "This is separate from completing the Item and will leave the Run in its history.",
      confirmLabel: "Stop Run",
      onConfirm: () => {
        setConfirmation(undefined);
        void saveItem(workActions.stopRun(run.id));
      },
    });
  }

  function handleFinishRun(run: Run) {
    setConfirmation({
      title: `Finish Run #${run.id}?`,
      description:
        "This records an explicit Run completion, keeps its transcript and answers in history, and leaves the Item status unchanged.",
      confirmLabel: "Finish Run",
      onConfirm: () => {
        setConfirmation(undefined);
        void saveItem(workActions.finishRun(run.id));
      },
    });
  }

  function handleDeleteRun(run: Run) {
    if (!isRunFinished(run)) return;
    setConfirmation({
      title: `Delete finished Run #${run.id}?`,
      description: "This removes the Run from history and cannot be undone.",
      confirmLabel: "Delete Run",
      onConfirm: () => {
        setConfirmation(undefined);
        void saveItem(workActions.deleteRun(run.id));
      },
    });
  }

  async function handlePrepareItemDeletion() {
    setIsSaving(true);
    try {
      const preview = await workCommand.execute(
        workActions.prepareItemDeletion(view.item.id),
        false,
      );
      setDeletionPreview(preview);
    } catch (previewError) {
      window.alert(errorMessage(previewError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleAddReminder() {
    if (!reminderAt) return;
    const result = await saveItem(
      workActions.addReminder(view.item.id, reminderAt),
    );
    if (!result) return;
    setReminderAt("");
    setIsReminderDialogOpen(false);
  }

  async function handleRenameTitle() {
    const nextTitle = titleDraft.trim();
    if (!nextTitle || nextTitle === view.item.title) return;
    const result = await saveItem(
      workActions.setItemTitle(view.item.id, nextTitle),
    );
    if (!result) return;
    setTitleDraft(result.title);
    setIsRenameDialogOpen(false);
  }

  async function handleDeleteItem() {
    if (!deletionPreview || deletionPreview.blockers.length > 0) return;
    setIsSaving(true);
    try {
      const result = await workCommand.execute(
        workActions.deleteItem(view.item.id),
      );
      setDeletionPreview(undefined);
      await onChanged();
      const summary = result.summary;
      window.alert(
        `Deleted ${displayItemIdentifier(deletionPreview.plan.humanIdentifier)}.\n\nRemoved ${summary.reminderCount} reminder(s), ${summary.relationshipCount} relationship(s), ${summary.workspaceCount} Workspace(s), ${summary.runCount} Run(s), ${summary.linkCount} Link(s), and ${summary.externalObjectCount} orphaned External Object(s).`,
      );
    } catch (deleteError) {
      setDeletionPreview(undefined);
      window.alert(errorMessage(deleteError));
    } finally {
      setIsSaving(false);
    }
  }

  async function executeUnlinkExternalLink(linkId: number) {
    const result = await saveItem(workActions.unlinkExternalLink(linkId));
    if (!result) return;
    if (result.externalObjectDeleted) {
      window.alert(
        "The Link was removed. It was the last Link, so its local External Object snapshot and Activity cache were also removed. The provider-owned object was not deleted.",
      );
    }
  }

  function handleUnlinkExternalLink(linkId: number) {
    setConfirmation({
      title: "Remove this Link from the Item?",
      description:
        "Its Link-scoped attention state will be removed. The GitHub Issue, pull request, or other provider-owned object will not be deleted.",
      confirmLabel: "Remove Link",
      onConfirm: () => {
        setConfirmation(undefined);
        void executeUnlinkExternalLink(linkId);
      },
    });
  }

  async function handlePrepareExternalObjectDeletion(externalObjectId: number) {
    setIsSaving(true);
    try {
      const preview = await workCommand.execute(
        workActions.prepareExternalObjectDeletion(externalObjectId),
        false,
      );
      setExternalObjectDeletionPreview(preview);
    } catch (previewError) {
      window.alert(errorMessage(previewError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleDeleteExternalObject() {
    if (!externalObjectDeletionPreview) return;
    const { plan } = externalObjectDeletionPreview;
    setConfirmation({
      title: `Remove this ${externalObjectKindLabel(plan.kind)} locally?`,
      description: `This will remove ${plan.linkIds.length} Link(s), ${plan.snapshotCount} snapshot(s), and ${plan.activityCount} Activity record(s). Provider-owned objects are never deleted.`,
      confirmLabel: "Remove locally",
      onConfirm: () => {
        setConfirmation(undefined);
        void executeDeleteExternalObject(plan.externalObjectId);
      },
    });
  }

  async function executeDeleteExternalObject(externalObjectId: number) {
    setIsSaving(true);
    try {
      const result = await workCommand.execute(
        workActions.deleteExternalObject(externalObjectId),
      );
      setExternalObjectDeletionPreview(undefined);
      await onChanged();
      window.alert(
        `Removed the External Object from Mission Manager. Removed ${result.summary.linkCount} Link(s), ${result.summary.snapshotCount} snapshot(s), and ${result.summary.activityCount} Activity record(s). Provider-owned objects were not deleted.`,
      );
    } catch (deleteError) {
      setExternalObjectDeletionPreview(undefined);
      window.alert(errorMessage(deleteError));
    } finally {
      setIsSaving(false);
    }
  }

  function handleItemStatusChange(nextStatus: ItemStatus) {
    if (nextStatus === "Done" && view.item.status !== "Done") {
      const activeRuns = view.runs.filter(
        (run) => !isRunFinished(run) && run.pane_status !== "missing",
      );
      if (activeRuns.length > 0) {
        setConfirmation({
          title: "Complete this Item with active Runs?",
          description: `${activeRuns.length} Run${activeRuns.length === 1 ? " is" : "s are"} still active. Completing the Item will not stop them.`,
          confirmLabel: "Complete Item",
          onConfirm: () => {
            setConfirmation(undefined);
            void saveItem(
              workActions.setItemStatus(view.item.id, nextStatus),
            );
          },
        });
        return;
      }
    }
    void saveItem(workActions.setItemStatus(view.item.id, nextStatus));
  }

  function runPromptSelection(): RunPromptSelection {
    return {
      includeObjective: includeRunObjective,
      includeNotes: includeRunNotes,
      externalObjectIds: selectedRunExternalObjectIds,
    };
  }

  async function composeRunPromptPreview() {
    if (!directRunWorkspaceId) return;
    setIsSaving(true);
    try {
      const composed = await workCommand.execute(
        workActions.composeRunPrompt(
          view.item.id,
          runProfile,
          runPromptSelection(),
          runProfile === "custom" ? runCustomPrompt : null,
        ),
        false,
      );
      setRunPrompt(composed);
      setRunPromptNeedsCompose(false);
    } catch (composeError) {
      window.alert(errorMessage(composeError));
    } finally {
      setIsSaving(false);
    }
  }

  async function openDirectRunPreview(workspace: Workspace) {
    setDirectRunWorkspaceId(workspace.id);
    setDirectRunMachineId(undefined);
    setDirectRunRepositoryId(undefined);
    setDirectRunPreview(undefined);
    setDirectRunDirtyConfirmed(false);
    setDirectRunSharedConfirmed(false);
    setRunAgent("claude");
    setRunProfile("implement");
    setIncludeRunObjective(true);
    setIncludeRunNotes(Boolean(view.item.notes.trim()));
    setSelectedRunExternalObjectIds([]);
    setRunCustomPrompt("");
    setIsSaving(true);
    try {
      const [prompt, preview] = await Promise.all([
        workCommand.execute(
          workActions.composeRunPrompt(
            view.item.id,
            "implement",
            {
              includeObjective: true,
              includeNotes: Boolean(view.item.notes.trim()),
              externalObjectIds: [],
            },
            null,
          ),
          false,
        ),
        workCommand.execute(
          workActions.prepareDirectRun(view.item.id, workspace.id, null),
          false,
        ),
      ]);
      setRunPrompt(prompt);
      setRunPromptNeedsCompose(false);
      setDirectRunPreview(preview);
    } catch (previewError) {
      setDirectRunWorkspaceId(undefined);
      window.alert(errorMessage(previewError));
    } finally {
      setIsSaving(false);
    }
  }

  async function openWorktreeRun(workspace: Workspace, worktreeId: number) {
    setWorktreeRunWorkspaceId(workspace.id);
    setWorktreeRunWorktreeId(worktreeId);
    setRunAgent("claude");
    setRunProfile("implement");
    setIncludeRunObjective(true);
    setIncludeRunNotes(Boolean(view.item.notes.trim()));
    setSelectedRunExternalObjectIds([]);
    setRunCustomPrompt("");
    setWorktreeRunPrompt("");
    setWorktreeRunPromptNeedsCompose(true);
    setIsSaving(true);
    try {
      const prompt = await workCommand.execute(
        workActions.composeRunPrompt(
          view.item.id,
          "implement",
          {
            includeObjective: true,
            includeNotes: Boolean(view.item.notes.trim()),
            externalObjectIds: [],
          },
          null,
        ),
        false,
      );
      setWorktreeRunPrompt(prompt);
      setWorktreeRunPromptNeedsCompose(false);
    } catch (promptError) {
      setWorktreeRunWorkspaceId(undefined);
      setWorktreeRunWorktreeId(undefined);
      window.alert(errorMessage(promptError));
    } finally {
      setIsSaving(false);
    }
  }

  async function composeWorktreeRunPrompt() {
    if (!worktreeRunWorkspaceId) return;
    setIsSaving(true);
    try {
      const composed = await workCommand.execute(
        workActions.composeRunPrompt(
          view.item.id,
          runProfile,
          runPromptSelection(),
          runProfile === "custom" ? runCustomPrompt : null,
        ),
        false,
      );
      setWorktreeRunPrompt(composed);
      setWorktreeRunPromptNeedsCompose(false);
    } catch (composeError) {
      window.alert(errorMessage(composeError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleStartWorktreeRun(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (
      !worktreeRunWorkspaceId ||
      !worktreeRunWorktreeId ||
      !worktreeRunPrompt.trim() ||
      worktreeRunPromptNeedsCompose
    ) {
      return;
    }
    await saveItem(
      workActions.startWorktreeRun({
        itemId: view.item.id,
        workspaceId: worktreeRunWorkspaceId,
        worktreeId: worktreeRunWorktreeId,
        agent: runAgent,
        executionProfile: runProfile,
        prompt: worktreeRunPrompt,
        promptSelection: runPromptSelection(),
      }),
    );
    setWorktreeRunWorkspaceId(undefined);
    setWorktreeRunWorktreeId(undefined);
    setWorktreeRunPrompt("");
  }

  async function handleStartDirectRun(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (
      !directRunWorkspaceId ||
      !directRunPreview ||
      !directRunRepositoryId ||
      !runPrompt.trim() ||
      runPromptNeedsCompose
    ) {
      return;
    }
    const dirtyConfirmed =
      directRunPreview.dirtyRepositoryIds.length === 0 ||
      directRunDirtyConfirmed;
    const sharedConfirmed =
      directRunPreview.sharedPaths.length === 0 || directRunSharedConfirmed;
    if (!dirtyConfirmed || !sharedConfirmed) return;

    await saveItem(
      workActions.startDirectRun({
        itemId: view.item.id,
        workspaceId: directRunWorkspaceId,
        primaryRepositoryId: directRunRepositoryId,
        machineId: directRunMachineId ?? null,
        agent: runAgent,
        executionProfile: runProfile,
        prompt: runPrompt,
        promptSelection: runPromptSelection(),
        expectedCheckouts: directRunPreview.checkouts,
        allowDirty: directRunPreview.dirtyRepositoryIds.length > 0,
        allowSharedCheckouts: directRunPreview.sharedPaths.length > 0,
      }),
    );
    setDirectRunWorkspaceId(undefined);
    setDirectRunPreview(undefined);
    setRunPrompt("");
  }

  async function refreshDirectRunPreview(machineId: number | undefined) {
    if (!directRunWorkspaceId) return;
    setDirectRunMachineId(machineId);
    setDirectRunDirtyConfirmed(false);
    setDirectRunSharedConfirmed(false);
    setIsSaving(true);
    try {
      const preview = await workCommand.execute(
        workActions.prepareDirectRun(
          view.item.id,
          directRunWorkspaceId,
          machineId ?? null,
        ),
        false,
      );
      setDirectRunPreview(preview);
    } catch (previewError) {
      window.alert(errorMessage(previewError));
    } finally {
      setIsSaving(false);
    }
  }

  async function prepareWorkspaceWorktree(
    workspace: Workspace,
    repositoryId: number,
    reuseExistingBranch: boolean,
  ) {
    const machineId = worktreeMachineId ?? itemMachines[0]?.id;
    if (!machineId) {
      window.alert("Configure a Machine-specific Repository checkout first.");
      return;
    }
    const confirmDirtyAttachment = reuseExistingBranch
      ? window.confirm(
          "Reuse this branch and attach the Git Worktree? If it is dirty, existing files will be preserved.",
        )
      : false;
    await saveItem(
      workActions.prepareWorktree(
        workspace.id,
        repositoryId,
        machineId,
        reuseExistingBranch,
        confirmDirtyAttachment,
      ),
    );
  }

  async function attachExistingWorkspaceWorktree(
    workspace: Workspace,
    repositoryId: number,
  ) {
    const machineId = worktreeMachineId ?? itemMachines[0]?.id;
    if (!machineId) {
      window.alert("Configure a Machine-specific Repository checkout first.");
      return;
    }
    const draftKey = `${workspace.id}:${repositoryId}`;
    const path = worktreePathDrafts[draftKey]?.trim();
    if (!path) {
      window.alert("Enter the existing Worktree path first.");
      return;
    }
    const confirmDirtyAttachment = window.confirm(
      "Attach this existing Git Worktree? This confirms the attachment and allows preserving existing dirty files.",
    );
    if (!confirmDirtyAttachment) return;
    await saveItem(
      workActions.attachWorktree(
        workspace.id,
        repositoryId,
        machineId,
        path,
        confirmDirtyAttachment,
      ),
    );
    setWorktreePathDrafts((current) => ({ ...current, [draftKey]: "" }));
  }

  async function reviewWorkspaceRemoval(workspaceId: number) {
    const report = await saveItem(
      workActions.prepareWorkspaceRemoval(workspaceId),
      false,
    );
    if (!report) return;
    setWorkspaceRemovalReport(report);
    setWorkspaceRemovalSelection(
      Object.fromEntries(report.worktrees.map((worktree) => [worktree.worktreeId, false])),
    );
    setWorkspaceRemovalDestructive({});
  }

  async function removeWorkspace() {
    if (!workspaceRemovalReport) return;
    const confirmedWorktreeIds = workspaceRemovalReport.worktrees
      .filter((worktree) => workspaceRemovalSelection[worktree.worktreeId])
      .map((worktree) => worktree.worktreeId);
    const destructiveWorktreeIds = workspaceRemovalReport.worktrees
      .filter((worktree) => workspaceRemovalDestructive[worktree.worktreeId])
      .map((worktree) => worktree.worktreeId);
    await saveItem(
      workActions.removeWorkspace(
        workspaceRemovalReport.workspaceId,
        confirmedWorktreeIds,
        destructiveWorktreeIds,
      ),
    );
    setWorkspaceRemovalReport(undefined);
  }

  async function refreshExternalObject(externalObjectId: number) {
    setIsSaving(true);
    try {
      await workCommand.execute(
        workActions.refreshExternalObject(externalObjectId),
      );
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
      const result = await workCommand.execute(
        workActions.createGithubIssue(
          view.item.id,
          issueRepository,
          issueTitle,
          issueBody,
        ),
      );
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

  function renderWorkspaceCard(workspace: Workspace) {
    const workspaceWorktrees = view.worktrees.filter(
      (worktree) => worktree.workspace_id === workspace.id,
    );

    return (
      <Card size="sm" key={workspace.id}>
        <CardHeader className="border-b border-border/70">
          <CardTitle className="text-sm">Workspace #{workspace.id}</CardTitle>
          <CardDescription>
            Persistent logical grouping for later Runs. It has no shared root
            directory.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-3 pt-4">
          <div className="flex flex-wrap gap-2">
            {workspace.repositories.map((selected) => (
              <Badge
                variant="secondary"
                className="h-auto items-start gap-1 py-1"
                key={selected.repository_id}
              >
                <span className="font-medium">
                  {repositoryName(repositories, selected.repository_id)}
                </span>
                <span className="text-muted-foreground">
                  {selected.branch} · base {selected.base_branch}
                </span>
              </Badge>
            ))}
          </div>
          <Button
            type="button"
            size="sm"
            disabled={isSaving}
            onClick={() => void openDirectRunPreview(workspace)}
          >
            Start Direct Run
          </Button>
          <div className="grid gap-2 rounded-md border p-3">
            <label className="grid gap-1.5 text-sm font-medium md:max-w-sm">
              <span>Worktree Machine</span>
              <NativeSelect
                value={worktreeMachineId ?? ""}
                onChange={(event) =>
                  setWorktreeMachineId(Number(event.target.value) || undefined)
                }
                disabled={isSaving}
              >
                <NativeSelectOption value="">
                  First configured Machine
                </NativeSelectOption>
                {itemMachines.map((machine) => (
                  <NativeSelectOption value={machine.id} key={machine.id}>
                    {machine.name} · {machine.last_observed}
                  </NativeSelectOption>
                ))}
              </NativeSelect>
            </label>
            {workspace.repositories.map((selected) => {
              const draftKey = `${workspace.id}:${selected.repository_id}`;
              const existingWorktree = workspaceWorktrees.find(
                (worktree) => worktree.repository_id === selected.repository_id,
              );
              return (
                <div className="grid gap-2 text-sm" key={selected.repository_id}>
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <span>
                      {repositoryName(repositories, selected.repository_id)} ·{" "}
                      <code>{selected.branch}</code>
                    </span>
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      disabled={isSaving || Boolean(existingWorktree)}
                      onClick={() =>
                        void prepareWorkspaceWorktree(
                          workspace,
                          selected.repository_id,
                          false,
                        )
                      }
                    >
                      Create Worktree
                    </Button>
                  </div>
                  {!existingWorktree && (
                    <div className="flex flex-wrap items-end gap-2">
                      <label className="grid min-w-64 flex-1 gap-1.5 text-xs font-medium">
                        <span>Existing Worktree path</span>
                        <Input
                          value={worktreePathDrafts[draftKey] ?? ""}
                          onChange={(event) =>
                            setWorktreePathDrafts((current) => ({
                              ...current,
                              [draftKey]: event.target.value,
                            }))
                          }
                          placeholder="/path/to/existing/worktree"
                          disabled={isSaving}
                        />
                      </label>
                      <Button
                        type="button"
                        size="sm"
                        variant="ghost"
                        disabled={isSaving || !worktreePathDrafts[draftKey]?.trim()}
                        onClick={() =>
                          void attachExistingWorkspaceWorktree(
                            workspace,
                            selected.repository_id,
                          )
                        }
                      >
                        Attach existing Worktree
                      </Button>
                    </div>
                  )}
                </div>
              );
            })}
          </div>
          {workspaceWorktrees.length > 0 && (
            <div className="grid gap-2 rounded-md border border-primary/30 p-3">
              <p className="m-0 text-sm font-medium">Registered Worktrees</p>
              {workspaceWorktrees.map((worktree) => (
                <div
                  className="flex flex-wrap items-center justify-between gap-2 text-sm"
                  key={worktree.id}
                >
                  <div className="grid gap-1">
                    <span>
                      {repositoryName(repositories, worktree.repository_id)} ·{" "}
                      <code>{worktree.branch}</code>
                    </span>
                    <code className="break-all text-xs text-muted-foreground">
                      {worktree.path}
                    </code>
                  </div>
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={isSaving}
                    onClick={() => void openWorktreeRun(workspace, worktree.id)}
                  >
                    Start Worktree Run
                  </Button>
                </div>
              ))}
            </div>
          )}
          <Button
            type="button"
            size="sm"
            variant="outline"
            disabled={isSaving}
            onClick={() => void reviewWorkspaceRemoval(workspace.id)}
          >
            Review Worktree cleanup
          </Button>
          {workspaceRemovalReport?.workspaceId === workspace.id && (
            <div className="grid gap-3 rounded-lg border border-destructive/30 p-4">
              <div>
                <h4 className="m-0 text-base font-medium">
                  Confirm each Worktree to remove
                </h4>
                <p className="mt-1 text-sm text-muted-foreground">
                  Physical cleanup is explicit. Git branches are preserved.
                </p>
              </div>
              {workspaceRemovalReport.blockers.length > 0 && (
                <Alert variant="destructive">
                  <AlertTitle>Workspace cleanup is blocked</AlertTitle>
                  <AlertDescription>
                    {workspaceRemovalReport.blockers.map((blocker) => (
                      <div key={blocker}>{blocker}</div>
                    ))}
                  </AlertDescription>
                </Alert>
              )}
              {workspaceRemovalReport.worktrees.map((worktree) => (
                <label
                  className="grid gap-2 rounded-md border p-3 text-sm"
                  key={worktree.worktreeId}
                >
                  <span className="flex items-center gap-2 font-medium">
                    <Checkbox
                      checked={
                        workspaceRemovalSelection[worktree.worktreeId] === true
                      }
                      onCheckedChange={(checked) =>
                        setWorkspaceRemovalSelection((current) => ({
                          ...current,
                          [worktree.worktreeId]: checked === true,
                        }))
                      }
                      disabled={isSaving || !workspaceRemovalReport.safe}
                    />
                    {worktree.repositoryName} · {worktree.branch}
                  </span>
                  <code className="break-all text-xs text-muted-foreground">
                    {worktree.path}
                  </code>
                  {worktree.requiresDestructiveConfirmation && (
                    <span className="flex items-center gap-2 font-normal text-destructive">
                      <Checkbox
                        checked={
                          workspaceRemovalDestructive[worktree.worktreeId] ===
                          true
                        }
                        onCheckedChange={(checked) =>
                          setWorkspaceRemovalDestructive((current) => ({
                            ...current,
                            [worktree.worktreeId]: checked === true,
                          }))
                        }
                        disabled={isSaving || !workspaceRemovalReport.safe}
                      />
                      Confirm destructive removal of dirty files.
                    </span>
                  )}
                </label>
              ))}
              <div className="flex flex-wrap gap-2">
                <Button
                  type="button"
                  variant="destructive"
                  disabled={
                    isSaving ||
                    !workspaceRemovalReport.safe ||
                    workspaceRemovalReport.worktrees.length === 0 ||
                    workspaceRemovalReport.worktrees.some(
                      (worktree) =>
                        !workspaceRemovalSelection[worktree.worktreeId] ||
                        (worktree.requiresDestructiveConfirmation &&
                          !workspaceRemovalDestructive[worktree.worktreeId]),
                    )
                  }
                  onClick={() => void removeWorkspace()}
                >
                  {isSaving ? "Removing…" : "Remove selected Worktrees"}
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  disabled={isSaving}
                  onClick={() => setWorkspaceRemovalReport(undefined)}
                >
                  Cancel
                </Button>
              </div>
            </div>
          )}
          {worktreeRunWorkspaceId === workspace.id &&
            worktreeRunWorktreeId !== undefined && (
              <form
                className="grid gap-4 rounded-lg border border-primary/30 bg-primary/5 p-4"
                onSubmit={handleStartWorktreeRun}
              >
                <div>
                  <h4 className="m-0 text-base font-medium">
                    Start Worktree Run
                  </h4>
                  <p className="mt-1 text-sm text-muted-foreground">
                    The agent will start in the selected registered Worktree.
                  </p>
                </div>
                <div className="grid gap-3 md:grid-cols-2">
                  <label className="grid gap-1.5 text-sm font-medium">
                    <span>Agent</span>
                    <NativeSelect
                      value={runAgent}
                      onChange={(event) =>
                        setRunAgent(event.target.value as AgentKind)
                      }
                      disabled={isSaving}
                    >
                      <NativeSelectOption value="claude">
                        Claude Code
                      </NativeSelectOption>
                      <NativeSelectOption value="codex">Codex</NativeSelectOption>
                    </NativeSelect>
                  </label>
                  <label className="grid gap-1.5 text-sm font-medium">
                    <span>Execution Profile</span>
                    <NativeSelect
                      value={runProfile}
                      onChange={(event) => {
                        setRunProfile(event.target.value as ExecutionProfile);
                        setWorktreeRunPromptNeedsCompose(true);
                      }}
                      disabled={isSaving}
                    >
                      <NativeSelectOption value="investigate">
                        Investigate
                      </NativeSelectOption>
                      <NativeSelectOption value="implement">
                        Implement
                      </NativeSelectOption>
                      <NativeSelectOption value="review">Review</NativeSelectOption>
                      <NativeSelectOption value="custom">
                        Custom prompt
                      </NativeSelectOption>
                    </NativeSelect>
                  </label>
                </div>
                {runProfile === "custom" && (
                  <label className="grid gap-1.5 text-sm font-medium">
                    <span>Custom prompt source</span>
                    <Textarea
                      value={runCustomPrompt}
                      onChange={(event) => {
                        setRunCustomPrompt(event.target.value);
                        setWorktreeRunPromptNeedsCompose(true);
                      }}
                      rows={3}
                      placeholder="Tell the agent exactly what to do"
                      disabled={isSaving}
                    />
                  </label>
                )}
                <label className="grid gap-1.5 text-sm font-medium">
                  <span>Editable composed prompt</span>
                  <Textarea
                    value={worktreeRunPrompt}
                    onChange={(event) => setWorktreeRunPrompt(event.target.value)}
                    rows={6}
                    disabled={isSaving}
                  />
                </label>
                <div className="flex flex-wrap gap-2">
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={isSaving}
                    onClick={() => void composeWorktreeRunPrompt()}
                  >
                    Compose from selection
                  </Button>
                  <Button
                    type="submit"
                    disabled={
                      isSaving ||
                      !worktreeRunPrompt.trim() ||
                      worktreeRunPromptNeedsCompose
                    }
                  >
                    {isSaving ? "Starting…" : "Confirm and start Worktree Run"}
                  </Button>
                  <Button
                    type="button"
                    size="sm"
                    variant="ghost"
                    disabled={isSaving}
                    onClick={() => {
                      setWorktreeRunWorkspaceId(undefined);
                      setWorktreeRunWorktreeId(undefined);
                    }}
                  >
                    Cancel
                  </Button>
                </div>
              </form>
            )}
          {directRunWorkspaceId === workspace.id && directRunPreview && (
            <form
              className="grid gap-4 rounded-lg border border-primary/30 bg-primary/5 p-4"
              onSubmit={handleStartDirectRun}
            >
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div>
                  <h4 className="m-0 text-base font-medium">
                    Confirm Direct Run
                  </h4>
                  <p className="mt-1 text-sm text-muted-foreground">
                    Context: {view.context_name} · Project: {view.project_name}
                  </p>
                </div>
                <Badge variant="outline">
                  Machine: {directRunPreview.machineName}
                </Badge>
              </div>
              <label className="grid gap-1.5 text-sm font-medium">
                <span>Machine</span>
                <NativeSelect
                  value={directRunMachineId ?? ""}
                  onChange={(event) =>
                    void refreshDirectRunPreview(
                      Number(event.target.value) || undefined,
                    )
                  }
                  disabled={isSaving}
                >
                  <NativeSelectOption value="">
                    Local Mac (default)
                  </NativeSelectOption>
                  {itemMachines.map((machine) => (
                    <NativeSelectOption value={machine.id} key={machine.id}>
                      {machine.name} · {machine.last_observed}
                    </NativeSelectOption>
                  ))}
                </NativeSelect>
              </label>
              <div className="grid gap-2 rounded-md border p-3 text-sm">
                <p className="m-0 font-medium">Registered checkouts</p>
                {directRunPreview.checkoutDetails.map((checkout) => (
                  <div
                    className="flex flex-wrap justify-between gap-2"
                    key={checkout.repositoryId}
                  >
                    <span>
                      {checkout.repositoryName} · {checkout.branch}
                    </span>
                    <code className="break-all text-xs text-muted-foreground">
                      {checkout.path}
                    </code>
                  </div>
                ))}
              </div>
              <label className="grid gap-1.5 text-sm font-medium">
                <span>Primary Repository / working directory</span>
                <NativeSelect
                  value={directRunRepositoryId ?? ""}
                  onChange={(event) =>
                    setDirectRunRepositoryId(
                      Number(event.target.value) || undefined,
                    )
                  }
                  disabled={isSaving}
                >
                  <NativeSelectOption value="">
                    Choose the checkout for this Run
                  </NativeSelectOption>
                  {directRunPreview.checkoutDetails.map((checkout) => (
                    <NativeSelectOption
                      value={checkout.repositoryId}
                      key={checkout.repositoryId}
                    >
                      {checkout.repositoryName} · {checkout.path}
                    </NativeSelectOption>
                  ))}
                </NativeSelect>
              </label>
              {[...new Set(directRunPreview.currentBranches)].length > 1 && (
                <Alert>
                  <AlertTitle>Repositories are on different branches</AlertTitle>
                  <AlertDescription>
                    {directRunPreview.currentBranches.join(", ")}. The Run
                    will use each checkout's current branch without switching
                    it.
                  </AlertDescription>
                </Alert>
              )}
              {directRunPreview.dirtyRepositoryIds.length > 0 && (
                <Alert variant="destructive">
                  <AlertTitle>Dirty checkouts detected</AlertTitle>
                  <AlertDescription>
                    Existing uncommitted changes will remain in the shared
                    checkouts.
                    <label className="mt-2 flex items-center gap-2 font-normal">
                      <Checkbox
                        checked={directRunDirtyConfirmed}
                        onCheckedChange={(checked) =>
                          setDirectRunDirtyConfirmed(checked === true)
                        }
                        disabled={isSaving}
                      />
                      I understand and want to use these dirty checkouts.
                    </label>
                  </AlertDescription>
                </Alert>
              )}
              {directRunPreview.sharedPaths.length > 0 && (
                <Alert variant="destructive">
                  <AlertTitle>Checkouts are shared with active Runs</AlertTitle>
                  <AlertDescription>
                    {directRunPreview.sharedRuns.map((shared) => (
                      <div key={`${shared.runId}-${shared.path}`}>
                        Run #{shared.runId} · {shared.path}
                      </div>
                    ))}
                    <label className="mt-2 flex items-center gap-2 font-normal">
                      <Checkbox
                        checked={directRunSharedConfirmed}
                        onCheckedChange={(checked) =>
                          setDirectRunSharedConfirmed(checked === true)
                        }
                        disabled={isSaving}
                      />
                      I understand and want to share these checkouts.
                    </label>
                  </AlertDescription>
                </Alert>
              )}
              <div className="grid gap-3 md:grid-cols-2">
                <label className="grid gap-1.5 text-sm font-medium">
                  <span>Agent</span>
                  <NativeSelect
                    value={runAgent}
                    onChange={(event) =>
                      setRunAgent(event.target.value as AgentKind)
                    }
                    disabled={isSaving}
                  >
                    <NativeSelectOption value="claude">
                      Claude Code
                    </NativeSelectOption>
                    <NativeSelectOption value="codex">Codex</NativeSelectOption>
                  </NativeSelect>
                </label>
                <label className="grid gap-1.5 text-sm font-medium">
                  <span>Execution Profile</span>
                  <NativeSelect
                    value={runProfile}
                    onChange={(event) => {
                      setRunProfile(event.target.value as ExecutionProfile);
                      setRunPromptNeedsCompose(true);
                    }}
                    disabled={isSaving}
                  >
                    <NativeSelectOption value="investigate">
                      Investigate
                    </NativeSelectOption>
                    <NativeSelectOption value="implement">
                      Implement
                    </NativeSelectOption>
                    <NativeSelectOption value="review">Review</NativeSelectOption>
                    <NativeSelectOption value="custom">
                      Custom prompt
                    </NativeSelectOption>
                  </NativeSelect>
                </label>
              </div>
              {runProfile === "custom" && (
                <label className="grid gap-1.5 text-sm font-medium">
                  <span>Custom prompt source</span>
                  <Textarea
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
              <label className="grid gap-1.5 text-sm font-medium">
                <span>Editable composed prompt</span>
                <Textarea
                  value={runPrompt}
                  onChange={(event) => setRunPrompt(event.target.value)}
                  rows={6}
                  disabled={isSaving}
                />
              </label>
              <div className="flex flex-wrap gap-2">
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  disabled={isSaving}
                  onClick={() => void composeRunPromptPreview()}
                >
                  Compose from selection
                </Button>
                <Button
                  type="submit"
                  disabled={
                    isSaving ||
                    !directRunRepositoryId ||
                    !runPrompt.trim() ||
                    runPromptNeedsCompose ||
                    (directRunPreview.dirtyRepositoryIds.length > 0 &&
                      !directRunDirtyConfirmed) ||
                    (directRunPreview.sharedPaths.length > 0 &&
                      !directRunSharedConfirmed)
                  }
                >
                  {isSaving ? "Starting…" : "Confirm and start Direct Run"}
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  disabled={isSaving}
                  onClick={() => {
                    setDirectRunWorkspaceId(undefined);
                    setDirectRunPreview(undefined);
                  }}
                >
                  Cancel
                </Button>
              </div>
            </form>
          )}
        </CardContent>
      </Card>
    );
  }

  return (
    <>
      <Card size="sm" className="h-full">
        <CardHeader className="border-b border-border/70">
          <div className="flex items-start justify-between gap-2">
            <button
              type="button"
              className="grid min-w-0 flex-1 gap-1 rounded-md text-left outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
              aria-expanded={isExpanded}
              aria-controls={`item-card-content-${view.item.id}`}
              onClick={() => setIsExpanded((current) => !current)}
              onKeyDown={(event: KeyboardEvent<HTMLButtonElement>) => {
                if (event.key === "Escape" && isExpanded) {
                  setIsExpanded(false);
                }
              }}
            >
              <span className="flex flex-wrap items-center gap-2">
                <Badge variant="outline">{displayIdentifier}</Badge>
                <Badge variant="secondary">{view.item.status}</Badge>
              </span>
              <span className="font-heading text-base leading-snug font-medium group-data-[size=sm]/card:text-sm">
                {view.item.title}
              </span>
              <span className="text-sm text-muted-foreground">
                {view.context_name} <span>·</span> {view.project_name}
              </span>
            </button>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  type="button"
                  size="icon-sm"
                  variant="ghost"
                  aria-label={`More actions for ${displayIdentifier}`}
                  title="More actions"
                  disabled={isSaving}
                  onClick={(event) => event.stopPropagation()}
                >
                  <EllipsisVerticalIcon aria-hidden="true" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="end" className="w-48">
                <DropdownMenuLabel>Item actions</DropdownMenuLabel>
                <DropdownMenuItem
                  disabled={isSaving || Boolean(activeGrillRun)}
                  onSelect={openGrillStart}
                >
                  {activeGrillRun ? "Grill Run already active" : "Start Grill Run"}
                </DropdownMenuItem>
                <DropdownMenuItem
                  disabled={isSaving}
                  onSelect={() => {
                    setTitleDraft(view.item.title);
                    setIsRenameDialogOpen(true);
                  }}
                >
                  Rename title
                </DropdownMenuItem>
                <DropdownMenuItem
                  disabled={isSaving}
                  onSelect={() => setIsReminderDialogOpen(true)}
                >
                  Add reminder
                </DropdownMenuItem>
                <DropdownMenuSub>
                  <DropdownMenuSubTrigger>Change status</DropdownMenuSubTrigger>
                  <DropdownMenuSubContent>
                    <DropdownMenuRadioGroup
                      value={view.item.status}
                      onValueChange={(nextStatus) =>
                        handleItemStatusChange(nextStatus as ItemStatus)
                      }
                    >
                      {itemStatuses.map((status) => (
                        <DropdownMenuRadioItem value={status} key={status}>
                          {status}
                        </DropdownMenuRadioItem>
                      ))}
                    </DropdownMenuRadioGroup>
                  </DropdownMenuSubContent>
                </DropdownMenuSub>
                <DropdownMenuSeparator />
                <DropdownMenuItem
                  variant="destructive"
                  disabled={isSaving}
                  onSelect={() => void handlePrepareItemDeletion()}
                >
                  Delete Item
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </CardHeader>
        {isExpanded && (
          <CardContent
            id={`item-card-content-${view.item.id}`}
            className="space-y-5 pt-4"
          >
        {isGrillRunOpen && (
          <Dialog
            open
            onOpenChange={(open) => {
              if (!open && !isSaving) {
                setIsGrillRunOpen(false);
                setGrillRunWorkspaceId(undefined);
                setGrillRunPreview(undefined);
                setGrillRunPreviewError(undefined);
                setGrillPromptPreview("");
              }
            }}
          >
            <DialogContent className="max-h-[calc(100vh-2rem)] overflow-y-auto sm:max-w-3xl">
              <DialogHeader>
                <div className="flex flex-wrap items-start justify-between gap-3 pr-8">
                  <div className="grid gap-1">
                    <DialogTitle>Start Grill Run</DialogTitle>
                    <DialogDescription>
                      The Run uses this Item&apos;s Context defaults unless you override them here.
                    </DialogDescription>
                  </div>
                  {grillRunPreview && (
                    <Badge variant="outline">Machine: {grillRunPreview.machineName}</Badge>
                  )}
                </div>
              </DialogHeader>
              <form className="grid gap-4" onSubmit={handleStartGrillRun}>
            {view.workspaces.length === 0 && (
              <Alert variant="destructive">
                <AlertTitle>Workspace required</AlertTitle>
                <AlertDescription>
                  Create a Workspace with at least one Repository before starting a Grill Run.
                </AlertDescription>
              </Alert>
            )}
            <div className="grid gap-3 md:grid-cols-2">
              <label className="grid gap-1.5 text-sm font-medium">
                <span>Workspace</span>
                <NativeSelect
                  value={grillRunWorkspaceId ?? ""}
                  onChange={(event) =>
                    void refreshGrillRunPreview(
                      Number(event.target.value) || undefined,
                      undefined,
                    )
                  }
                  disabled={isSaving || view.workspaces.length === 0}
                >
                  <NativeSelectOption value="">Choose a Workspace</NativeSelectOption>
                  {view.workspaces.map((workspace) => (
                    <NativeSelectOption value={workspace.id} key={workspace.id}>
                      Workspace #{workspace.id} · {workspace.repositories.length} Repository(s)
                    </NativeSelectOption>
                  ))}
                </NativeSelect>
              </label>
              <label className="grid gap-1.5 text-sm font-medium">
                <span>Machine</span>
                <NativeSelect
                  value={grillRunMachineId ?? ""}
                  onChange={(event) =>
                    void refreshGrillRunPreview(
                      grillRunWorkspaceId,
                      Number(event.target.value) || undefined,
                    )
                  }
                  disabled={isSaving || !grillRunWorkspaceId}
                >
                  <NativeSelectOption value="">Local Mac (default)</NativeSelectOption>
                  {itemMachines.map((machine) => (
                    <NativeSelectOption value={machine.id} key={machine.id}>
                      {machine.name} · {machine.last_observed}
                    </NativeSelectOption>
                  ))}
                </NativeSelect>
              </label>
            </div>
            {grillRunPreviewError && (
              <Alert variant="destructive">
                <AlertTitle>Could not prepare the Grill checkout</AlertTitle>
                <AlertDescription>
                  <p>{grillRunPreviewError}</p>
                  <p>
                    Register this Repository&apos;s checkout for the selected Machine
                    under Structure, then reopen the Grill Run.
                  </p>
                </AlertDescription>
              </Alert>
            )}
            {grillRunPreview && (
              <>
                <label className="grid gap-1.5 text-sm font-medium">
                  <span>Primary Repository / checkout root</span>
                  <NativeSelect
                    value={grillRunRepositoryId ?? ""}
                    onChange={(event) =>
                      setGrillRunRepositoryId(Number(event.target.value) || undefined)
                    }
                    disabled={isSaving}
                  >
                    <NativeSelectOption value="">Choose the primary checkout</NativeSelectOption>
                    {grillRunPreview.checkoutDetails.map((checkout) => (
                      <NativeSelectOption value={checkout.repositoryId} key={checkout.repositoryId}>
                        {checkout.repositoryName} · {checkout.path}
                      </NativeSelectOption>
                    ))}
                  </NativeSelect>
                </label>
                <div className="grid gap-2 rounded-md border p-3 text-sm">
                  <p className="m-0 font-medium">Registered checkouts</p>
                  {grillRunPreview.checkoutDetails.map((checkout) => (
                    <div className="flex flex-wrap justify-between gap-2" key={checkout.repositoryId}>
                      <span>{checkout.repositoryName} · {checkout.branch}</span>
                      <code className="break-all text-xs text-muted-foreground">{checkout.path}</code>
                    </div>
                  ))}
                </div>
                {grillRunPreview.dirtyRepositoryIds.length > 0 && (
                  <Alert>
                    <AlertTitle>Checkout has local changes</AlertTitle>
                    <AlertDescription>
                      Review the checkout before allowing the Grill to use it.
                      <label className="mt-2 flex items-center gap-2 font-normal">
                        <Checkbox
                          checked={grillDirtyConfirmed}
                          onCheckedChange={(checked) => setGrillDirtyConfirmed(checked === true)}
                          disabled={isSaving}
                        />
                        I understand and want to use the dirty checkout.
                      </label>
                    </AlertDescription>
                  </Alert>
                )}
                {grillRunPreview.sharedPaths.length > 0 && (
                  <Alert variant="destructive">
                    <AlertTitle>Checkout is shared with an active Run</AlertTitle>
                    <AlertDescription>
                      {grillRunPreview.sharedRuns.map((shared) => (
                        <div key={`${shared.runId}-${shared.path}`}>Run #{shared.runId} · {shared.path}</div>
                      ))}
                      <label className="mt-2 flex items-center gap-2 font-normal">
                        <Checkbox
                          checked={grillSharedConfirmed}
                          onCheckedChange={(checked) => setGrillSharedConfirmed(checked === true)}
                          disabled={isSaving}
                        />
                        I understand and want to share this checkout.
                      </label>
                    </AlertDescription>
                  </Alert>
                )}
              </>
            )}
            <div className="grid gap-3 md:grid-cols-3">
              <label className="grid gap-1.5 text-sm font-medium">
                <span>Agent</span>
                <NativeSelect
                  value={grillAgent}
                  onChange={(event) => {
                    const nextAgent = event.target.value as GrillConfiguration["agent"];
                    const nextCatalog = grillModelCatalog.find((catalog) => catalog.agent === nextAgent);
                    const nextModel = nextCatalog?.models[0];
                    setGrillAgent(nextAgent);
                    setGrillModel(nextModel?.id ?? "");
                    setGrillEffort(nextModel?.efforts[0]?.id ?? "");
                    setGrillPromptPreview("");
                  }}
                  disabled={isSaving || grillModelCatalog.length === 0}
                >
                  {grillModelCatalog.map((catalog) => (
                    <NativeSelectOption value={catalog.agent} key={catalog.agent}>
                      {catalog.agent === "claude" ? "Claude Code" : "Codex"}
                    </NativeSelectOption>
                  ))}
                </NativeSelect>
              </label>
              <label className="grid gap-1.5 text-sm font-medium">
                <span>Model</span>
                <NativeSelect
                  value={grillModel}
                  onChange={(event) => {
                    const nextModel = selectedGrillCatalog?.models.find((model) => model.id === event.target.value);
                    setGrillModel(event.target.value);
                    setGrillEffort(nextModel?.efforts[0]?.id ?? "");
                    setGrillPromptPreview("");
                  }}
                  disabled={isSaving || !selectedGrillCatalog}
                >
                  {selectedGrillCatalog?.models.map((model) => (
                    <NativeSelectOption value={model.id} key={model.id}>
                      {model.label} ({model.id})
                    </NativeSelectOption>
                  ))}
                </NativeSelect>
              </label>
              <label className="grid gap-1.5 text-sm font-medium">
                <span>Effort</span>
                <NativeSelect
                  value={grillEffort}
                  onChange={(event) => {
                    setGrillEffort(event.target.value);
                    setGrillPromptPreview("");
                  }}
                  disabled={isSaving || !selectedGrillModel}
                >
                  {selectedGrillModel?.efforts.map((effort) => (
                    <NativeSelectOption value={effort.id} key={effort.id}>
                      {effort.label} ({effort.id})
                    </NativeSelectOption>
                  ))}
                </NativeSelect>
              </label>
            </div>
            <label className="grid gap-1.5 text-sm font-medium">
              <span>Initial prompt</span>
              <Textarea
                value={grillInitialPrompt}
                onChange={(event) => {
                  setGrillInitialPrompt(event.target.value);
                  setGrillPromptPreview("");
                }}
                rows={3}
                placeholder="What decision, assumption, or plan should the Grill stress-test?"
                disabled={isSaving}
              />
            </label>
            {grillPromptPreview && (
              <label className="grid gap-1.5 text-sm font-medium">
                <span>Composed prompt preview</span>
                <Textarea value={grillPromptPreview} rows={8} readOnly />
              </label>
            )}
            <DialogFooter>
              <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={isSaving || !grillInitialPrompt.trim() || !selectedGrillModel}
                onClick={() => void composeGrillPromptPreview()}
              >
                Preview composed prompt
              </Button>
              <Button
                type="submit"
                disabled={
                  isSaving ||
                  !grillRunPreview ||
                  !grillRunRepositoryId ||
                  !grillInitialPrompt.trim() ||
                  !selectedGrillModel ||
                  (grillRunPreview?.dirtyRepositoryIds.length ?? 0) > 0 && !grillDirtyConfirmed ||
                  (grillRunPreview?.sharedPaths.length ?? 0) > 0 && !grillSharedConfirmed
                }
              >
                {isSaving ? "Starting…" : "Start Grill Run"}
              </Button>
              <Button
                type="button"
                size="sm"
                variant="ghost"
                disabled={isSaving}
                onClick={() => {
                  setIsGrillRunOpen(false);
                  setGrillRunWorkspaceId(undefined);
                  setGrillRunPreview(undefined);
                  setGrillRunPreviewError(undefined);
                  setGrillPromptPreview("");
                }}
              >
                Cancel
              </Button>
            </DialogFooter>
              </form>
            </DialogContent>
          </Dialog>
        )}
        <label className="grid gap-1.5 text-sm font-medium">
          <span>Notes</span>
          <Textarea
            value={notes}
            onChange={(event) => setNotes(event.target.value)}
            placeholder="Add a useful handoff note"
            rows={3}
            disabled={isSaving}
          />
        </label>
        <Button
          size="sm"
          type="button"
          variant="outline"
          disabled={isSaving || notes === view.item.notes}
          onClick={() =>
            void saveItem(workActions.setItemNotes(view.item.id, notes))
          }
        >
          Save notes
        </Button>
        {view.item.reminders.length > 0 && (
          <div className="grid gap-2">
            <span className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              Reminders
            </span>
            <div className="flex flex-wrap gap-2">
              {view.item.reminders.map((reminder) => (
                <Badge
                  variant="secondary"
                  className="h-auto gap-1 py-1"
                  key={reminder.id}
                >
                  {reminder.remind_at}
                  <Button
                    type="button"
                    size="xs"
                    variant="ghost"
                    disabled={isSaving}
                    onClick={() =>
                      void saveItem(
                        workActions.removeReminder(view.item.id, reminder.id),
                      )
                    }
                  >
                    Remove
                  </Button>
                </Badge>
              ))}
            </div>
          </div>
        )}
        {view.runs.length > 0 && (
          <div className="grid gap-2">
            <span className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
              Run history
            </span>
            {view.runs.map((run) => {
              const runWorkspace =
                run.workspace_id === null
                  ? undefined
                  : view.workspaces.find(
                      (workspace) => workspace.id === run.workspace_id,
                    );
              const runRepository =
                run.repository_id === null
                  ? run.direct_checkouts.find(
                      (checkout) => checkout.path === run.working_directory,
                    )?.repositoryId
                  : run.repository_id;
              const runWorktree =
                run.worktree_id === null
                  ? view.worktrees.find(
                      (worktree) => worktree.path === run.working_directory,
                    )
                  : view.worktrees.find(
                      (worktree) => worktree.id === run.worktree_id,
                    );
              return (
                <Card size="sm" className="bg-muted/20" key={run.id}>
                  <CardContent className="grid gap-2 pt-4">
                    <div>
                      <strong className="block text-sm">
                        Run #{run.id} ·{" "}
                        {run.agent === "claude" ? "Claude Code" : "Codex"}
                      </strong>
                      <span className="text-xs text-muted-foreground">
                        {run.execution_profile} ·{" "}
                        {run.execution_profile === "grill" && run.model && run.effort
                          ? `${run.model} · ${run.effort} · `
                          : ""}
                        {machines.find(
                          (machine) => machine.id === run.machine_id,
                        )?.name ?? "Machine #" + run.machine_id}{" "}
                        · {runStateLabel(run.state, run.execution_profile === "grill")}
                        {run.execution_profile === "grill" && run.grill_phase
                          ? ` · ${grillPhaseLabel(run.grill_phase)}`
                          : ""}
                        {run.pane_status === "available" && " · Pane available"}
                      </span>
                    </div>
                    <code className="break-all font-mono text-xs">
                      {run.working_directory}
                    </code>
                    <span className="text-xs text-muted-foreground">
                      {runWorkspace
                        ? `Workspace #${runWorkspace.id}`
                        : "Unscoped"}
                      {runRepository !== undefined
                        ? ` · Repository ${repositoryName(repositories, runRepository)}`
                        : ""}
                      {runWorktree ? " · Worktree" : runWorkspace ? " · Direct checkout" : ""}
                    </span>
                    <span className="text-xs text-muted-foreground">
                      Session {run.session_name} · Pane {run.pane_id}
                    </span>
                    {run.execution_profile === "grill" && run.transcript && (
                      <details className="rounded-md border bg-background/60 p-2 text-xs">
                        <summary className="cursor-pointer font-medium">
                          Retained Grill transcript
                        </summary>
                        <pre className="mt-2 max-h-64 overflow-auto whitespace-pre-wrap font-mono text-muted-foreground">
                          {run.transcript}
                        </pre>
                      </details>
                    )}
                    {(run.pane_status === "missing" ||
                      run.grill_phase === "recoverablePaneLoss") && (
                      <span className="text-xs text-destructive">
                        Pane unavailable. This Run is preserved and recoverable;
                        reconnect the exact Pane or use the terminal escape hatch
                        when the runtime returns.
                      </span>
                    )}
                    {run.pane_status === "unknown" && (
                      <span className="text-xs text-muted-foreground">
                        Pane status not confirmed.
                      </span>
                    )}
                    {run.execution_profile === "grill" &&
                      run.grill_phase === "waitingForAnswers" &&
                      !run.grill_response && (
                        <GrillQuestionFlow
                          run={run}
                          disabled={isSaving}
                          onSubmit={(answers) =>
                            saveItem(workActions.submitGrillAnswers(run.id, answers))
                          }
                        />
                      )}
                    {run.execution_profile === "grill" &&
                      run.grill_phase === "awaitingNextAction" && (
                        <div className="grid gap-2 rounded-md border border-primary/30 bg-primary/5 p-3">
                          <div>
                            <strong className="block text-sm">
                              Grill completed · choose the next action
                            </strong>
                            <span className="text-xs text-muted-foreground">
                              Each action continues this Run in the same Pane and working directory.
                              Finish or stop remains explicit.
                            </span>
                          </div>
                          <div className="flex flex-wrap gap-2">
                            {([
                              ["to-spec", "to-spec"],
                              ["to-tickets", "to-tickets"],
                              ["implement", "implement"],
                            ] as const).map(([action, label]) => (
                              <Button
                                key={action}
                                type="button"
                                size="sm"
                                variant="outline"
                                disabled={isSaving || run.pane_status !== "available"}
                                onClick={() => void handleContinueGrill(run, action)}
                              >
                                {label}
                              </Button>
                            ))}
                          </div>
                          {run.pane_status !== "available" && (
                            <span className="text-xs text-destructive">
                              Reconnect the exact Pane before continuing this Run.
                            </span>
                          )}
                        </div>
                      )}
                    {run.workspace_id !== null && (
                      <div className="flex flex-wrap gap-2">
                        <Button
                          type="button"
                          size="sm"
                          variant="outline"
                          disabled={isSaving}
                          onClick={() =>
                            onOpenTerminal(run.id, paneTabForRun(run))
                          }
                        >
                          {run.pane_status === "missing" ||
                          run.grill_phase === "recoverablePaneLoss"
                            ? "Reconnect embedded terminal"
                            : "Open embedded terminal"}
                        </Button>
                        <Button
                          type="button"
                          size="sm"
                          variant="outline"
                          disabled={isSaving}
                          onClick={() =>
                            void saveItem(
                              workActions.openExternalTerminal(run.id),
                              false,
                            )
                          }
                        >
                          Open in Terminal
                        </Button>
                        {!isRunFinished(run) &&
                          run.pane_status !== "missing" && (
                            <Button
                              type="button"
                              size="sm"
                              variant="outline"
                              disabled={isSaving}
                              onClick={() => void handleStopRun(run)}
                            >
                              Stop Run
                            </Button>
                          )}
                        {!isRunFinished(run) && (
                          <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            disabled={isSaving}
                            onClick={() => void handleFinishRun(run)}
                          >
                            Finish Run
                          </Button>
                        )}
                        {isRunFinished(run) && (
                          <Button
                            type="button"
                            size="sm"
                            variant="destructive"
                            disabled={isSaving}
                            onClick={() => void handleDeleteRun(run)}
                          >
                            Delete finished Run
                          </Button>
                        )}
                      </div>
                    )}
                  </CardContent>
                </Card>
              );
            })}
          </div>
        )}
        <div className="grid gap-3">
          <span className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
            Workspaces
          </span>
          {view.workspaces.map(renderWorkspaceCard)}
          <Card size="sm">
            <CardHeader>
              <CardTitle className="text-sm">Create a Workspace</CardTitle>
              <CardDescription>
                Save a reusable Repository subset and its branch
                configuration. Workspaces do not create a shared
                multi-Repository root.
              </CardDescription>
            </CardHeader>
            <CardContent>
              <form className="grid gap-3" onSubmit={handleCreateWorkspace}>
                <span className="text-sm font-medium">
                  Select repositories
                </span>
                {itemRepositories.length === 0 ? (
                  <span className="text-sm text-muted-foreground">
                    Register a Repository under this Project first.
                  </span>
                ) : (
                  <div className="grid gap-2">
                    {itemRepositories.map((repository) => {
                      const selected = workspaceRepositoryIds.includes(
                        repository.id,
                      );
                      return (
                        <div
                          className="grid gap-2 rounded-md border p-3"
                          key={repository.id}
                        >
                          <label className="flex items-center gap-2 text-sm font-normal">
                            <Checkbox
                              checked={selected}
                              onCheckedChange={(checked) => {
                                setWorkspaceRepositoryIds((current) =>
                                  checked === true
                                    ? [...current, repository.id]
                                    : current.filter(
                                        (id) => id !== repository.id,
                                      ),
                                );
                                if (checked === true) {
                                  setWorkspaceBranches((current) => ({
                                    ...current,
                                    [repository.id]:
                                      current[repository.id] ??
                                      repository.base_branch,
                                  }));
                                  setWorkspaceBaseBranches((current) => ({
                                    ...current,
                                    [repository.id]:
                                      current[repository.id] ??
                                      repository.base_branch,
                                  }));
                                }
                              }}
                              disabled={isSaving}
                            />
                            {repository.name}
                          </label>
                          {selected && (
                            <div className="grid gap-2 sm:grid-cols-2">
                              <label className="grid gap-1.5 text-sm font-medium">
                                <span>Workspace branch</span>
                                <Input
                                  aria-label={`${repository.name} workspace branch`}
                                  value={
                                    workspaceBranches[repository.id] ?? ""
                                  }
                                  onChange={(event) =>
                                    setWorkspaceBranches((current) => ({
                                      ...current,
                                      [repository.id]: event.target.value,
                                    }))
                                  }
                                  placeholder={repository.base_branch}
                                  disabled={isSaving}
                                />
                              </label>
                              <label className="grid gap-1.5 text-sm font-medium">
                                <span>Base branch</span>
                                <Input
                                  aria-label={`${repository.name} workspace base branch`}
                                  value={
                                    workspaceBaseBranches[repository.id] ??
                                    ""
                                  }
                                  onChange={(event) =>
                                    setWorkspaceBaseBranches((current) => ({
                                      ...current,
                                      [repository.id]: event.target.value,
                                    }))
                                  }
                                  placeholder={repository.base_branch}
                                  disabled={isSaving}
                                />
                              </label>
                            </div>
                          )}
                        </div>
                      );
                    })}
                  </div>
                )}
                <Button
                  type="submit"
                  disabled={
                    isSaving ||
                    workspaceRepositoryIds.length === 0 ||
                    workspaceRepositoryIds.some(
                      (repositoryId) =>
                        !workspaceBranches[repositoryId]?.trim() ||
                        !workspaceBaseBranches[repositoryId]?.trim(),
                    )
                  }
                >
                  {isSaving ? "Saving…" : "Create Workspace"}
                </Button>
              </form>
            </CardContent>
          </Card>
        </div>
        <div className="grid gap-3">
          <span className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
            External Links
          </span>
          {view.links.map((externalLink) => (
            <ExternalLinkCard
              key={externalLink.link.id}
              externalLink={externalLink}
              isSaving={isSaving}
              onRefresh={() => refreshExternalObject(externalLink.object.id)}
              onUnlink={() => handleUnlinkExternalLink(externalLink.link.id)}
              onPrepareDeleteObject={() =>
                handlePrepareExternalObjectDeletion(externalLink.object.id)
              }
              onSavePolicy={async (policy) => {
                await saveItem(
                  workActions.setLinkAttentionPolicy(
                    externalLink.link.id,
                    policy,
                  ),
                );
              }}
              onMarkReviewed={async () => {
                await saveItem(workActions.markLinkReviewed(externalLink.link.id));
              }}
              onSaveWatchUntil={async (watchUntil) => {
                await saveItem(
                  workActions.setLinkWatchUntil(
                    externalLink.link.id,
                    watchUntil,
                  ),
                );
              }}
              onSaveReviewAt={async (reviewAt) => {
                await saveItem(
                  workActions.setLinkReviewAt(externalLink.link.id, reviewAt),
                );
              }}
              onClearReviewAt={async () => {
                await saveItem(
                  workActions.clearLinkReviewAt(externalLink.link.id),
                );
              }}
              onAddComment={async (body) => {
                await saveItem(
                  workActions.addExternalComment(externalLink.link.id, body),
                );
              }}
            />
          ))}
          {externalObjectDeletionPreview && (
            <Alert variant="destructive">
              <AlertTitle>Remove External Object locally</AlertTitle>
              <AlertDescription className="space-y-3">
                <p>
                  This removes the local External Object record and every Link
                  to it. It does not call GitHub or any other provider, so
                  provider-owned Issues, pull requests, and other objects are
                  never deleted.
                </p>
                <ul className="grid gap-1 pl-5">
                  <li>
                    {externalObjectDeletionPreview.plan.linkIds.length} Link(s)
                  </li>
                  <li>
                    {externalObjectDeletionPreview.plan.snapshotCount}{" "}
                    snapshot(s)
                  </li>
                  <li>
                    {externalObjectDeletionPreview.plan.activityCount} Activity
                    record(s)
                  </li>
                </ul>
                <div className="grid gap-1">
                  <strong>Items affected</strong>
                  {externalObjectDeletionPreview.links.map((link) => (
                    <span key={link.linkId}>
                      {displayItemIdentifier(link.itemIdentifier)} · {link.itemTitle}
                    </span>
                  ))}
                </div>
                <p className="font-medium">
                  {externalObjectDeletionPreview.providerWarning}
                </p>
              </AlertDescription>
              <div className="mt-3 flex flex-wrap gap-2">
                <Button
                  type="button"
                  size="sm"
                  disabled={isSaving}
                  onClick={() => void handleDeleteExternalObject()}
                >
                  Confirm local removal
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  disabled={isSaving}
                  onClick={() => setExternalObjectDeletionPreview(undefined)}
                >
                  Keep local records
                </Button>
              </div>
            </Alert>
          )}
          <form className="flex flex-wrap gap-2" onSubmit={handleExternalLink}>
            <Input
              className="min-w-0 flex-1"
              aria-label={`External URL for ${displayIdentifier}`}
              value={externalUrl}
              onChange={(event) => setExternalUrl(event.target.value)}
              placeholder="Paste a GitHub issue, pull request, or URL"
              disabled={isSaving}
            />
            <Button
              type="submit"
              variant="outline"
              disabled={isSaving || !externalUrl.trim()}
            >
              Add link
            </Button>
          </form>
          {!isIssuePreviewOpen ? (
            <Button
              type="button"
              variant="outline"
              disabled={isSaving}
              onClick={openIssuePreview}
            >
              Create GitHub Issue
            </Button>
          ) : (
            <Card size="sm" className="border-primary/30 bg-primary/5">
              <form className="grid gap-3 p-4" onSubmit={handleCreateIssue}>
                <div>
                  <strong className="block text-sm">
                    Preview GitHub Issue
                  </strong>
                  <p className="mt-1 text-sm text-muted-foreground">
                    Nothing is sent until you confirm. The existing Item will
                    remain unchanged and the created Issue will be linked to it.
                  </p>
                </div>
                <label className="grid gap-1.5 text-sm font-medium">
                  <span>Repository</span>
                  <Input
                    value={issueRepository}
                    onChange={(event) => setIssueRepository(event.target.value)}
                    placeholder="owner/repository"
                    disabled={isSaving}
                  />
                </label>
                <label className="grid gap-1.5 text-sm font-medium">
                  <span>Public title</span>
                  <Input
                    value={issueTitle}
                    onChange={(event) => setIssueTitle(event.target.value)}
                    disabled={isSaving}
                  />
                </label>
                <label className="grid gap-1.5 text-sm font-medium">
                  <span>Public body</span>
                  <Textarea
                    value={issueBody}
                    onChange={(event) => setIssueBody(event.target.value)}
                    rows={4}
                    placeholder="Optional public context"
                    disabled={isSaving}
                  />
                </label>
                <div className="flex flex-wrap gap-2">
                  <Button
                    type="submit"
                    disabled={
                      isSaving || !issueRepository.trim() || !issueTitle.trim()
                    }
                  >
                    {isSaving ? "Creating…" : "Confirm and create Issue"}
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    disabled={isSaving}
                    onClick={() => setIsIssuePreviewOpen(false)}
                  >
                    Cancel
                  </Button>
                </div>
              </form>
            </Card>
          )}
        </div>
        <div className="grid gap-2">
          <span className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
            Relationships
          </span>
          {view.relationships.length === 0 ? (
            <span className="text-sm text-muted-foreground">None yet</span>
          ) : (
            view.relationships.map((relation) => {
              const otherId =
                relation.from_item_id === view.item.id
                  ? relation.to_item_id
                  : relation.from_item_id;
              const other = allItems.find(
                (candidate) => candidate.item.id === otherId,
              );
              return (
                <Badge
                  variant="secondary"
                  key={`${relation.from_item_id}-${relation.to_item_id}-${relation.kind}`}
                >
                  {relationshipLabel(relation, view.item.id)}{" "}
                  {displayItemIdentifier(
                    other?.item.human_identifier ?? `MC-${otherId}`,
                  )}
                </Badge>
              );
            })
          )}
        </div>
        {visibleTargets.length > 0 && (
          <form
            className="grid gap-2 sm:grid-cols-[1fr_2fr_auto] sm:items-end"
            onSubmit={handleRelation}
          >
            <NativeSelect
              aria-label="Relationship kind"
              value={relationKind}
              onChange={(event) =>
                setRelationKind(event.target.value as ItemRelationKind)
              }
              disabled={isSaving}
            >
              {relationKinds.map((kind) => (
                <NativeSelectOption value={kind} key={kind}>
                  {relationKindLabel(kind)}
                </NativeSelectOption>
              ))}
            </NativeSelect>
            <NativeSelect
              aria-label="Related Item"
              value={targetItemId ?? ""}
              onChange={(event) => setTargetItemId(Number(event.target.value))}
              disabled={isSaving}
            >
              <NativeSelectOption value="">Choose an Item</NativeSelectOption>
              {visibleTargets.map((candidate) => (
                <NativeSelectOption
                  value={candidate.item.id}
                  key={candidate.item.id}
                >
                  {displayItemIdentifier(candidate.item.human_identifier)} ·{" "}
                  {candidate.item.title}
                </NativeSelectOption>
              ))}
            </NativeSelect>
            <Button
              type="submit"
              variant="outline"
              disabled={isSaving || !targetItemId}
            >
              Link
            </Button>
          </form>
        )}
          </CardContent>
        )}
      </Card>
      {isRenameDialogOpen && (
        <Dialog
          open
          onOpenChange={(open) => {
            if (!open && !isSaving) {
              setIsRenameDialogOpen(false);
              setTitleDraft(view.item.title);
            }
          }}
        >
          <DialogContent>
            <DialogHeader>
              <DialogTitle>Rename title</DialogTitle>
              <DialogDescription>
                Choose a clear title for {displayIdentifier}.
              </DialogDescription>
            </DialogHeader>
            <form
              className="grid gap-4"
              onSubmit={(event) => {
                event.preventDefault();
                void handleRenameTitle();
              }}
            >
              <label className="grid gap-1.5 text-sm font-medium">
                <span>Title</span>
                <Input
                  autoFocus
                  value={titleDraft}
                  onChange={(event) => setTitleDraft(event.target.value)}
                  disabled={isSaving}
                />
              </label>
              <DialogFooter>
                <Button
                  type="button"
                  variant="outline"
                  disabled={isSaving}
                  onClick={() => {
                    setIsRenameDialogOpen(false);
                    setTitleDraft(view.item.title);
                  }}
                >
                  Cancel
                </Button>
                <Button
                  type="submit"
                  disabled={
                    isSaving ||
                    !titleDraft.trim() ||
                    titleDraft.trim() === view.item.title
                  }
                >
                  {isSaving ? "Saving…" : "Save title"}
                </Button>
              </DialogFooter>
            </form>
          </DialogContent>
        </Dialog>
      )}
      {isReminderDialogOpen && (
        <Dialog
          open
          onOpenChange={(open) => {
            if (!open && !isSaving) {
              setIsReminderDialogOpen(false);
              setReminderAt("");
            }
          }}
        >
          <DialogContent>
            <DialogHeader>
              <DialogTitle>Add reminder</DialogTitle>
              <DialogDescription>
                Choose when {displayIdentifier} should appear in Needs Attention.
              </DialogDescription>
            </DialogHeader>
            <form
              className="grid gap-4"
              onSubmit={(event) => {
                event.preventDefault();
                void handleAddReminder();
              }}
            >
              <label className="grid gap-1.5 text-sm font-medium">
                <span>Reminder date and time</span>
                <Input
                  autoFocus
                  type="datetime-local"
                  value={reminderAt}
                  onChange={(event) => setReminderAt(event.target.value)}
                  disabled={isSaving}
                />
              </label>
              <DialogFooter>
                <Button
                  type="button"
                  variant="outline"
                  disabled={isSaving}
                  onClick={() => {
                    setIsReminderDialogOpen(false);
                    setReminderAt("");
                  }}
                >
                  Cancel
                </Button>
                <Button type="submit" disabled={isSaving || !reminderAt}>
                  {isSaving ? "Saving…" : "Add reminder"}
                </Button>
              </DialogFooter>
            </form>
          </DialogContent>
        </Dialog>
      )}
      {confirmation && (
        <ConfirmationDialog
          open
          title={confirmation.title}
          description={confirmation.description}
          confirmLabel={confirmation.confirmLabel}
          disabled={isSaving}
          onOpenChange={(open) => {
            if (!open && !isSaving) setConfirmation(undefined);
          }}
          onConfirm={confirmation.onConfirm}
        />
      )}
      {deletionPreview && (
        <Dialog
          open
          onOpenChange={(open) => {
            if (!open && !isSaving) {
              setDeletionPreview(undefined);
            }
          }}
        >
          <DialogContent className="max-h-[calc(100vh-2rem)] overflow-y-auto sm:max-w-2xl">
            <DialogHeader>
              <DialogTitle>
                Delete {displayItemIdentifier(deletionPreview.plan.humanIdentifier)}?
              </DialogTitle>
              <DialogDescription>
                This removes the Item and its local descendants. External Issues
                and pull requests are never changed.
              </DialogDescription>
            </DialogHeader>
            <div className="grid gap-4 text-sm">
              <ul className="grid gap-1 pl-5">
                <li>{deletionPreview.plan.reminderCount} reminder(s)</li>
                <li>
                  {deletionPreview.plan.relationshipCount} Item relationship(s)
                </li>
                <li>{deletionPreview.plan.workspaces.length} Workspace(s)</li>
                <li>{deletionPreview.plan.runIds.length} Run(s)</li>
                <li>{deletionPreview.plan.linkIds.length} Link(s)</li>
                <li>
                  {deletionPreview.plan.orphanedExternalObjectIds.length}{" "}
                  orphaned External Object(s),{" "}
                  {deletionPreview.plan.orphanedSnapshotCount} snapshot(s), and{" "}
                  {deletionPreview.plan.orphanedActivityCount} Activity record(s)
                </li>
              </ul>
              {deletionPreview.blockers.length > 0 && (
                <div className="grid gap-1 rounded-md border border-destructive/30 bg-destructive/5 p-3 text-destructive">
                  <strong>Deletion blocked</strong>
                  {deletionPreview.blockers.map((blocker) => (
                    <span key={blocker}>{blocker}</span>
                  ))}
                  <span>Resolve each blocker, then create a fresh preview.</span>
                </div>
              )}
            </div>
            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                disabled={isSaving}
                onClick={() => {
                  setDeletionPreview(undefined);
                }}
              >
                Cancel
              </Button>
              <Button
                type="button"
                variant="destructive"
                disabled={isSaving || deletionPreview.blockers.length > 0}
                onClick={() => void handleDeleteItem()}
              >
                {isSaving ? "Deleting…" : "Confirm logical deletion"}
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      )}
    </>
  );
}

function GrillQuestionFlow({
  run,
  disabled,
  onSubmit,
}: {
  run: Run;
  disabled: boolean;
  onSubmit: (answers: GrillAnswer[]) => Promise<unknown>;
}) {
  const group = run.grill_question_group;
  const [answers, setAnswers] = useState<Record<number, string>>(() =>
    Object.fromEntries(
      run.grill_answers.map((answer) => [answer.questionNumber, answer.answer]),
    ),
  );
  const [isSubmitting, setIsSubmitting] = useState(false);

  useEffect(() => {
    setAnswers(
      Object.fromEntries(
        run.grill_answers.map((answer) => [answer.questionNumber, answer.answer]),
      ),
    );
  }, [run.id, run.grill_answers]);

  if (!group || group.questions.length === 0) {
    return (
      <Alert className="border-amber-500/30 bg-amber-500/5">
        <AlertTitle>Grill is waiting for your input</AlertTitle>
        <AlertDescription>
          The question markers were incomplete or could not be parsed. The raw
          transcript is retained; continue safely through the embedded terminal.
        </AlertDescription>
      </Alert>
    );
  }
  const questionGroup = group;

  const unanswered = questionGroup.questions.some(
    (question) => !answers[question.number]?.trim(),
  );

  async function submitAnswers() {
    if (unanswered || disabled || isSubmitting) return;
    setIsSubmitting(true);
    try {
      await onSubmit(
        questionGroup.questions.map((question) => ({
          questionNumber: question.number,
          answer: answers[question.number].trim(),
        })),
      );
    } finally {
      setIsSubmitting(false);
    }
  }

  return (
    <section className="grid gap-3 rounded-lg border border-primary/30 bg-primary/5 p-4">
      <div>
        <h4 className="m-0 text-base font-medium">Grill questions</h4>
        <p className="mt-1 text-sm text-muted-foreground">
          Answer this complete group once. Mission Manager will send one numbered response.
        </p>
      </div>
      {questionGroup.questions.map((question) => {
        const recommendationAnswer = question.recommendation
          ? `Recommendation: ${question.recommendation}`
          : undefined;
        return (
          <div className="grid gap-2 rounded-md border bg-background/60 p-3" key={question.number}>
            <div className="text-sm">
              <strong>
                {question.number}. {question.title ?? "Question"}
              </strong>
              <p className="mt-1 whitespace-pre-wrap text-muted-foreground">
                {question.prompt}
              </p>
            </div>
            {recommendationAnswer && (
              <Button
                type="button"
                size="sm"
                variant={answers[question.number] === recommendationAnswer ? "default" : "outline"}
                className="justify-start whitespace-normal text-left"
                disabled={disabled || isSubmitting}
                onClick={() =>
                  setAnswers((current) => ({
                    ...current,
                    [question.number]: recommendationAnswer,
                  }))
                }
              >
                Accept recommendation: {question.recommendation}
              </Button>
            )}
            {question.options.length > 0 ? (
              <label className="grid gap-1.5 text-sm font-medium">
                <span>Choose an option</span>
                <NativeSelect
                  value={
                    question.options.some(
                      (option) => answers[question.number] === `${option.key}. ${option.label}`,
                    )
                      ? answers[question.number]
                      : ""
                  }
                  onChange={(event) =>
                    setAnswers((current) => ({
                      ...current,
                      [question.number]: event.target.value,
                    }))
                  }
                  disabled={disabled || isSubmitting}
                >
                  <NativeSelectOption value="">Choose an option</NativeSelectOption>
                  {question.options.map((option) => (
                    <NativeSelectOption
                      value={`${option.key}. ${option.label}`}
                      key={option.key}
                    >
                      {option.key}. {option.label}
                    </NativeSelectOption>
                  ))}
                </NativeSelect>
              </label>
            ) : (
              <label className="grid gap-1.5 text-sm font-medium">
                <span>Your answer</span>
                <Textarea
                  value={answers[question.number] ?? ""}
                  onChange={(event) =>
                    setAnswers((current) => ({
                      ...current,
                      [question.number]: event.target.value,
                    }))
                  }
                  rows={3}
                  placeholder="Write a free-form answer"
                  disabled={disabled || isSubmitting}
                />
              </label>
            )}
          </div>
        );
      })}
      <Button
        type="button"
        disabled={disabled || isSubmitting || unanswered}
        onClick={() => void submitAnswers()}
      >
        {isSubmitting ? "Sending grouped response…" : "Submit all answers"}
      </Button>
    </section>
  );
}

export function ExternalLinkCard({
  externalLink,
  isSaving,
  onRefresh,
  onUnlink,
  onPrepareDeleteObject,
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
  onUnlink: () => void | Promise<void>;
  onPrepareDeleteObject: () => Promise<void>;
  onSavePolicy: (policy: ExternalChangePolicy | null) => Promise<void>;
  onMarkReviewed: () => Promise<void>;
  onSaveWatchUntil: (watchUntil: string | null) => Promise<void>;
  onSaveReviewAt: (reviewAt: string | null) => Promise<void>;
  onClearReviewAt: () => Promise<void>;
  onAddComment: (body: string) => Promise<void>;
}) {
  const { object, snapshot } = externalLink;
  const [policy, setPolicy] = useState(externalLink.attention_policy);
  const [watchUntil, setWatchUntil] = useState(
    externalLink.link.watch_until ?? "",
  );
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
    <Card size="sm">
      <CardHeader className="border-b border-border/70">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <CardTitle className="text-sm">
              {snapshot?.title ?? object.canonical_url}
            </CardTitle>
            <CardDescription className="mt-1">
              {externalObjectKindLabel(object.kind)} ·{" "}
              {snapshot?.state ?? "Not fetched"}
            </CardDescription>
          </div>
          <div className="flex flex-wrap gap-2">
            {object.provider === "github" && (
              <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={isSaving}
                onClick={() => void onRefresh()}
              >
                Refresh
              </Button>
            )}
            <Button
              type="button"
              size="sm"
              variant="ghost"
              disabled={isSaving}
              onClick={() => void onUnlink()}
            >
              Unlink this Item
            </Button>
            <Button
              type="button"
              size="sm"
              variant="ghost"
              className="text-destructive hover:text-destructive"
              disabled={isSaving}
              onClick={() => void onPrepareDeleteObject()}
            >
              Remove local object…
            </Button>
          </div>
        </div>
      </CardHeader>
      <CardContent className="grid gap-3 pt-4">
        <a
          href={object.canonical_url}
          target="_blank"
          rel="noreferrer"
          className="break-all text-sm text-primary underline-offset-4 hover:underline"
        >
          {object.canonical_url}
        </a>
        {externalLink.link.provenance && (
          <p className="m-0 text-xs text-muted-foreground">
            Discovered by Run #{externalLink.link.provenance.run_id} via{" "}
            {externalLink.link.provenance.action} ·{" "}
            {externalLink.link.provenance.discovery === "structured-event"
              ? "structured Issue-created event"
              : "Run output URL"}
          </p>
        )}
        {snapshot ? (
          <>
            <div className="flex flex-wrap gap-2">
              {snapshot.metadata.map((metadata) => (
                <Badge variant="secondary" key={metadata.key}>
                  {metadata.key}: {metadata.value}
                </Badge>
              ))}
            </div>
            <p className="m-0 text-xs text-muted-foreground">
              Fetched {formatSnapshotAge(snapshot.fetched_at)}
            </p>
          </>
        ) : (
          <p className="m-0 text-xs text-muted-foreground">No snapshot yet</p>
        )}
        {object.provider === "github" && object.kind !== "generic" && (
          <form className="grid gap-2" onSubmit={handleComment}>
            <label className="grid gap-1.5 text-sm font-medium">
              <span>Comment on GitHub</span>
              <Textarea
                value={comment}
                onChange={(event) => setComment(event.target.value)}
                rows={2}
                placeholder="Write a short public reply"
                disabled={isSaving || isCommenting}
              />
            </label>
            <Button
              type="submit"
              size="sm"
              variant="outline"
              disabled={isSaving || isCommenting || !comment.trim()}
            >
              {isCommenting ? "Posting…" : "Add comment"}
            </Button>
          </form>
        )}
        {reviewDateReached && (
          <Alert>
            <AlertTitle>Review date reached</AlertTitle>
            <AlertDescription>
              Review scheduled for {externalLink.link.review_at}
            </AlertDescription>
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={isSaving}
              onClick={() => void onClearReviewAt()}
            >
              Clear review date
            </Button>
          </Alert>
        )}
        {externalLink.attention_entry && (
          <Alert>
            <AlertTitle>
              {externalLink.attention_entry.kind === "review"
                ? "Review date reached"
                : "Needs review"}
            </AlertTitle>
            <AlertDescription>
              {externalLink.attention_entry.summary}
            </AlertDescription>
            {externalLink.attention_entry.kind === "review" ? (
              <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={isSaving}
                onClick={() => void onClearReviewAt()}
              >
                Clear review date
              </Button>
            ) : (
              <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={isSaving}
                onClick={() => void onMarkReviewed()}
              >
                Mark changes reviewed
              </Button>
            )}
          </Alert>
        )}
        <div className="grid gap-3 rounded-md border p-3">
          <span className="text-sm font-medium">Watch schedule</span>
          <div className="grid gap-3 sm:grid-cols-2">
            <label className="grid gap-1.5 text-sm font-medium">
              <span>Watch until</span>
              <Input
                type="datetime-local"
                value={watchUntil}
                onChange={(event) => setWatchUntil(event.target.value)}
                disabled={isSaving}
              />
            </label>
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={isSaving}
              onClick={() => void onSaveWatchUntil(watchUntil || null)}
            >
              Save watch period
            </Button>
            <label className="grid gap-1.5 text-sm font-medium">
              <span>Review at</span>
              <Input
                type="datetime-local"
                value={reviewAt}
                onChange={(event) => setReviewAt(event.target.value)}
                disabled={isSaving}
              />
            </label>
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={isSaving}
              onClick={() => void onSaveReviewAt(reviewAt || null)}
            >
              Save review date
            </Button>
          </div>
        </div>
        <div className="grid gap-3 rounded-md border p-3">
          <span className="text-sm font-medium">Attention for this Link</span>
          <div className="flex flex-wrap gap-4">
            <label className="flex items-center gap-2 text-sm font-normal">
              <Checkbox
                checked={policy.title}
                onCheckedChange={(checked) =>
                  setPolicy((current) => ({
                    ...current,
                    title: checked === true,
                  }))
                }
                disabled={isSaving}
              />
              Title
            </label>
            <label className="flex items-center gap-2 text-sm font-normal">
              <Checkbox
                checked={policy.state}
                onCheckedChange={(checked) =>
                  setPolicy((current) => ({
                    ...current,
                    state: checked === true,
                  }))
                }
                disabled={isSaving}
              />
              State
            </label>
            <label className="flex items-center gap-2 text-sm font-normal">
              <Checkbox
                checked={policy.metadata}
                onCheckedChange={(checked) =>
                  setPolicy((current) => ({
                    ...current,
                    metadata: checked === true,
                  }))
                }
                disabled={isSaving}
              />
              Metadata
            </label>
          </div>
          <div className="flex flex-wrap gap-2">
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={isSaving}
              onClick={() => void onSavePolicy(policy)}
            >
              Save Link policy
            </Button>
            {externalLink.link.attention_policy && (
              <Button
                type="button"
                size="sm"
                variant="ghost"
                disabled={isSaving}
                onClick={() => void onSavePolicy(null)}
              >
                Use Context default
              </Button>
            )}
          </div>
        </div>
      </CardContent>
    </Card>
  );
}

export function AttentionEntryCard({
  entry,
  item,
  onMarkedReviewed,
}: {
  entry: AttentionEntry;
  item: ItemView | undefined;
  onMarkedReviewed: () => Promise<void>;
}) {
  const [isSaving, setIsSaving] = useState(false);
  const workCommand = useWorkCommand();

  async function markReviewed() {
    setIsSaving(true);
    try {
      await workCommand.execute(workActions.markLinkReviewed(entry.link_id));
      await onMarkedReviewed();
    } catch (reviewError) {
      window.alert(errorMessage(reviewError));
    } finally {
      setIsSaving(false);
    }
  }

  return (
    <Card size="sm">
      <CardContent className="flex flex-wrap items-center justify-between gap-3 pt-4">
        <div className="min-w-0">
          <strong className="block text-sm">{entry.source_title}</strong>
          <span className="text-xs text-muted-foreground">
            {displayItemIdentifier(item?.item.human_identifier ?? "Item")} ·{" "}
            {attentionEntryLabel(entry)}
          </span>
        </div>
        <p className="m-0 min-w-0 flex-1 text-sm text-muted-foreground">
          {entry.summary}
        </p>
        {entry.kind === "reminder" && item && entry.reminder_id !== null ? (
          <Button
            type="button"
            size="sm"
            variant="outline"
            disabled={isSaving}
            onClick={() =>
              void saveReminder(item.item.id, entry.reminder_id as number)
            }
          >
            Dismiss reminder
          </Button>
        ) : entry.kind === "review" ? (
          <Button
            type="button"
            size="sm"
            variant="outline"
            disabled={isSaving}
            onClick={() => void clearReviewDate()}
          >
            Clear review date
          </Button>
        ) : entry.kind === "blocked_run" ? (
          <span className="text-xs text-muted-foreground">
            Open the Run to answer the agent.
          </span>
        ) : (
          <Button
            type="button"
            size="sm"
            variant="outline"
            disabled={isSaving}
            onClick={() => void markReviewed()}
          >
            Mark reviewed
          </Button>
        )}
      </CardContent>
    </Card>
  );

  async function saveReminder(itemId: number, reminderId: number) {
    setIsSaving(true);
    try {
      await workCommand.execute(workActions.removeReminder(itemId, reminderId));
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
      await workCommand.execute(workActions.clearLinkReviewAt(entry.link_id));
      await onMarkedReviewed();
    } catch (clearError) {
      window.alert(errorMessage(clearError));
    } finally {
      setIsSaving(false);
    }
  }
}

export function RunSuggestionCard({
  suggestion,
  disabled,
  onAttach,
}: {
  suggestion: RunSuggestion;
  disabled: boolean;
  onAttach: (suggestion: RunSuggestion) => Promise<void>;
}) {
  return (
    <Card size="sm">
      <CardContent className="grid gap-3 pt-4 md:grid-cols-[1fr_1fr_auto] md:items-center">
        <div className="grid gap-1">
          <strong className="text-sm">
            {suggestion.agent === "claude" ? "Claude Code" : "Codex"} in{" "}
            {suggestion.machineName}
          </strong>
          <span className="text-xs text-muted-foreground">
            {displayItemIdentifier(suggestion.itemIdentifier)} ·{" "}
            {suggestion.itemTitle} ·{" "}
            {suggestion.contextName}
          </span>
        </div>
        <div className="grid gap-1 text-xs">
          <strong>
            {suggestion.workspaceId
              ? `Workspace #${suggestion.workspaceId} · ${
                  suggestion.worktreeId ? "Worktree" : "Direct checkout"
                }`
              : "Unregistered working location"}
          </strong>
          <span className="break-all text-muted-foreground">
            {suggestion.locationPath ?? suggestion.currentPath}
          </span>
          {suggestion.repositoryId !== null && (
            <span className="text-muted-foreground">
              Repository #{suggestion.repositoryId}
            </span>
          )}
          <code className="break-all text-muted-foreground">
            Session {suggestion.sessionName} · Pane {suggestion.paneId} ·{" "}
            {suggestion.currentPath}
          </code>
        </div>
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={disabled}
          onClick={() => void onAttach(suggestion)}
        >
          Attach Run
        </Button>
      </CardContent>
    </Card>
  );
}

function attentionEntryLabel(entry: AttentionEntry): string {
  if (entry.kind === "reminder") return "Reminder due";
  if (entry.kind === "review") return "Review date reached";
  if (entry.kind === "blocked_run") return "Run blocked";
  return `${entry.activities.length} change${entry.activities.length === 1 ? "" : "s"}`;
}

function runStateLabel(state: RunState, isGrill = false): string {
  if (state === "working") return "Working";
  if (state === "blocked") return isGrill ? "Waiting for answers" : "Blocked";
  if (state === "finished") return "Finished";
  return "Unknown";
}

export function SearchResult({ view }: { view: ItemView }) {
  return (
    <Card size="sm">
      <CardContent className="flex items-center gap-3 pt-4">
        <Badge variant="outline">
          {displayItemIdentifier(view.item.human_identifier)}
        </Badge>
        <div>
          <h3 className="font-heading text-sm font-medium normal-case tracking-normal text-foreground">
            {view.item.title}
          </h3>
          <p className="m-0 text-xs text-muted-foreground">
            {view.context_name} <span>·</span> {view.project_name}{" "}
            <span>·</span> {view.item.status}
          </p>
        </div>
      </CardContent>
    </Card>
  );
}
