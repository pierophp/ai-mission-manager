import { useState } from "react";
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "../../../components/ui/alert";
import { Badge } from "../../../components/ui/badge";
import { Button } from "../../../components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "../../../components/ui/card";
import { Checkbox } from "../../../components/ui/checkbox";
import { Input } from "../../../components/ui/input";
import type {
  Context,
  GrillAgentCatalog,
  ItemView,
  Machine,
  Repository,
  Workspace,
  WorktreeRemovalReport,
} from "../../../runtime/types";
import type { ItemCommands } from "../use-item-commands";
import { workActions } from "../work-commands";
import { repositoryName } from "../work-utils";
import { RunLaunchForm } from "./RunLaunchForm";
import { itemExecution } from "./shared";

export function RepositoriesTab({
  view,
  repositories,
  machines,
  contexts,
  grillModelCatalog,
  commands,
}: {
  view: ItemView;
  repositories: Repository[];
  machines: Machine[];
  contexts: Context[];
  grillModelCatalog: GrillAgentCatalog[];
  commands: ItemCommands;
}) {
  const { isSaving, saveItem } = commands;
  const { itemRepositories, executionMachineId, executionMachine } =
    itemExecution(view, repositories, contexts, machines);
  const [worktreePathDrafts, setWorktreePathDrafts] = useState<
    Record<string, string>
  >({});
  const [worktreeRun, setWorktreeRun] = useState<{
    workspace: Workspace;
    worktreeId: number;
  }>();
  const [worktreeRemovalReport, setWorktreeRemovalReport] =
    useState<WorktreeRemovalReport>();
  const [
    worktreeRemovalDestructiveConfirmed,
    setWorktreeRemovalDestructiveConfirmed,
  ] = useState(false);

  async function prepareWorkspaceWorktree(
    workspace: Workspace,
    repositoryId: number,
    reuseExistingBranch: boolean,
  ) {
    const machineId = executionMachineId;
    if (!machineId) {
      window.alert("Configure an execution Machine for this Context in Settings → Machines first.");
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
    const machineId = executionMachineId;
    if (!machineId) {
      window.alert("Configure an execution Machine for this Context in Settings → Machines first.");
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

  async function reviewWorktreeRemoval(worktreeId: number) {
    const report = await saveItem(
      workActions.prepareWorktreeRemoval(worktreeId),
      false,
    );
    if (!report) return;
    setWorktreeRemovalReport(report);
    setWorktreeRemovalDestructiveConfirmed(false);
  }

  async function removeWorktree() {
    if (!worktreeRemovalReport) return;
    await saveItem(
      workActions.removeWorktree(
        worktreeRemovalReport.worktreeId,
        worktreeRemovalReport.requiresDestructiveConfirmation &&
          worktreeRemovalDestructiveConfirmed,
      ),
    );
    setWorktreeRemovalReport(undefined);
    setWorktreeRemovalDestructiveConfirmed(false);
  }

  const workspace = view.workspaces[0];
  if (itemRepositories.length === 0 || !workspace) {
    return (
      <span className="text-sm text-muted-foreground">
        Register a Repository in this Project&apos;s settings to make it
        available to the Item.
      </span>
    );
  }

  const workspaceWorktrees = view.worktrees.filter(
    (worktree) => worktree.workspace_id === workspace.id,
  );
  const worktreeRunWorktree = view.worktrees.find(
    (worktree) => worktree.id === worktreeRun?.worktreeId,
  );
  const worktreeRunTarget =
    worktreeRun &&
    worktreeRunWorktree &&
    view.workspaces.some(
      (candidate) => candidate.id === worktreeRun.workspace.id,
    )
      ? {
          kind: "worktree" as const,
          workspace: worktreeRun.workspace,
          worktree: worktreeRunWorktree,
          repositoryName: repositoryName(
            repositories,
            worktreeRunWorktree.repository_id,
          ),
        }
      : undefined;

  return (
    <Card size="sm">
      <CardHeader className="border-b border-border/70">
        <CardTitle className="text-sm">Project Repositories</CardTitle>
        <CardDescription>
          Every Repository configured for this Project is available to this
          Item. Each Worktree stays isolated to this Item.
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
        <div className="grid gap-2 rounded-md border p-3">
          <p className="m-0 text-sm">
            <span className="font-medium">Context execution Machine: </span>
            {executionMachine
              ? `${executionMachine.name} · ${executionMachine.last_observed}`
              : "Not configured"}
          </p>
          {!executionMachine && (
            <Alert variant="destructive">
              <AlertTitle>Execution Machine required</AlertTitle>
              <AlertDescription>
                Choose a Machine in Settings → Machines before creating
                Worktrees or starting Runs.
              </AlertDescription>
            </Alert>
          )}
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
                    disabled={
                      isSaving || !executionMachineId || Boolean(existingWorktree)
                    }
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
                      disabled={
                        isSaving ||
                        !executionMachineId ||
                        !worktreePathDrafts[draftKey]?.trim()
                      }
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
                {worktree.machine_id !== executionMachineId && (
                  <Alert variant="destructive" className="basis-full">
                    <AlertTitle>Worktree is on a previous Machine</AlertTitle>
                    <AlertDescription>
                      It belongs to{" "}
                      {machines.find((machine) => machine.id === worktree.machine_id)
                        ?.name ?? `Machine ${worktree.machine_id}`}
                      ; this Context now uses{" "}
                      {executionMachine?.name ?? "no execution Machine"}. Remove
                      and recreate it explicitly to run here. Its files remain
                      on the previous Machine.
                    </AlertDescription>
                  </Alert>
                )}
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
                  disabled={isSaving || worktree.machine_id !== executionMachineId}
                  onClick={() =>
                    setWorktreeRun({
                      workspace:
                        view.workspaces.find(
                          (candidate) => candidate.id === worktree.workspace_id,
                        ) ?? workspace,
                      worktreeId: worktree.id,
                    })
                  }
                >
                  {worktree.machine_id === executionMachineId
                    ? "Start Worktree Run"
                    : "Worktree on previous Machine"}
                </Button>
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  disabled={isSaving}
                  onClick={() => void reviewWorktreeRemoval(worktree.id)}
                >
                  Remove Worktree
                </Button>
              </div>
            ))}
          </div>
        )}
        {worktreeRemovalReport &&
          view.workspaces.some(
            (candidate) => candidate.id === worktreeRemovalReport.workspaceId,
          ) && (
            <div className="grid gap-3 rounded-lg border border-destructive/30 p-4">
              <div>
                <h4 className="m-0 text-base font-medium">
                  Confirm Worktree removal
                </h4>
                <p className="mt-1 text-sm text-muted-foreground">
                  The Worktree directory will be removed. Its Git branch will be
                  preserved.
                </p>
              </div>
              <div className="grid gap-1 text-sm">
                <span>
                  {worktreeRemovalReport.repositoryName} ·{" "}
                  {worktreeRemovalReport.branch}
                </span>
                <code className="break-all text-xs text-muted-foreground">
                  {worktreeRemovalReport.path}
                </code>
              </div>
              {worktreeRemovalReport.requiresDestructiveConfirmation && (
                <label className="flex items-center gap-2 text-sm text-destructive">
                  <Checkbox
                    checked={worktreeRemovalDestructiveConfirmed}
                    onCheckedChange={(checked) =>
                      setWorktreeRemovalDestructiveConfirmed(checked === true)
                    }
                    disabled={isSaving}
                  />
                  Confirm destructive removal of dirty files.
                </label>
              )}
              <div className="flex flex-wrap gap-2">
                <Button
                  type="button"
                  variant="destructive"
                  disabled={
                    isSaving ||
                    (worktreeRemovalReport.requiresDestructiveConfirmation &&
                      !worktreeRemovalDestructiveConfirmed)
                  }
                  onClick={() => void removeWorktree()}
                >
                  {isSaving ? "Removing…" : "Remove Worktree"}
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  disabled={isSaving}
                  onClick={() => setWorktreeRemovalReport(undefined)}
                >
                  Cancel
                </Button>
              </div>
            </div>
          )}
        {worktreeRunTarget && (
          <RunLaunchForm
            key={worktreeRunTarget.worktree.id}
            view={view}
            modelCatalog={grillModelCatalog}
            commands={commands}
            target={worktreeRunTarget}
            onClose={() => setWorktreeRun(undefined)}
          />
        )}
      </CardContent>
    </Card>
  );
}
