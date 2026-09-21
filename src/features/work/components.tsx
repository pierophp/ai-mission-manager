import { FormEvent, useEffect, useState } from "react";
import { currentMinute } from "../../runtime/time";
import { errorMessage } from "../../runtime/errors";
import { workAdapter } from "../../runtime/adapters";
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
  PaneTab,
  Repository,
  Run,
  RunPromptSelection,
  RunState,
  RunSuggestion,
  Workset,
  WorksetRemovalReport,
  WorksetRepositoryInput,
} from "../../runtime/types";
import {
  externalObjectKindLabel,
  findWorkset,
  formatSnapshotAge,
  paneTabForRun,
  relationKindLabel,
  relationshipLabel,
  repositoryName,
} from "./work-utils";

const itemStatuses: ItemStatus[] = ["Inbox", "Active", "Waiting", "Done"];
const relationKinds: ItemRelationKind[] = [
  "Blocks",
  "BlockedBy",
  "RelatedTo",
];

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
  const [deletionPreview, setDeletionPreview] = useState<ItemDeletionPreview>();
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
      workAdapter.setRelation(view.item.id, targetItemId, relationKind),
    );
    setTargetItemId(undefined);
  }

  async function handleExternalLink(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!externalUrl.trim()) return;
    setIsSaving(true);
    try {
      const result = await workAdapter.linkExternalObject(view.item.id, externalUrl);
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
      await workAdapter.createWorkset(
        view.item.id,
        worksetRoot,
        worksetBranch,
        selected,
      );
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
      await workAdapter.attachWorkset(view.item.id, attachWorksetRoot);
      setAttachWorksetRoot("");
    });
  }

  async function handleAddRepositoryToWorkset(worksetId: number) {
    if (!additionalRepositoryId) return;
    await saveItem(async () => {
      await workAdapter.addRepositoryToWorkset(
        worksetId,
        additionalRepositoryId,
        additionalBranchOverride.trim() || null,
        additionalBaseBranchOverride.trim() || null,
      );
      setAdditionalRepositoryId(undefined);
      setAdditionalBranchOverride("");
      setAdditionalBaseBranchOverride("");
    });
  }

  async function handleSetWorksetArchived(worksetId: number, archived: boolean) {
    await saveItem(() =>
      workAdapter.setWorksetArchived(worksetId, archived),
    );
    if (removalReport?.workset_id === worksetId) {
      setRemovalReport(undefined);
    }
  }

  async function handleStopRun(run: Run) {
    if (
      !window.confirm(
        `Stop Run #${run.id}? This is separate from completing the Item and will leave the Run in its history.`,
      )
    ) {
      return;
    }
    await saveItem(() => workAdapter.stopRun(run.id));
  }

  async function handleDeleteRun(run: Run) {
    if (run.state !== "finished") return;
    if (
      !window.confirm(
        `Delete finished Run #${run.id} from Run history? This cannot be undone.`,
      )
    ) {
      return;
    }
    await saveItem(async () => {
      await workAdapter.deleteRun(run.id);
    });
  }

  async function handlePrepareRemoval(worksetId: number) {
    setIsSaving(true);
    try {
      const report = await workAdapter.prepareWorksetRemoval(worksetId);
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
      const result = await workAdapter.removeWorkset(worksetId);
      setRemovalReport(undefined);
      if (result.physicalCleanupWarning) {
        window.alert(result.physicalCleanupWarning);
      }
    });
  }

  async function handlePrepareItemDeletion() {
    setIsSaving(true);
    try {
      const preview = await workAdapter.prepareItemDeletion(view.item.id);
      setDeletionPreview(preview);
    } catch (previewError) {
      window.alert(errorMessage(previewError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleDeleteItem() {
    if (!deletionPreview || deletionPreview.blockers.length > 0) return;
    if (
      !window.confirm(
        `Delete ${deletionPreview.plan.humanIdentifier} and its local records? This cannot be undone.`,
      )
    ) {
      return;
    }
    const deleteWorksetDirectories =
      deletionPreview.worksets.length > 0 &&
      window.confirm(
        `Permanently delete these ${deletionPreview.worksets.length} Workset director${deletionPreview.worksets.length === 1 ? "y" : "ies"} from disk too? Choose Cancel to delete the Item records while leaving the directories in place.`,
      );

    setIsSaving(true);
    try {
      const result = await workAdapter.deleteItem(view.item.id, deleteWorksetDirectories);
      setDeletionPreview(undefined);
      await onChanged();
      const summary = result.summary;
      const physicalWarning = result.physicalCleanupWarning
        ? `\n\n${result.physicalCleanupWarning}`
        : "";
      window.alert(
        `Deleted ${deletionPreview.plan.humanIdentifier}.\n\nRemoved ${summary.reminderCount} reminder(s), ${summary.relationshipCount} relationship(s), ${summary.worksetCount} Workset(s), ${summary.runCount} Run(s), ${summary.linkCount} Link(s), and ${summary.externalObjectCount} orphaned External Object(s).${physicalWarning}`,
      );
    } catch (deleteError) {
      setDeletionPreview(undefined);
      window.alert(errorMessage(deleteError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleUnlinkExternalLink(linkId: number) {
    if (
      !window.confirm(
        "Remove this Link from the Item? Its Link-scoped attention state will be removed. The GitHub Issue, pull request, or other provider-owned object will not be deleted.",
      )
    ) {
      return;
    }
    await saveItem(async () => {
      const result = await workAdapter.unlinkExternalLink(linkId);
      if (result.externalObjectDeleted) {
        window.alert(
          "The Link was removed. It was the last Link, so its local External Object snapshot and Activity cache were also removed. The provider-owned object was not deleted.",
        );
      }
    });
  }

  async function handlePrepareExternalObjectDeletion(externalObjectId: number) {
    setIsSaving(true);
    try {
      const preview = await workAdapter.prepareExternalObjectDeletion(externalObjectId);
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
    if (
      !window.confirm(
        `Remove this ${externalObjectKindLabel(plan.kind)} and its local records from Mission Manager? This will remove ${plan.linkIds.length} Link(s), ${plan.snapshotCount} snapshot(s), and ${plan.activityCount} Activity record(s). Provider-owned objects are never deleted.`,
      )
    ) {
      return;
    }
    setIsSaving(true);
    try {
      const result = await workAdapter.deleteExternalObject(plan.externalObjectId);
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
      const composed = await workAdapter.composeRunPrompt(
        view.item.id,
        runProfile,
        runPromptSelection(),
        runProfile === "custom" ? runCustomPrompt : null,
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
      const composed = await workAdapter.composeRunPrompt(
        view.item.id,
        "implement",
        {
          includeObjective: true,
          includeNotes: Boolean(view.item.notes.trim()),
          externalObjectIds: [],
        },
        null,
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
    if (!runPreviewWorksetId || !runPrompt.trim() || runPromptNeedsCompose) return;
    await saveItem(async () => {
      await workAdapter.startRun({
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
      await workAdapter.refreshExternalObject(externalObjectId);
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
      const result = await workAdapter.createGithubIssue(
        view.item.id,
        issueRepository,
        issueTitle,
        issueBody,
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
            {report.blockers.length > 0 && (
              <div className="deletion-blockers">
                <strong>Removal blocked</strong>
                {report.blockers.map((blocker) => (
                  <span key={blocker}>{blocker}</span>
                ))}
                <p>Resolve each finding, then create a fresh report.</p>
              </div>
            )}
            <div className="workset-actions">
              <button
                type="button"
                disabled={isSaving || !report.safe}
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
          onChange={(event) => {
            const nextStatus = event.target.value as ItemStatus;
            if (nextStatus === "Done" && view.item.status !== "Done") {
              const activeRuns = view.runs.filter(
                (run) => run.state !== "finished" && run.pane_status !== "missing",
              );
              if (
                activeRuns.length > 0 &&
                !window.confirm(
                  `${activeRuns.length} Run${activeRuns.length === 1 ? " is" : "s are"} still active. Complete the Item without stopping them?`,
                )
              ) {
                return;
              }
            }
            void saveItem(() =>
              workAdapter.setItemStatus(view.item.id, nextStatus),
            );
          }}
          disabled={isSaving}
        >
          {itemStatuses.map((status) => (
            <option value={status} key={status}>
              {status}
            </option>
          ))}
        </select>
        <button
          type="button"
          className="danger-button"
          disabled={isSaving}
          onClick={() => void handlePrepareItemDeletion()}
        >
          Delete Item
        </button>
      </div>
      <h4>{view.item.title}</h4>
      <p className="item-context">
        {view.context_name} <span>·</span> {view.project_name}
      </p>
      {deletionPreview && (
        <div className="deletion-preview" role="alert">
          <strong>Item deletion preview</strong>
          <p>
            This removes <b>{deletionPreview.plan.humanIdentifier}</b> ·{" "}
            {deletionPreview.plan.title} and its local descendants. External Issues and
            pull requests are never changed.
          </p>
          <ul>
            <li>{deletionPreview.plan.reminderCount} reminder(s)</li>
            <li>{deletionPreview.plan.relationshipCount} Item relationship(s)</li>
            <li>{deletionPreview.plan.worksets.length} Workset(s)</li>
            <li>{deletionPreview.plan.runIds.length} Run(s)</li>
            <li>{deletionPreview.plan.linkIds.length} Link(s)</li>
            <li>
              {deletionPreview.plan.orphanedExternalObjectIds.length} orphaned External
              Object(s), {deletionPreview.plan.orphanedSnapshotCount} snapshot(s), and{" "}
              {deletionPreview.plan.orphanedActivityCount} Activity record(s)
            </li>
          </ul>
          {deletionPreview.worksets.length > 0 && (
            <div className="deletion-worksets">
              <span className="relationship-label">Workset directories</span>
              {deletionPreview.worksets.map((workset) => (
                <div className="deletion-workset" key={workset.worksetId}>
                  <strong>
                    {workset.branch} {workset.archived ? "· Archived" : ""}
                  </strong>
                  <code>{workset.rootDirectory}</code>
                  {!workset.safe &&
                    workset.blockers.map((blocker) => <span key={blocker}>{blocker}</span>)}
                </div>
              ))}
            </div>
          )}
          {deletionPreview.blockers.length > 0 && (
            <div className="deletion-blockers">
              <strong>Deletion blocked</strong>
              {deletionPreview.blockers.map((blocker) => (
                <span key={blocker}>{blocker}</span>
              ))}
              <p>Resolve each blocker, then create a fresh preview.</p>
            </div>
          )}
          <div className="deletion-preview-actions">
            <button
              type="button"
              className="danger-button"
              disabled={isSaving || deletionPreview.blockers.length > 0}
              onClick={() => void handleDeleteItem()}
            >
              Confirm logical deletion
            </button>
            <button
              type="button"
              className="text-button"
              disabled={isSaving}
              onClick={() => setDeletionPreview(undefined)}
            >
              Cancel
            </button>
          </div>
        </div>
      )}
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
              workAdapter.setItemNotes(view.item.id, notes),
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
              workAdapter.addReminder(view.item.id, reminderAt),
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
                    workAdapter.removeReminder(view.item.id, reminder.id),
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
          {view.runs.map((run) => {
            const runWorkset = findWorkset(view, run.workset_id);
            return (
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
                    {run.pane_status === "available" && " · Pane available"}
                  </span>
                </div>
                <code>{run.working_directory}</code>
                <span>
                  Session {run.session_name} · Pane {run.pane_id}
                </span>
                {run.pane_status === "missing" && (
                  <span className="run-pane-missing">
                    Pane missing. Decide whether to start another Run.
                  </span>
                )}
                {run.pane_status === "unknown" && (
                  <span className="run-pane-unknown">Pane status not confirmed.</span>
                )}
                {runWorkset && (
                  <div className="run-history-actions">
                    <button
                      type="button"
                      className="secondary-button"
                      disabled={isSaving || run.pane_status === "missing"}
                      onClick={() =>
                        onOpenTerminal(run.workset_id, paneTabForRun(run))
                      }
                    >
                      Open embedded terminal
                    </button>
                    <button
                      type="button"
                      className="secondary-button"
                      disabled={isSaving || run.pane_status === "missing"}
                        onClick={() =>
                          void saveItem(() =>
                            workAdapter.openExternalTerminal(run.id),
                        )
                      }
                    >
                      Open in Terminal
                    </button>
                    {run.state !== "finished" && run.pane_status !== "missing" && (
                      <button
                        type="button"
                        className="secondary-button"
                        disabled={isSaving}
                        onClick={() => void handleStopRun(run)}
                      >
                        Stop Run
                      </button>
                    )}
                    {run.state === "finished" && (
                      <button
                        type="button"
                        className="danger-button"
                        disabled={isSaving}
                        onClick={() => void handleDeleteRun(run)}
                      >
                        Delete finished Run
                      </button>
                    )}
                    {run.pane_status === "missing" && !runWorkset.archived && (
                      <button
                        type="button"
                        className="secondary-button"
                        disabled={isSaving}
                        onClick={() => void openRunPreview(runWorkset)}
                      >
                        Start a new Run
                      </button>
                    )}
                  </div>
                )}
              </article>
            );
          })}
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
            onUnlink={() => handleUnlinkExternalLink(externalLink.link.id)}
            onPrepareDeleteObject={() =>
              handlePrepareExternalObjectDeletion(externalLink.object.id)
            }
            onSavePolicy={(policy) =>
              saveItem(() =>
                workAdapter.setLinkAttentionPolicy(externalLink.link.id, policy),
              )
            }
            onMarkReviewed={() =>
              saveItem(() =>
                workAdapter.markLinkReviewed(externalLink.link.id),
              )
            }
            onSaveWatchUntil={(watchUntil) =>
              saveItem(() =>
                workAdapter.setLinkWatchUntil(externalLink.link.id, watchUntil),
              )
            }
            onSaveReviewAt={(reviewAt) =>
              saveItem(() =>
                workAdapter.setLinkReviewAt(externalLink.link.id, reviewAt),
              )
            }
            onClearReviewAt={() =>
              saveItem(() =>
                workAdapter.clearLinkReviewAt(externalLink.link.id),
              )
            }
            onAddComment={(body) =>
              saveItem(() =>
                workAdapter.addExternalComment(externalLink.link.id, body),
              )
            }
          />
        ))}
        {externalObjectDeletionPreview && (
          <div className="deletion-preview external-object-deletion-preview" role="alert">
            <strong>Remove External Object locally</strong>
            <p>
              This removes the local External Object record and every Link to it. It does not
              call GitHub or any other provider, so provider-owned Issues, pull requests, and
              other objects are never deleted.
            </p>
            <ul>
              <li>{externalObjectDeletionPreview.plan.linkIds.length} Link(s)</li>
              <li>{externalObjectDeletionPreview.plan.snapshotCount} snapshot(s)</li>
              <li>{externalObjectDeletionPreview.plan.activityCount} Activity record(s)</li>
            </ul>
            <div className="external-object-deletion-links">
              <strong>Items affected</strong>
              {externalObjectDeletionPreview.links.map((link) => (
                <span key={link.linkId}>
                  {link.itemIdentifier} · {link.itemTitle}
                </span>
              ))}
            </div>
            <p className="provider-warning">{externalObjectDeletionPreview.providerWarning}</p>
            <div className="deletion-preview-actions">
              <button
                type="button"
                disabled={isSaving}
                onClick={() => void handleDeleteExternalObject()}
              >
                Confirm local removal
              </button>
              <button
                type="button"
                className="text-button"
                disabled={isSaving}
                onClick={() => setExternalObjectDeletionPreview(undefined)}
              >
                Keep local records
              </button>
            </div>
          </div>
        )}
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
  onUnlink: () => Promise<void>;
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
        <div className="external-link-actions">
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
          <button
            type="button"
            className="text-button"
            disabled={isSaving}
            onClick={() => void onUnlink()}
          >
            Unlink this Item
          </button>
          <button
            type="button"
            className="text-button danger-text-button"
            disabled={isSaving}
            onClick={() => void onPrepareDeleteObject()}
          >
            Remove local object…
          </button>
        </div>
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

  async function markReviewed() {
    setIsSaving(true);
    try {
      await workAdapter.markLinkReviewed(entry.link_id);
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
      await workAdapter.removeReminder(itemId, reminderId);
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
      await workAdapter.clearLinkReviewAt(entry.link_id);
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
    <article className="run-suggestion-card">
      <div>
        <strong>
          {suggestion.agent === "claude" ? "Claude Code" : "Codex"} in {suggestion.machineName}
        </strong>
        <span>
          {suggestion.itemIdentifier} · {suggestion.itemTitle} · {suggestion.contextName}
        </span>
      </div>
      <div>
        <strong>Likely Workset: {suggestion.worksetBranch}</strong>
        <span>{suggestion.worksetRootDirectory}</span>
        <code>
          Session {suggestion.sessionName} · Pane {suggestion.paneId} · {suggestion.currentPath}
        </code>
      </div>
      <button
        type="button"
        className="secondary-button"
        disabled={disabled}
        onClick={() => void onAttach(suggestion)}
      >
        Attach Run
      </button>
    </article>
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
