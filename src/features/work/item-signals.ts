import { currentMinute } from "../../runtime/time";
import type { ItemView, RunDisplayPhase } from "../../runtime/types";

export const itemDetailTabs = [
  "overview",
  "spec",
  "runs",
  "repositories",
  "links",
] as const;
export type ItemDetailTab = (typeof itemDetailTabs)[number];

/** Forms an Item action opens inside the Item detail panel. */
export type ItemForm = "run" | "rename" | "reminder" | "link" | "issue";

/**
 * A request to open a form, carried to the panel once. The nonce lets the same
 * form be requested again after it was closed.
 */
export type ItemIntent = { itemId: number; form: ItemForm; nonce: number };

export function tabForForm(form: ItemForm): ItemDetailTab | undefined {
  if (form === "run") return "runs";
  if (form === "reminder") return "overview";
  if (form === "link" || form === "issue") return "links";
  return undefined;
}

export function displayItemIdentifier(identifier: string): string {
  const match = /^MC-(\d+)$/.exec(identifier);
  return match ? `#${match[1]}` : identifier;
}

/** Identifies one round of questions, so a new round counts as new. */
export function grillQuestionKey(run: ItemView["runs"][number]): string {
  return `${run.id}:${run.grill_question_group?.round ?? 0}:${JSON.stringify(run.grill_question_group)}`;
}

/**
 * What an Item needs from the user, shown on the collapsed card and the Runs
 * tab. A Grill waiting for answers is not also counted as an active Run.
 */
export function itemSignals(view: ItemView) {
  const now = currentMinute();
  const reminderDue = view.item.reminders.some(
    (reminder) => reminder.remind_at <= now,
  );
  return { ...view.run_signals, reminderDue };
}

export function defaultItemTab(view: ItemView): ItemDetailTab {
  return view.run_signals.grillWaiting ? "runs" : "overview";
}

export function runPhaseLabel(phase: RunDisplayPhase): string {
  switch (phase) {
    case "awaitingGo":
      return "Awaiting Go";
    case "blocked":
      return "Needs input";
    case "grillStarting":
      return "Starting Grill";
    case "grillWorking":
      return "Grill working";
    case "working":
      return "Working";
    case "grillWaitingForAnswers":
      return "Waiting for answers";
    case "grillAwaitingNextAction":
      return "Awaiting next action";
    case "grillRecoverablePaneLoss":
      return "Pane unavailable · recoverable";
    case "finished":
      return "Finished";
    case "unknown":
      return "Unknown";
  }
}

/**
 * The Item's Specs: supported Spec Objects explicitly marked on their Link,
 * newest first.
 */
export function itemSpecs(view: ItemView): ItemView["links"] {
  return view.links
    .filter(
      (link) =>
        link.link.purpose === "to-spec" &&
        link.supports_implementation_spec,
    )
    .sort((left, right) => right.link.id - left.link.id);
}
