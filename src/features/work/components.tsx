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
  ExecutionProfile,
  ExternalChangePolicy,
  ExternalLinkView,
  ExternalObjectDeletionPreview,
  ItemStatus,
  ItemDeletionPreview,
  ItemRelationKind,
  ItemView,
  Machine,
  Repository,
  Run,
  RunPromptSelection,
  RunState,
  RunSuggestion,
  Workset,
  WorksetRemovalReport,
  WorksetRepositoryInput,
} from "../../runtime/types";
import type { PaneTab } from "../../runtime/terminal-types";
import {
  externalObjectKindLabel,
  findWorkset,
  formatSnapshotAge,
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
  const [isReminderDialogOpen, setIsReminderDialogOpen] = useState(false);
  const [isRenameDialogOpen, setIsRenameDialogOpen] = useState(false);
  const [titleDraft, setTitleDraft] = useState(view.item.title);
  const [issueRepository, setIssueRepository] = useState("");
  const [issueTitle, setIssueTitle] = useState(view.item.title);
  const [issueBody, setIssueBody] = useState(view.item.notes);
  const [worksetRoot, setWorksetRoot] = useState("");
  const [worksetBranch, setWorksetBranch] = useState("");
  const [attachWorksetRoot, setAttachWorksetRoot] = useState("");
  const [selectedRepositoryIds, setSelectedRepositoryIds] = useState<number[]>(
    [],
  );
  const [worksetBranchOverrides, setWorksetBranchOverrides] = useState<
    Record<number, string>
  >({});
  const [worksetBaseBranchOverrides, setWorksetBaseBranchOverrides] = useState<
    Record<number, string>
  >({});
  const [additionalRepositoryId, setAdditionalRepositoryId] =
    useState<number>();
  const [additionalBranchOverride, setAdditionalBranchOverride] = useState("");
  const [additionalBaseBranchOverride, setAdditionalBaseBranchOverride] =
    useState("");
  const [removalReport, setRemovalReport] = useState<WorksetRemovalReport>();
  const [deletionPreview, setDeletionPreview] = useState<ItemDeletionPreview>();
  const [deleteWorksetDirectories, setDeleteWorksetDirectories] =
    useState(false);
  const [confirmation, setConfirmation] = useState<WorkConfirmation>();
  const [externalObjectDeletionPreview, setExternalObjectDeletionPreview] =
    useState<ExternalObjectDeletionPreview>();
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
  const [isExpanded, setIsExpanded] = useState(false);
  const workCommand = useWorkCommand();

  const displayIdentifier = displayItemIdentifier(view.item.human_identifier);

  const itemRepositories = repositories.filter(
    (repository) => repository.project_id === view.item.project_id,
  );

  useEffect(() => {
    setNotes(view.item.notes);
  }, [view.item.notes]);

  useEffect(() => {
    setTitleDraft(view.item.title);
  }, [view.item.title]);

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

  async function handleCreateWorkset(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (
      !worksetRoot.trim() ||
      !worksetBranch.trim() ||
      selectedRepositoryIds.length === 0
    ) {
      return;
    }
    const selected: WorksetRepositoryInput[] = selectedRepositoryIds.map(
      (repositoryId) => ({
        repositoryId,
        branchOverride: worksetBranchOverrides[repositoryId]?.trim() || null,
        baseBranchOverride:
          worksetBaseBranchOverrides[repositoryId]?.trim() || null,
      }),
    );
    await saveItem(
      workActions.createWorkset(
        view.item.id,
        worksetRoot,
        worksetBranch,
        selected,
      ),
    );
    setWorksetRoot("");
    setWorksetBranch("");
    setSelectedRepositoryIds([]);
    setWorksetBranchOverrides({});
    setWorksetBaseBranchOverrides({});
  }

  async function handleAttachWorkset(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!attachWorksetRoot.trim()) return;
    await saveItem(workActions.attachWorkset(view.item.id, attachWorksetRoot));
    setAttachWorksetRoot("");
  }

  async function handleAddRepositoryToWorkset(worksetId: number) {
    if (!additionalRepositoryId) return;
    await saveItem(
      workActions.addRepositoryToWorkset(
        worksetId,
        additionalRepositoryId,
        additionalBranchOverride.trim() || null,
        additionalBaseBranchOverride.trim() || null,
      ),
    );
    setAdditionalRepositoryId(undefined);
    setAdditionalBranchOverride("");
    setAdditionalBaseBranchOverride("");
  }

  async function handleSetWorksetArchived(
    worksetId: number,
    archived: boolean,
  ) {
    await saveItem(workActions.setWorksetArchived(worksetId, archived));
    if (removalReport?.workset_id === worksetId) {
      setRemovalReport(undefined);
    }
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

  function handleDeleteRun(run: Run) {
    if (run.state !== "finished") return;
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

  async function handlePrepareRemoval(worksetId: number) {
    setIsSaving(true);
    try {
      const report = await workCommand.execute(
        workActions.prepareWorksetRemoval(worksetId),
        false,
      );
      setRemovalReport(report);
    } catch (reportError) {
      window.alert(errorMessage(reportError));
    } finally {
      setIsSaving(false);
    }
  }

  async function executeRemoveWorkset(worksetId: number) {
    const result = await saveItem(workActions.removeWorkset(worksetId));
    if (!result) return;
    setRemovalReport(undefined);
    if (result.physicalCleanupWarning) {
      window.alert(result.physicalCleanupWarning);
    }
  }

  function handleRemoveWorkset(worksetId: number) {
    if (removalReport?.workset_id !== worksetId) return;
    setConfirmation({
      title: "Remove this Workset?",
      description: "The Workset record and its directory will be removed from disk.",
      confirmLabel: "Remove Workset",
      onConfirm: () => {
        setConfirmation(undefined);
        void executeRemoveWorkset(worksetId);
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
      setDeleteWorksetDirectories(false);
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
        workActions.deleteItem(view.item.id, deleteWorksetDirectories),
      );
      setDeletionPreview(undefined);
      setDeleteWorksetDirectories(false);
      await onChanged();
      const summary = result.summary;
      const physicalWarning = result.physicalCleanupWarning
        ? `\n\n${result.physicalCleanupWarning}`
        : "";
      window.alert(
        `Deleted ${displayItemIdentifier(deletionPreview.plan.humanIdentifier)}.\n\nRemoved ${summary.reminderCount} reminder(s), ${summary.relationshipCount} relationship(s), ${summary.worksetCount} Workset(s), ${summary.runCount} Run(s), ${summary.linkCount} Link(s), and ${summary.externalObjectCount} orphaned External Object(s).${physicalWarning}`,
      );
    } catch (deleteError) {
      setDeletionPreview(undefined);
      setDeleteWorksetDirectories(false);
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
        (run) => run.state !== "finished" && run.pane_status !== "missing",
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
    if (!runPreviewWorksetId) return;
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
      const composed = await workCommand.execute(
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
    if (!runPreviewWorksetId || !runPrompt.trim() || runPromptNeedsCompose)
      return;
    await saveItem(
      workActions.startRun({
        itemId: view.item.id,
        worksetId: runPreviewWorksetId,
        machineId: runMachineId ?? null,
        agent: runAgent,
        executionProfile: runProfile,
        prompt: runPrompt,
        promptSelection: runPromptSelection(),
      }),
    );
    setRunPreviewWorksetId(undefined);
    setRunPrompt("");
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
    const report =
      removalReport?.workset_id === workset.id ? removalReport : undefined;

    return (
      <Card
        size="sm"
        className={archived ? "border-dashed opacity-80" : ""}
        key={workset.id}
      >
        <CardHeader className="border-b border-border/70">
          <div>
            <CardTitle className="flex items-center gap-2">
              {workset.branch}
              {archived && <Badge variant="outline">Archived</Badge>}
            </CardTitle>
            <CardDescription className="mt-1 break-all font-mono text-xs">
              {workset.root_directory}
            </CardDescription>
          </div>
        </CardHeader>
        <CardContent className="space-y-3 pt-4">
          <div className="flex flex-wrap gap-2">
            {workset.repositories.map((selected) => (
              <Badge
                variant="secondary"
                className="h-auto items-start gap-1 py-1"
                key={selected.repository_id}
              >
                <span className="font-medium">
                  {repositoryName(repositories, selected.repository_id)}
                </span>
                <span className="text-muted-foreground">
                  {selected.current_branch} ·{" "}
                  {selected.is_dirty ? "uncommitted changes" : "clean"}
                </span>
              </Badge>
            ))}
          </div>
          <div className="flex flex-wrap gap-2">
            {!archived && (
              <Button
                type="button"
                size="sm"
                disabled={isSaving}
                onClick={() => void openRunPreview(workset)}
              >
                Start Run
              </Button>
            )}
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={isSaving}
              onClick={() =>
                void handleSetWorksetArchived(workset.id, !archived)
              }
            >
              {archived ? "Restore" : "Archive"}
            </Button>
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={isSaving}
              onClick={() => void handlePrepareRemoval(workset.id)}
            >
              Review removal
            </Button>
          </div>
          {report && (
            <Alert
              variant={report.blockers.length > 0 ? "destructive" : "default"}
            >
              <AlertTitle>Removal safety report</AlertTitle>
              <AlertDescription className="space-y-3">
                <p>{report.root_directory} will be removed from disk.</p>
                {report.repositories.map((repository) => (
                  <div className="grid gap-1" key={repository.repository_id}>
                    <strong>{repository.name}</strong>
                    <span>
                      {repository.current_branch} · {repository.path}
                    </span>
                    {repository.unpushed_commits_unknown ? (
                      <p>
                        Unpushed commits could not be verified: no upstream
                        branch is configured.
                      </p>
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
                {report.blockers.length > 0 && (
                  <div className="grid gap-1">
                    <strong>Removal blocked</strong>
                    {report.blockers.map((blocker) => (
                      <span key={blocker}>{blocker}</span>
                    ))}
                    <p>Resolve each finding, then create a fresh report.</p>
                  </div>
                )}
              </AlertDescription>
              <div className="mt-3 flex flex-wrap gap-2">
                <Button
                  type="button"
                  size="sm"
                  disabled={isSaving || !report.safe}
                  onClick={() => void handleRemoveWorkset(workset.id)}
                >
                  Confirm and remove
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  disabled={isSaving}
                  onClick={() => setRemovalReport(undefined)}
                >
                  Keep Workset
                </Button>
              </div>
            </Alert>
          )}
          {runPreviewWorksetId === workset.id && (
            <form
              className="grid gap-4 rounded-lg border border-primary/30 bg-primary/5 p-4"
              onSubmit={handleStartRun}
            >
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div>
                  <h4 className="m-0 text-base font-medium">Confirm Run</h4>
                  <p className="mt-1 text-sm text-muted-foreground">
                    Context: {view.context_name} · Project: {view.project_name}{" "}
                    · Workset: {workset.branch}
                  </p>
                </div>
                <Badge variant="outline">
                  Machine:{" "}
                  {itemMachines.find((machine) => machine.id === runMachineId)
                    ?.name ?? "Local Mac"}
                </Badge>
              </div>
              <p className="m-0 text-sm text-muted-foreground">
                Working directory:{" "}
                <code className="break-all font-mono text-xs">
                  {workset.root_directory}
                </code>
              </p>
              <div className="grid gap-3 md:grid-cols-3">
                <label className="grid gap-1.5 text-sm font-medium">
                  <span>Machine</span>
                  <NativeSelect
                    value={runMachineId ?? ""}
                    onChange={(event) =>
                      setRunMachineId(Number(event.target.value) || undefined)
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
                    <NativeSelectOption value="review">
                      Review
                    </NativeSelectOption>
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
              <fieldset className="grid gap-2 rounded-md border p-3">
                <legend className="px-1 text-sm font-medium">
                  Include explicitly selected content
                </legend>
                <label className="flex items-center gap-2 text-sm font-normal">
                  <Checkbox
                    checked={includeRunObjective}
                    onCheckedChange={(checked) => {
                      setIncludeRunObjective(checked === true);
                      setRunPromptNeedsCompose(true);
                    }}
                    disabled={isSaving}
                  />
                  Item objective
                </label>
                {view.item.notes.trim() && (
                  <label className="flex items-center gap-2 text-sm font-normal">
                    <Checkbox
                      checked={includeRunNotes}
                      onCheckedChange={(checked) => {
                        setIncludeRunNotes(checked === true);
                        setRunPromptNeedsCompose(true);
                      }}
                      disabled={isSaving}
                    />
                    Item notes
                  </label>
                )}
                {view.links.map((link) => (
                  <label
                    className="flex items-center gap-2 text-sm font-normal"
                    key={link.object.id}
                  >
                    <Checkbox
                      checked={selectedRunExternalObjectIds.includes(
                        link.object.id,
                      )}
                      onCheckedChange={(checked) => {
                        setSelectedRunExternalObjectIds((current) =>
                          checked === true
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
              </fieldset>
              <label className="grid gap-1.5 text-sm font-medium">
                <span>Editable composed prompt</span>
                <Textarea
                  value={runPrompt}
                  onChange={(event) => setRunPrompt(event.target.value)}
                  rows={7}
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
                    isSaving || !runPrompt.trim() || runPromptNeedsCompose
                  }
                >
                  {isSaving
                    ? "Starting…"
                    : runPromptNeedsCompose
                      ? "Compose before starting"
                      : "Confirm and start Run"}
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  disabled={isSaving}
                  onClick={() => setRunPreviewWorksetId(undefined)}
                >
                  Cancel
                </Button>
              </div>
            </form>
          )}
          {!archived && availableRepositories.length > 0 && (
            <div className="grid gap-2 rounded-md border p-3 md:grid-cols-[1.2fr_1fr_1fr_auto] md:items-end">
              <NativeSelect
                aria-label={`Repository to add to Workset ${workset.id}`}
                value={additionalRepositoryId ?? ""}
                onChange={(event) =>
                  setAdditionalRepositoryId(
                    Number(event.target.value) || undefined,
                  )
                }
                disabled={isSaving}
              >
                <NativeSelectOption value="">
                  Add a Repository
                </NativeSelectOption>
                {availableRepositories.map((repository) => (
                  <NativeSelectOption value={repository.id} key={repository.id}>
                    {repository.name}
                  </NativeSelectOption>
                ))}
              </NativeSelect>
              <Input
                aria-label="Added Repository branch override"
                value={additionalBranchOverride}
                onChange={(event) =>
                  setAdditionalBranchOverride(event.target.value)
                }
                placeholder="Branch override (optional)"
                disabled={isSaving}
              />
              <Input
                aria-label="Added Repository base branch override"
                value={additionalBaseBranchOverride}
                onChange={(event) =>
                  setAdditionalBaseBranchOverride(event.target.value)
                }
                placeholder="Base branch override (optional)"
                disabled={isSaving}
              />
              <Button
                type="button"
                variant="outline"
                disabled={isSaving || !additionalRepositoryId}
                onClick={() => void handleAddRepositoryToWorkset(workset.id)}
              >
                Add
              </Button>
            </div>
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
              const runWorkset = findWorkset(view, run.workset_id);
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
                        {machines.find(
                          (machine) => machine.id === run.machine_id,
                        )?.name ?? "Machine #" + run.machine_id}{" "}
                        · {runStateLabel(run.state)}
                        {run.pane_status === "available" && " · Pane available"}
                      </span>
                    </div>
                    <code className="break-all font-mono text-xs">
                      {run.working_directory}
                    </code>
                    <span className="text-xs text-muted-foreground">
                      Session {run.session_name} · Pane {run.pane_id}
                    </span>
                    {run.pane_status === "missing" && (
                      <span className="text-xs text-destructive">
                        Pane missing. Decide whether to start another Run.
                      </span>
                    )}
                    {run.pane_status === "unknown" && (
                      <span className="text-xs text-muted-foreground">
                        Pane status not confirmed.
                      </span>
                    )}
                    {runWorkset && (
                      <div className="flex flex-wrap gap-2">
                        <Button
                          type="button"
                          size="sm"
                          variant="outline"
                          disabled={isSaving || run.pane_status === "missing"}
                          onClick={() =>
                            onOpenTerminal(run.workset_id, paneTabForRun(run))
                          }
                        >
                          Open embedded terminal
                        </Button>
                        <Button
                          type="button"
                          size="sm"
                          variant="outline"
                          disabled={isSaving || run.pane_status === "missing"}
                          onClick={() =>
                            void saveItem(
                              workActions.openExternalTerminal(run.id),
                              false,
                            )
                          }
                        >
                          Open in Terminal
                        </Button>
                        {run.state !== "finished" &&
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
                        {run.state === "finished" && (
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
                        {run.pane_status === "missing" &&
                          !runWorkset.archived && (
                            <Button
                              type="button"
                              size="sm"
                              variant="outline"
                              disabled={isSaving}
                              onClick={() => void openRunPreview(runWorkset)}
                            >
                              Start a new Run
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
            Worksets
          </span>
          {view.worksets.map((workset) => renderWorksetCard(workset, false))}
          {view.archived_worksets.length > 0 && (
            <div className="grid gap-2">
              <span className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
                Archived Worksets
              </span>
              {view.archived_worksets.map((workset) =>
                renderWorksetCard(workset, true),
              )}
            </div>
          )}
          <Card size="sm">
            <CardHeader>
              <CardTitle className="text-sm">Create a Workset</CardTitle>
              <CardDescription>
                Choose repositories and prepare a shared working environment.
              </CardDescription>
            </CardHeader>
            <CardContent>
              <form className="grid gap-3" onSubmit={handleCreateWorkset}>
                <label className="grid gap-1.5 text-sm font-medium">
                  <span>Root directory</span>
                  <Input
                    value={worksetRoot}
                    onChange={(event) => setWorksetRoot(event.target.value)}
                    placeholder="/Users/me/worksets/PLAT-847"
                    disabled={isSaving}
                  />
                </label>
                <label className="grid gap-1.5 text-sm font-medium">
                  <span>Logical branch</span>
                  <Input
                    value={worksetBranch}
                    onChange={(event) => setWorksetBranch(event.target.value)}
                    placeholder="feature/PLAT-847"
                    disabled={isSaving}
                  />
                </label>
                <span className="text-sm font-medium">Select repositories</span>
                {itemRepositories.length === 0 ? (
                  <span className="text-sm text-muted-foreground">
                    Register a Repository under this Project first.
                  </span>
                ) : (
                  <div className="grid gap-2">
                    {itemRepositories.map((repository) => {
                      const selected = selectedRepositoryIds.includes(
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
                              onCheckedChange={(checked) =>
                                setSelectedRepositoryIds((current) =>
                                  checked === true
                                    ? [...current, repository.id]
                                    : current.filter(
                                        (id) => id !== repository.id,
                                      ),
                                )
                              }
                              disabled={isSaving}
                            />
                            {repository.name}
                          </label>
                          {selected && (
                            <div className="grid gap-2 sm:grid-cols-2">
                              <Input
                                aria-label={`${repository.name} branch override`}
                                value={
                                  worksetBranchOverrides[repository.id] ?? ""
                                }
                                onChange={(event) =>
                                  setWorksetBranchOverrides((current) => ({
                                    ...current,
                                    [repository.id]: event.target.value,
                                  }))
                                }
                                placeholder="Branch override (optional)"
                                disabled={isSaving}
                              />
                              <Input
                                aria-label={`${repository.name} base branch override`}
                                value={
                                  worksetBaseBranchOverrides[repository.id] ??
                                  ""
                                }
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
                <Button
                  type="submit"
                  disabled={
                    isSaving ||
                    !worksetRoot.trim() ||
                    !worksetBranch.trim() ||
                    selectedRepositoryIds.length === 0
                  }
                >
                  {isSaving ? "Checking out…" : "Create Workset"}
                </Button>
              </form>
            </CardContent>
          </Card>
          <Card size="sm">
            <CardHeader>
              <CardTitle className="text-sm">
                Attach an Existing Workset
              </CardTitle>
              <CardDescription>
                Inspect direct child repositories without modifying Git.
              </CardDescription>
            </CardHeader>
            <CardContent>
              <form className="grid gap-3" onSubmit={handleAttachWorkset}>
                <label className="grid gap-1.5 text-sm font-medium">
                  <span>Existing root directory</span>
                  <Input
                    value={attachWorksetRoot}
                    onChange={(event) =>
                      setAttachWorksetRoot(event.target.value)
                    }
                    placeholder="/Users/me/worksets/PLAT-847"
                    disabled={isSaving}
                  />
                </label>
                <p className="m-0 text-sm text-muted-foreground">
                  Inspects direct child repositories, branches, and uncommitted
                  changes without modifying Git.
                </p>
                <Button
                  type="submit"
                  disabled={isSaving || !attachWorksetRoot.trim()}
                >
                  {isSaving ? "Inspecting…" : "Attach Workset"}
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
              setDeleteWorksetDirectories(false);
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
                <li>{deletionPreview.plan.worksets.length} Workset(s)</li>
                <li>{deletionPreview.plan.runIds.length} Run(s)</li>
                <li>{deletionPreview.plan.linkIds.length} Link(s)</li>
                <li>
                  {deletionPreview.plan.orphanedExternalObjectIds.length}{" "}
                  orphaned External Object(s),{" "}
                  {deletionPreview.plan.orphanedSnapshotCount} snapshot(s), and{" "}
                  {deletionPreview.plan.orphanedActivityCount} Activity record(s)
                </li>
              </ul>
              {deletionPreview.worksets.length > 0 && (
                <div className="grid gap-2">
                  <span className="font-medium">Workset directories</span>
                  {deletionPreview.worksets.map((workset) => (
                    <div
                      className="grid gap-1 rounded-md border p-2"
                      key={workset.worksetId}
                    >
                      <strong>
                        {workset.branch} {workset.archived ? "· Archived" : ""}
                      </strong>
                      <code className="break-all font-mono text-xs">
                        {workset.rootDirectory}
                      </code>
                      {!workset.safe &&
                        workset.blockers.map((blocker) => (
                          <span className="text-destructive" key={blocker}>
                            {blocker}
                          </span>
                        ))}
                    </div>
                  ))}
                </div>
              )}
              {deletionPreview.worksets.length > 0 &&
                deletionPreview.blockers.length === 0 && (
                  <label className="flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/5 p-3">
                    <Checkbox
                      checked={deleteWorksetDirectories}
                      onCheckedChange={(checked) =>
                        setDeleteWorksetDirectories(checked === true)
                      }
                      disabled={isSaving}
                    />
                    <span className="grid gap-1">
                      <span className="font-medium">
                        Also delete Workset directories from disk
                      </span>
                      <span className="text-muted-foreground">
                        Leave this unchecked to remove only the local records.
                      </span>
                    </span>
                  </label>
                )}
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
                  setDeleteWorksetDirectories(false);
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
          <strong>Likely Workset: {suggestion.worksetBranch}</strong>
          <span className="break-all text-muted-foreground">
            {suggestion.worksetRootDirectory}
          </span>
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

function runStateLabel(state: RunState): string {
  if (state === "working") return "Working";
  if (state === "blocked") return "Blocked";
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
