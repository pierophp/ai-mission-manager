import {
  type Dispatch,
  type SetStateAction,
  useEffect,
  useRef,
  useState,
} from "react";
import { cn } from "cn";
import { Button } from "../../../components/ui/button";
import { Card, CardContent } from "../../../components/ui/card";
import { Spinner } from "../../../components/ui/spinner";
import type { GrillContinuationAction } from "../../../runtime/execution-types";
import type { PaneTab } from "../../../runtime/terminal-types";
import type {
  Context,
  GrillAgentCatalog,
  GrillAnswer,
  ItemView,
  Machine,
  Repository,
  Run,
} from "../../../runtime/types";
import { GrillQuestionFlow } from "../grill-questions";
import {
  type ItemForm,
  type ItemIntent,
  grillQuestionKey,
  isGrillWaitingForAnswers,
  isRunFinished,
  nextGrillAction,
  runStateLabel,
} from "../item-signals";
import type { ItemCommands } from "../use-item-commands";
import { workActions } from "../work-mutations";
import { grillPhaseLabel, paneTabForRun, repositoryName } from "../work-utils";
import { RunLaunchForm } from "./RunLaunchForm";
import { itemExecution, useFormIntent } from "./shared";

/** Unsent Grill answers, keyed by question round, kept across Item switches. */
export type GrillAnswerDrafts = Record<string, Record<number, string>>;

const runForms = ["run"] as const;

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
  focusedRunRequest,
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
  focusedRunRequest?: { runId: number; request: number };
}) {
  const { isSaving, saveItem, confirm } = commands;
  const { itemContext } = itemExecution(
    view,
    repositories,
    contexts,
    machines,
  );
  const [runForm, setRunForm] = useState<"run">();
  const grillSubmissionLocks = useRef(new Set<string>());

  useEffect(() => {
    if (!focusedRunRequest) return;
    const runCard = document.getElementById(
      `run-card-${focusedRunRequest.runId}`,
    );
    runCard?.scrollIntoView({ behavior: "smooth", block: "center" });
    runCard?.focus({ preventScroll: true });
  }, [focusedRunRequest]);

  const runsNewestFirst = [...view.runs].sort(
    (left, right) => right.started_at - left.started_at || right.id - left.id,
  );
  function openRunForm(form: ItemForm) {
    if (form === "run") openRun();
  }

  useFormIntent(intent, runForms, openRunForm);

  function closeRunForm() {
    setRunForm(undefined);
  }

  function openRun() {
    setRunForm("run");
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

  async function handleContinueGrill(
    run: Run,
    action: GrillContinuationAction,
  ) {
    await saveItem(workActions.continueGrill(run.id, action));
  }

  async function handleGoPlan(run: Run) {
    await saveItem(workActions.goPlan(run.id));
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

  const launchWorkspace = view.workspaces[0];
  if (runForm === "run" && launchWorkspace) {
    return (
      <RunLaunchForm
        view={view}
        itemContext={itemContext}
        modelCatalog={grillModelCatalog}
        commands={commands}
        target={{ kind: "checkout", workspace: launchWorkspace }}
        onClose={closeRunForm}
      />
    );
  }

  return (
    <div className="grid gap-4">
      <div className="flex flex-wrap gap-2">
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={isSaving || !view.workspaces[0]}
          onClick={() => {
            openRun();
          }}
        >
          Start Run
        </Button>
      </div>
      {view.runs.length === 0 && (
        <span className="text-sm text-muted-foreground">No Runs yet.</span>
      )}
      {runsNewestFirst.map((run) => {
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
        const questionKey = grillQuestionKey(run);
        const persistedAnswers = Object.fromEntries(
          run.grill_answers.map((answer) => [
            answer.questionNumber,
            answer.answer,
          ]),
        );
        const answers = grillDrafts[questionKey] ?? persistedAnswers;
        const skipAction = nextGrillAction(run.grill_action);
        // A step that already ran is not offered again as the next action.
        const nextActions = (
          ["to-spec", "to-tickets", "implement"] as const
        ).filter(
          (action) => !(action === "to-spec" && run.grill_action === "to-spec"),
        );
        return (
          <Card
            size="sm"
            id={`run-card-${run.id}`}
            tabIndex={-1}
            className={cn(
              "bg-muted/20",
              focusedRunRequest?.runId === run.id && "ring-2 ring-primary",
            )}
            key={run.id}
          >
            <CardContent className="grid gap-2 pt-4">
              <div>
                <strong className="block text-sm">
                  Run #{run.id} ·{" "}
                  {run.agent === "claude" ? "Claude Code" : "Codex"}
                  {run.cli_configuration_profile &&
                    ` · ${run.cli_configuration_profile.name}`}
                </strong>
                <span className="text-xs text-muted-foreground">
                  {run.execution_profile} ·{" "}
                  {run.model && run.effort
                    ? `${run.model} · ${run.effort} · `
                    : ""}
                  {machines.find((machine) => machine.id === run.machine_id)
                    ?.name ?? "Machine #" + run.machine_id}{" "}
                  ·{" "}
                  {run.workflow === "pstack"
                    ? run.plan_phase === "awaitingGo"
                      ? "Awaiting Go"
                      : run.state === "blocked"
                      ? "Needs input"
                      : run.state === "finished"
                        ? "Finished"
                        : "Working"
                    : runStateLabel(run.state, run.execution_profile === "grill")}
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
              {run.reported_pull_requests.length > 0 && (
                <div className="grid gap-1 text-xs">
                  <strong>Pull Requests reported by this Run</strong>
                  {run.reported_pull_requests.map((url) => (
                    <a
                      className="break-all text-primary underline"
                      href={url}
                      key={url}
                      rel="noreferrer"
                      target="_blank"
                    >
                      {url}
                    </a>
                  ))}
                </div>
              )}
              {run.attention_summary && (
                <div className="grid gap-1 rounded-md border border-amber-500/40 bg-amber-500/5 p-2 text-sm">
                  <strong>Attention</strong>
                  <p className="whitespace-pre-wrap">{run.attention_summary}</p>
                </div>
              )}
              {run.execution_profile === "plan" && run.plan_phase === "awaitingGo" && (
                <section className="flex flex-wrap items-center justify-between gap-2 rounded-md border border-primary/30 bg-primary/5 p-3" aria-label={`Plan ready for Run #${run.id}`}>
                  <div className="grid gap-1 text-sm">
                    <strong>Plan ready</strong>
                    {run.plan_path ? (
                      <a className="break-all text-primary underline" href={run.plan_path} target="_blank" rel="noreferrer">{run.plan_path}</a>
                    ) : <span className="text-muted-foreground">Plan path not reported</span>}
                  </div>
                  <Button type="button" size="sm" disabled={isSaving || run.pane_status !== "available"} onClick={() => void handleGoPlan(run)}>Go</Button>
                </section>
              )}
              {run.execution_profile === "grill" &&
                (run.grill_phase === "starting" ||
                  run.grill_phase === "working") && (
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
                        Answer the complete group once. Mission Manager will
                        send one numbered response to the same Pane.
                      </span>
                    </div>
                    {skipAction && (
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        title={
                          skipAction === "to-spec"
                            ? "Stop grilling and write the spec now; the open questions go into the spec."
                            : `Leave these questions to their recommendations and continue with ${skipAction}.`
                        }
                        disabled={isSaving || run.pane_status !== "available"}
                        onClick={() =>
                          void handleContinueGrill(run, skipAction)
                        }
                      >
                        Skip to {skipAction}
                      </Button>
                    )}
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
                      {nextActions.map((action) => (
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
                      void saveItem(
                        workActions.openExternalTerminal(run.id),
                        false,
                      )
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
