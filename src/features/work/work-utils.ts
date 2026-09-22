import type {
  AttentionEntry,
  ExternalObject,
  HomeView,
  ItemRelation,
  ItemRelationKind,
  ItemView,
  Repository,
  Run,
} from "../../runtime/types";
import type { PaneTab } from "../../runtime/terminal-types";

export function attentionEntryLabel(entry: AttentionEntry): string {
  if (entry.kind === "reminder") return "Reminder due";
  if (entry.kind === "review") return "Review date reached";
  if (entry.kind === "blocked_run") return "Run blocked";
  return `${entry.activities.length} change${entry.activities.length === 1 ? "" : "s"}`;
}

export function runStateLabel(state: Run["state"]): string {
  if (state === "working") return "Working";
  if (state === "blocked") return "Blocked";
  if (state === "finished") return "Finished";
  return "Unknown";
}

export function externalObjectKindLabel(kind: ExternalObject["kind"]): string {
  if (kind === "pull_request") return "Pull request";
  if (kind === "issue") return "Issue";
  return "Link";
}

export function formatSnapshotAge(fetchedAt: number): string {
  const ageSeconds = Math.max(0, Math.floor(Date.now() / 1000) - fetchedAt);
  if (ageSeconds < 60) return "just now";
  if (ageSeconds < 3600) return `${Math.floor(ageSeconds / 60)}m ago`;
  if (ageSeconds < 86400) return `${Math.floor(ageSeconds / 3600)}h ago`;
  return `${Math.floor(ageSeconds / 86400)}d ago`;
}

export function relationshipLabel(
  relation: ItemRelation,
  currentItemId: number,
): string {
  if (relation.from_item_id === currentItemId) {
    return relationKindLabel(relation.kind);
  }
  if (relation.kind === "Blocks") return "blocked by";
  if (relation.kind === "BlockedBy") return "blocks";
  return "related to";
}

export function relationKindLabel(kind: ItemRelationKind): string {
  if (kind === "BlockedBy") return "blocked by";
  if (kind === "RelatedTo") return "related to";
  return "blocks";
}

export function repositoryName(
  repositories: Repository[],
  repositoryId: number,
): string {
  return (
    repositories.find((repository) => repository.id === repositoryId)?.name ??
    `Repository ${repositoryId}`
  );
}

export function flattenHome(view: HomeView): ItemView[] {
  return [
    ...view.needs_attention,
    ...view.running,
    ...view.waiting,
    ...view.due,
    ...view.completed,
  ];
}

export function paneTabForRun(run: Run): PaneTab {
  return {
    paneId: run.pane_id,
    sessionName: run.session_name,
    runId: run.id,
    label: `Run #${run.id}`,
    available: run.pane_status === "available",
    paneIndex: 0,
    pid: 0,
    columns: 0,
    rows: 0,
    title: "",
    currentCommand: "",
    currentPath: run.working_directory,
  };
}

export function uniqueItems(items: ItemView[]): ItemView[] {
  return Array.from(new Map(items.map((item) => [item.item.id, item])).values());
}
