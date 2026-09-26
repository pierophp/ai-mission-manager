import { currentMinute } from "../../runtime/time";
import type { GrillContinuationAction } from "../../runtime/execution-types";
import type { ExternalLinkView, ItemView, Run } from "../../runtime/types";

export const itemDetailTabs = [
  "overview",
  "spec",
  "runs",
  "repositories",
  "links",
] as const;
export type ItemDetailTab = (typeof itemDetailTabs)[number];

/** Forms an Item action opens inside the Item detail panel. */
export type ItemForm =
  | "grill"
  | "direct-run"
  | "rename"
  | "reminder"
  | "link"
  | "issue";

/**
 * A request to open a form, carried to the panel once. The nonce lets the same
 * form be requested again after it was closed.
 */
export type ItemIntent = { itemId: number; form: ItemForm; nonce: number };

export function tabForForm(form: ItemForm): ItemDetailTab | undefined {
  if (form === "grill" || form === "direct-run") return "runs";
  if (form === "reminder") return "overview";
  if (form === "link" || form === "issue") return "links";
  return undefined;
}

export function displayItemIdentifier(identifier: string): string {
  const match = /^MC-(\d+)$/.exec(identifier);
  return match ? `#${match[1]}` : identifier;
}

export function isRunFinished(run: Run): boolean {
  return (
    run.state === "finished" &&
    (run.execution_profile !== "grill" || run.grill_phase === "finished")
  );
}

export function isGrillWaitingForAnswers(run: Run): boolean {
  return (
    run.execution_profile === "grill" &&
    run.grill_phase === "waitingForAnswers" &&
    !run.grill_response
  );
}

/** Identifies one round of questions, so a new round counts as new. */
export function grillQuestionKey(run: Run): string {
  return `${run.id}:${run.grill_question_group?.round ?? 0}:${JSON.stringify(run.grill_question_group)}`;
}

export function activeRuns(view: ItemView): Run[] {
  return view.runs.filter(
    (run) => !isRunFinished(run) && run.pane_status !== "missing",
  );
}

/**
 * What an Item needs from the user, shown on the collapsed card and the Runs
 * tab. A Grill waiting for answers is not also counted as an active Run.
 */
export function itemSignals(view: ItemView) {
  const grillWaiting = view.runs.some(isGrillWaitingForAnswers);
  const runActive = activeRuns(view).some(
    (run) => !isGrillWaitingForAnswers(run),
  );
  const now = currentMinute();
  const reminderDue = view.item.reminders.some(
    (reminder) => reminder.remind_at <= now,
  );
  return { grillWaiting, runActive, reminderDue };
}

export function defaultItemTab(view: ItemView): ItemDetailTab {
  return view.runs.some(isGrillWaitingForAnswers) ? "runs" : "overview";
}

export function runStateLabel(state: Run["state"], isGrill = false): string {
  if (state === "working") return "Working";
  if (state === "blocked") return isGrill ? "Waiting for answers" : "Blocked";
  if (state === "finished") return "Finished";
  return "Unknown";
}

/**
 * The step that follows `previous` in a Grill Run, mirroring the backend:
 * the Grill itself, then to-spec, to-tickets, and implement.
 */
export function nextGrillAction(
  previous: GrillContinuationAction | null | undefined,
): GrillContinuationAction | undefined {
  if (!previous) return "to-spec";
  if (previous === "to-spec") return "to-tickets";
  if (previous === "to-tickets") return "implement";
  return undefined;
}

/**
 * The Item's specs: GitHub Issues a Grill's to-spec step created, newest
 * first. A spec linked by hand has no provenance and is not recognised.
 */
export function itemSpecs(view: ItemView): ExternalLinkView[] {
  return view.links
    .filter(
      (link) =>
        link.link.provenance?.action === "to-spec" &&
        link.object.provider === "github" &&
        link.object.kind === "issue",
    )
    .sort((left, right) => right.link.id - left.link.id);
}
