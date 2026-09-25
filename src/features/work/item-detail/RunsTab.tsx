import {
  type Dispatch,
  type FormEvent,
  type SetStateAction,
  useRef,
  useState,
} from "react";
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "../../../components/ui/alert";
import { Badge } from "../../../components/ui/badge";
import { Button } from "../../../components/ui/button";
import { Card, CardContent } from "../../../components/ui/card";
import { Checkbox } from "../../../components/ui/checkbox";
import {
  NativeSelect,
  NativeSelectOption,
} from "../../../components/ui/native-select";
import { Spinner } from "../../../components/ui/spinner";
import { Textarea } from "../../../components/ui/textarea";
import { errorMessage } from "../../../runtime/errors";
import type { GrillContinuationAction } from "../../../runtime/execution-types";
import type { PaneTab } from "../../../runtime/terminal-types";
import type {
  AgentKind,
  Context,
  DirectRunPreview,
  ExecutionProfile,
  GrillAgentCatalog,
  GrillAnswer,
  GrillConfiguration,
  ItemView,
  Machine,
  Repository,
  Run,
  Workspace,
} from "../../../runtime/types";
import { GrillQuestionFlow } from "../grill-questions";
import {
  type ItemForm,
  type ItemIntent,
  grillQuestionKey,
  isGrillWaitingForAnswers,
  isRunFinished,
  runStateLabel,
} from "../item-signals";
import type { ItemCommands } from "../use-item-commands";
import { workActions } from "../work-mutations";
import { grillPhaseLabel, paneTabForRun, repositoryName } from "../work-utils";
import { itemExecution, useFormIntent } from "./shared";

/** Unsent Grill answers, keyed by question round, kept across Item switches. */
export type GrillAnswerDrafts = Record<string, Record<number, string>>;

const runForms = ["grill", "direct-run"] as const;

export function RunsTab({
  view,
  repositories,
  machines,
  contexts,
  grillModelCatalog,
  commands,
  intent,
  grillDrafts,
  onGrillDraftsChange,
  onOpenTerminal,
}: {
  view: ItemView;
  repositories: Repository[];
  machines: Machine[];
  contexts: Context[];
  grillModelCatalog: GrillAgentCatalog[];
  commands: ItemCommands;
  intent: ItemIntent | undefined;
  grillDrafts: GrillAnswerDrafts;
  onGrillDraftsChange: Dispatch<SetStateAction<GrillAnswerDrafts>>;
  onOpenTerminal: (runId: number, pane: PaneTab) => void;
}) {
  const { isSaving, saveItem, whileSaving, confirm, workCommand } = commands;
  const { itemRepositories, itemContext, executionMachine } = itemExecution(
    view,
    repositories,
    contexts,
    machines,
  );
  const [runForm, setRunForm] = useState<"grill" | "direct">();
  const grillSubmissionLocks = useRef(new Set<string>());

  // Direct Run form
  const [runAgent, setRunAgent] = useState<AgentKind>("claude");
  const [runProfile, setRunProfile] = useState<ExecutionProfile>("implement");
  const [includeRunNotes, setIncludeRunNotes] = useState(false);
  const [runCustomPrompt, setRunCustomPrompt] = useState("");
  const [runPrompt, setRunPrompt] = useState("");
  const [runPromptNeedsCompose, setRunPromptNeedsCompose] = useState(false);
  const [directRunWorkspaceId, setDirectRunWorkspaceId] = useState<number>();
  const [directRunRepositoryId, setDirectRunRepositoryId] = useState<number>();
  const [directRunPreview, setDirectRunPreview] = useState<DirectRunPreview>();
  const [directRunDirtyConfirmed, setDirectRunDirtyConfirmed] = useState(false);
  const [directRunSharedConfirmed, setDirectRunSharedConfirmed] =
    useState(false);

  // Grill Run form
  const [grillRunWorkspaceId, setGrillRunWorkspaceId] = useState<number>();
  const [grillRunRepositoryId, setGrillRunRepositoryId] = useState<number>();
  const [grillRunPreview, setGrillRunPreview] = useState<DirectRunPreview>();
  const [grillRunPreviewError, setGrillRunPreviewError] = useState<string>();
  const [grillAgent, setGrillAgent] =
    useState<GrillConfiguration["agent"]>("claude");
  const [grillModel, setGrillModel] = useState("claude-sonnet-4-5");
  const [grillEffort, setGrillEffort] = useState("high");
  const [grillInitialPrompt, setGrillInitialPrompt] = useState("");
  const [grillPromptPreview, setGrillPromptPreview] = useState("");
  const [grillDirtyConfirmed, setGrillDirtyConfirmed] = useState(false);
  const [grillSharedConfirmed, setGrillSharedConfirmed] = useState(false);

  const activeGrillRun = view.runs.find(
    (run) => run.execution_profile === "grill" && !isRunFinished(run),
  );
  const selectedGrillCatalog = grillModelCatalog.find(
    (catalog) => catalog.agent === grillAgent,
  );
  const selectedGrillModel = selectedGrillCatalog?.models.find(
    (model) => model.id === grillModel,
  );

  function openRunForm(form: ItemForm) {
    if (form === "grill" && !activeGrillRun) openGrillStart();
    if (form === "direct-run" && view.workspaces[0]) {
      void openDirectRunPreview(view.workspaces[0]);
    }
  }

  useFormIntent(intent, runForms, openRunForm);

  function closeRunForm() {
    setRunForm(undefined);
    setDirectRunWorkspaceId(undefined);
    setDirectRunPreview(undefined);
    setRunPrompt("");
    setGrillRunWorkspaceId(undefined);
    setGrillRunPreview(undefined);
    setGrillRunPreviewError(undefined);
    setGrillPromptPreview("");
  }

  function openGrillStart() {
    const defaults = itemContext?.grill_defaults;
    const executionWorkspaceId = view.workspaces[0]?.id;
    setRunForm("grill");
    setGrillAgent(defaults?.agent ?? "claude");
    setGrillModel(defaults?.model ?? "claude-sonnet-4-5");
    setGrillEffort(defaults?.effort ?? "high");
    setGrillInitialPrompt("");
    setGrillPromptPreview("");
    void refreshGrillRunPreview(executionWorkspaceId);
  }

  async function refreshGrillRunPreview(workspaceId: number | undefined) {
    setGrillRunWorkspaceId(workspaceId);
    setGrillRunRepositoryId(undefined);
    setGrillRunPreview(undefined);
    setGrillRunPreviewError(undefined);
    setGrillDirtyConfirmed(false);
    setGrillSharedConfirmed(false);
    if (!workspaceId) return;

    await whileSaving(async () => {
      try {
        const preview = await workCommand.execute(
          workActions.prepareGrillRun(view.item.id, workspaceId, null),
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
      }
    });
  }

  async function composeGrillPromptPreview() {
    if (!grillInitialPrompt.trim() || !selectedGrillModel) return;
    await whileSaving(async () => {
      try {
        setGrillPromptPreview(
          await workCommand.execute(
            workActions.composeGrillPrompt(
              view.item.id,
              { agent: grillAgent, model: grillModel, effort: grillEffort },
              grillInitialPrompt,
            ),
            false,
          ),
        );
      } catch (composeError) {
        window.alert(errorMessage(composeError));
      }
    });
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
        machineId: null,
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
    // The new Run appears in the list below and shows its questions there.
    if (started) closeRunForm();
  }

  async function handleSubmitGrillAnswers(
    run: Run,
    submissionKey: string,
    answers: GrillAnswer[],
  ) {
    if (grillSubmissionLocks.current.has(submissionKey)) return;
    grillSubmissionLocks.current.add(submissionKey);
    const submitted = await saveItem(
      workActions.submitGrillAnswers(run.id, answers),
    );
    if (!submitted) {
      grillSubmissionLocks.current.delete(submissionKey);
      return;
    }
    onGrillDraftsChange((current) => {
      const next = { ...current };
      delete next[submissionKey];
      return next;
    });
  }

  async function handleContinueGrill(run: Run, action: GrillContinuationAction) {
    await saveItem(workActions.continueGrill(run.id, action));
  }

  function runPromptSelection() {
    return {
      includeObjective: true,
      includeNotes: includeRunNotes,
      externalObjectIds: [],
    };
  }

  async function composeRunPromptPreview() {
    if (!directRunWorkspaceId) return;
    await whileSaving(async () => {
      try {
        setRunPrompt(
          await workCommand.execute(
            workActions.composeRunPrompt(
              view.item.id,
              runProfile,
              runPromptSelection(),
              runProfile === "custom" ? runCustomPrompt : null,
            ),
            false,
          ),
        );
        setRunPromptNeedsCompose(false);
      } catch (composeError) {
        window.alert(errorMessage(composeError));
      }
    });
  }

  async function openDirectRunPreview(workspace: Workspace) {
    const includeNotes = Boolean(view.item.notes.trim());
    setRunForm("direct");
    setDirectRunWorkspaceId(workspace.id);
    setDirectRunRepositoryId(undefined);
    setDirectRunPreview(undefined);
    setDirectRunDirtyConfirmed(false);
    setDirectRunSharedConfirmed(false);
    setRunAgent("claude");
    setRunProfile("implement");
    setIncludeRunNotes(includeNotes);
    setRunCustomPrompt("");
    await whileSaving(async () => {
      try {
        const [prompt, preview] = await Promise.all([
          workCommand.execute(
            workActions.composeRunPrompt(
              view.item.id,
              "implement",
              { includeObjective: true, includeNotes, externalObjectIds: [] },
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
        closeRunForm();
        window.alert(errorMessage(previewError));
      }
    });
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
      directRunPreview.dirtyRepositoryIds.length === 0 || directRunDirtyConfirmed;
    const sharedConfirmed =
      directRunPreview.sharedPaths.length === 0 || directRunSharedConfirmed;
    if (!dirtyConfirmed || !sharedConfirmed) return;

    const started = await saveItem(
      workActions.startDirectRun({
        itemId: view.item.id,
        workspaceId: directRunWorkspaceId,
        primaryRepositoryId: directRunRepositoryId,
        machineId: null,
        agent: runAgent,
        executionProfile: runProfile,
        prompt: runPrompt,
        promptSelection: runPromptSelection(),
        expectedCheckouts: directRunPreview.checkouts,
        allowDirty: directRunPreview.dirtyRepositoryIds.length > 0,
        allowSharedCheckouts: directRunPreview.sharedPaths.length > 0,
      }),
    );
    if (started) closeRunForm();
  }

  function handleStopRun(run: Run) {
    confirm({
      title: `Stop Run #${run.id}?`,
      description:
        "This is separate from completing the Item and will leave the Run in its history.",
      confirmLabel: "Stop Run",
      onConfirm: () => void saveItem(workActions.stopRun(run.id)),
    });
  }

  function handleFinishRun(run: Run) {
    confirm({
      title: `Finish Run #${run.id}?`,
      description:
        "This records an explicit Run completion, keeps its transcript and answers in history, and leaves the Item status unchanged.",
      confirmLabel: "Finish Run",
      onConfirm: () => void saveItem(workActions.finishRun(run.id)),
    });
  }

  function handleDeleteRun(run: Run) {
    if (!isRunFinished(run)) return;
    confirm({
      title: `Delete finished Run #${run.id}?`,
      description: "This removes the Run from history and cannot be undone.",
      confirmLabel: "Delete Run",
      onConfirm: () => void saveItem(workActions.deleteRun(run.id)),
    });
  }

  if (runForm === "grill") {
    return (
      <form
        className="grid gap-4 rounded-lg border border-primary/30 bg-primary/5 p-4"
        onSubmit={(event) => void handleStartGrillRun(event)}
      >
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <h4 className="m-0 text-base font-medium">Start Grill Run</h4>
            <p className="mt-1 text-sm text-muted-foreground">
              The Run uses this Item’s Context defaults unless you override them here.
            </p>
          </div>
          {grillRunPreview && (
            <Badge variant="outline">Machine: {grillRunPreview.machineName}</Badge>
          )}
        </div>
        {itemRepositories.length === 0 && (
          <Alert variant="destructive">
            <AlertTitle>Project Repository required</AlertTitle>
            <AlertDescription>
              Register a Repository in this Project&apos;s settings before
              starting a Grill Run.
            </AlertDescription>
          </Alert>
        )}
        <div className="grid gap-3 md:grid-cols-2">
          <div className="grid gap-1.5 text-sm">
            <span className="font-medium">Project Repositories</span>
            <span className="text-muted-foreground">
              All configured Repositories are included.
            </span>
            <div className="flex flex-wrap gap-1.5">
              {itemRepositories.map((repository) => (
                <Badge variant="secondary" key={repository.id}>
                  {repository.name}
                </Badge>
              ))}
            </div>
          </div>
          <div className="grid gap-1.5 text-sm">
            <span className="font-medium">Context execution Machine</span>
            <span className="text-muted-foreground">
              {executionMachine
                ? `${executionMachine.name} · ${executionMachine.last_observed}`
                : "Not configured"}
            </span>
          </div>
        </div>
        {grillRunPreviewError && (
          <Alert variant="destructive">
            <AlertTitle>Could not prepare the Grill checkout</AlertTitle>
            <AlertDescription>
              <p>{grillRunPreviewError}</p>
              <p>
                Register this Repository&apos;s checkout for the Context
                execution Machine under Settings, then reopen the Grill Run.
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
                <NativeSelectOption value="">
                  Choose the primary checkout
                </NativeSelectOption>
                {grillRunPreview.checkoutDetails.map((checkout) => (
                  <NativeSelectOption
                    value={checkout.repositoryId}
                    key={checkout.repositoryId}
                  >
                    {checkout.repositoryName} · {checkout.path}
                  </NativeSelectOption>
                ))}
              </NativeSelect>
            </label>
            <CheckoutList preview={grillRunPreview} />
            {grillRunPreview.dirtyRepositoryIds.length > 0 && (
              <Alert>
                <AlertTitle>Checkout has local changes</AlertTitle>
                <AlertDescription>
                  Review the checkout before allowing the Grill to use it.
                  <label className="mt-2 flex items-center gap-2 font-normal">
                    <Checkbox
                      checked={grillDirtyConfirmed}
                      onCheckedChange={(checked) =>
                        setGrillDirtyConfirmed(checked === true)
                      }
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
                    <div key={`${shared.runId}-${shared.path}`}>
                      Run #{shared.runId} · {shared.path}
                    </div>
                  ))}
                  <label className="mt-2 flex items-center gap-2 font-normal">
                    <Checkbox
                      checked={grillSharedConfirmed}
                      onCheckedChange={(checked) =>
                        setGrillSharedConfirmed(checked === true)
                      }
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
                const nextCatalog = grillModelCatalog.find(
                  (catalog) => catalog.agent === nextAgent,
                );
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
                const nextModel = selectedGrillCatalog?.models.find(
                  (model) => model.id === event.target.value,
                );
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
            autoFocus
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
        <div className="flex flex-wrap gap-2">
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
              ((grillRunPreview?.dirtyRepositoryIds.length ?? 0) > 0 &&
                !grillDirtyConfirmed) ||
              ((grillRunPreview?.sharedPaths.length ?? 0) > 0 &&
                !grillSharedConfirmed)
            }
          >
            {isSaving ? "Starting…" : "Start Grill Run"}
          </Button>
          <Button
            type="button"
            size="sm"
            variant="ghost"
            disabled={isSaving}
            onClick={closeRunForm}
          >
            Cancel
          </Button>
        </div>
      </form>
    );
  }

  if (runForm === "direct") {
    return (
      <div className="grid gap-4 rounded-lg border border-primary/30 bg-primary/5 p-4">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <h4 className="m-0 text-base font-medium">Start Direct Run</h4>
            <p className="mt-1 text-sm text-muted-foreground">
              This Run will use{" "}
              {executionMachine?.name ?? "the Context execution Machine"}{" "}
              configured for {view.context_name}.
            </p>
          </div>
          {directRunPreview && (
            <Badge variant="outline">Machine: {directRunPreview.machineName}</Badge>
          )}
        </div>
        {directRunPreview ? (
          <form
            className="grid gap-4"
            onSubmit={(event) => void handleStartDirectRun(event)}
          >
            <CheckoutList preview={directRunPreview} />
            <label className="grid gap-1.5 text-sm font-medium">
              <span>Primary Repository / working directory</span>
              <NativeSelect
                autoFocus
                value={directRunRepositoryId ?? ""}
                onChange={(event) =>
                  setDirectRunRepositoryId(Number(event.target.value) || undefined)
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
                  {directRunPreview.currentBranches.join(", ")}. The Run will use
                  each checkout&apos;s current branch without switching it.
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
                  onChange={(event) => setRunAgent(event.target.value as AgentKind)}
                  disabled={isSaving}
                >
                  <NativeSelectOption value="claude">Claude Code</NativeSelectOption>
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
                  <NativeSelectOption value="investigate">Investigate</NativeSelectOption>
                  <NativeSelectOption value="implement">Implement</NativeSelectOption>
                  <NativeSelectOption value="review">Review</NativeSelectOption>
                  <NativeSelectOption value="custom">Custom prompt</NativeSelectOption>
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
                onClick={closeRunForm}
              >
                Cancel
              </Button>
            </div>
          </form>
        ) : (
          <p className="flex items-center gap-2 text-sm text-muted-foreground">
            <Spinner /> Preparing checkout preview…
          </p>
        )}
      </div>
    );
  }

  return (
    <div className="grid gap-4">
      <div className="flex flex-wrap gap-2">
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={isSaving || Boolean(activeGrillRun)}
          onClick={openGrillStart}
        >
          {activeGrillRun ? "Grill Run already active" : "Start Grill Run"}
        </Button>
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={isSaving || !view.workspaces[0]}
          onClick={() => {
            const workspace = view.workspaces[0];
            if (workspace) void openDirectRunPreview(workspace);
          }}
        >
          Start Direct Run
        </Button>
      </div>
      {view.runs.length === 0 && (
        <span className="text-sm text-muted-foreground">No Runs yet.</span>
      )}
      {view.runs.map((run) => {
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
            : view.worktrees.find((worktree) => worktree.id === run.worktree_id);
        const questionKey = grillQuestionKey(run);
        const persistedAnswers = Object.fromEntries(
          run.grill_answers.map((answer) => [answer.questionNumber, answer.answer]),
        );
        const answers = grillDrafts[questionKey] ?? persistedAnswers;
        return (
          <Card size="sm" className="bg-muted/20" key={run.id}>
            <CardContent className="grid gap-2 pt-4">
              <div>
                <strong className="block text-sm">
                  Run #{run.id} · {run.agent === "claude" ? "Claude Code" : "Codex"}
                </strong>
                <span className="text-xs text-muted-foreground">
                  {run.execution_profile} ·{" "}
                  {run.execution_profile === "grill" && run.model && run.effort
                    ? `${run.model} · ${run.effort} · `
                    : ""}
                  {machines.find((machine) => machine.id === run.machine_id)
                    ?.name ?? "Machine #" + run.machine_id}{" "}
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
                {runRepository !== undefined
                  ? `Repository ${repositoryName(repositories, runRepository)}`
                  : ""}
                {runWorktree
                  ? " · Worktree"
                  : runRepository !== undefined
                    ? " · Direct checkout"
                    : "Unregistered working location"}
              </span>
              <span className="text-xs text-muted-foreground">
                Session {run.session_name} · Pane {run.pane_id}
              </span>
              {run.execution_profile === "grill" &&
                (run.grill_phase === "starting" || run.grill_phase === "working") && (
                  <div
                    className="flex items-center gap-2 text-sm text-muted-foreground"
                    aria-live="polite"
                  >
                    <Spinner />
                    {run.grill_phase === "starting"
                      ? "Waiting for the Grill’s first questions…"
                      : "The Grill is working…"}
                  </div>
                )}
              {isGrillWaitingForAnswers(run) && (
                <section
                  className="grid gap-3 rounded-lg border border-amber-500/30 bg-amber-500/5 p-3"
                  aria-label={`Grill questions for Run #${run.id}`}
                >
                  <div className="flex flex-wrap items-start justify-between gap-2">
                    <div>
                      <strong className="block text-sm">Grill questions</strong>
                      <span className="text-xs text-muted-foreground">
                        Answer the complete group once. Mission Manager will send
                        one numbered response to the same Pane.
                      </span>
                    </div>
                    <Button
                      type="button"
                      variant="ghost"
                      size="sm"
                      title="Stop grilling and write the spec now; the open questions go into the spec."
                      disabled={isSaving || run.pane_status !== "available"}
                      onClick={() => void handleContinueGrill(run, "to-spec")}
                    >
                      Skip to to-spec
                    </Button>
                  </div>
                  <GrillQuestionFlow
                    key={questionKey}
                    run={run}
                    answers={answers}
                    disabled={isSaving}
                    onAnswerChange={(questionNumber, answer) =>
                      onGrillDraftsChange((current) => ({
                        ...current,
                        [questionKey]: {
                          ...(current[questionKey] ?? persistedAnswers),
                          [questionNumber]: answer,
                        },
                      }))
                    }
                    onSubmit={(submitted) =>
                      handleSubmitGrillAnswers(run, questionKey, submitted)
                    }
                  />
                </section>
              )}
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
                  reconnect the exact Pane or use the terminal escape hatch when
                  the runtime returns.
                </span>
              )}
              {run.pane_status === "unknown" && (
                <span className="text-xs text-muted-foreground">
                  Pane status not confirmed.
                </span>
              )}
              {run.execution_profile === "grill" &&
                run.grill_phase === "awaitingNextAction" && (
                  <div className="grid gap-2 rounded-md border border-primary/30 bg-primary/5 p-3">
                    <div>
                      <strong className="block text-sm">
                        Grill completed · choose the next action
                      </strong>
                      <span className="text-xs text-muted-foreground">
                        Each action continues this Run in the same Pane and
                        working directory. Finish or stop remains explicit.
                      </span>
                    </div>
                    <div className="flex flex-wrap gap-2">
                      {(["to-spec", "to-tickets", "implement"] as const).map(
                        (action) => (
                          <Button
                            key={action}
                            type="button"
                            size="sm"
                            variant="outline"
                            disabled={isSaving || run.pane_status !== "available"}
                            onClick={() => void handleContinueGrill(run, action)}
                          >
                            {action}
                          </Button>
                        ),
                      )}
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
                    onClick={() => onOpenTerminal(run.id, paneTabForRun(run))}
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
                      void saveItem(workActions.openExternalTerminal(run.id), false)
                    }
                  >
                    Open in Terminal
                  </Button>
                  {!isRunFinished(run) && run.pane_status !== "missing" && (
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      disabled={isSaving}
                      onClick={() => handleStopRun(run)}
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
                      onClick={() => handleFinishRun(run)}
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
                      onClick={() => handleDeleteRun(run)}
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
  );
}

function CheckoutList({ preview }: { preview: DirectRunPreview }) {
  return (
    <div className="grid gap-2 rounded-md border p-3 text-sm">
      <p className="m-0 font-medium">Registered checkouts</p>
      {preview.checkoutDetails.map((checkout) => (
        <div className="flex flex-wrap justify-between gap-2" key={checkout.repositoryId}>
          <span>
            {checkout.repositoryName} · {checkout.branch}
          </span>
          <code className="break-all text-xs text-muted-foreground">
            {checkout.path}
          </code>
        </div>
      ))}
    </div>
  );
}
