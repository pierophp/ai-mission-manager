import { Badge } from "../../components/ui/badge";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "../../components/ui/card";
import { Empty, EmptyDescription } from "../../components/ui/empty";
import type {
  ActivityTabView,
  AuditAction,
  ExternalChange,
  ItemDeletionResult,
  ObservedActivity,
  ParentDeletionResult,
} from "../../runtime/types";
import { externalObjectKindLabel } from "../work/work-utils";
import { useActivityQuery } from "../work/work-queries";

export function ActivityPage() {
  const activityQuery = useActivityQuery();
  const activity = activityQuery.data ?? { audit_entries: [], activities: [] };

  return (
    <div className="mt-6 grid gap-5">
      <AuditRecord activity={activity} />
      <ObservedActivity activity={activity} />
    </div>
  );
}

function AuditRecord({ activity }: { activity: ActivityTabView }) {
  const entries = activity.audit_entries;

  return (
    <Card className="border-border/70 bg-card/80 shadow-sm">
      <CardHeader className="border-b border-border/70">
        <div className="flex items-start justify-between gap-4 max-[560px]:flex-col">
          <div>
            <CardDescription>Append-only record</CardDescription>
            <CardTitle className="mt-1">Recent actions</CardTitle>
          </div>
          <Badge variant="outline" className="shrink-0">
            {entries.length} {entries.length === 1 ? "entry" : "entries"}
          </Badge>
        </div>
      </CardHeader>
      <CardContent className="p-4">
        {entries.length === 0 ? (
          <Empty className="border-0 p-4">
            <EmptyDescription>No actions have been recorded yet.</EmptyDescription>
          </Empty>
        ) : (
          <ol className="grid gap-2">
            {entries.slice(0, 50).map((entry) => (
              <li
                className="flex items-start justify-between gap-4 border-b border-border/70 py-2 text-sm last:border-b-0 max-[560px]:flex-col max-[560px]:gap-1"
                key={entry.id}
              >
                <span>{auditActionLabel(entry.action)}</span>
                <time
                  className="shrink-0 text-xs text-muted-foreground"
                  dateTime={new Date(entry.recorded_at * 1000).toISOString()}
                >
                  {new Date(entry.recorded_at * 1000).toLocaleString()}
                </time>
              </li>
            ))}
          </ol>
        )}
      </CardContent>
    </Card>
  );
}

function ObservedActivity({ activity }: { activity: ActivityTabView }) {
  const activities = activity.activities;

  return (
    <Card className="border-border/70 bg-card/80 shadow-sm">
      <CardHeader className="border-b border-border/70">
        <div className="flex items-start justify-between gap-4 max-[560px]:flex-col">
          <div>
            <CardDescription>Observed External Objects</CardDescription>
            <CardTitle className="mt-1">Activity</CardTitle>
            <p className="mt-2 max-w-2xl text-sm text-muted-foreground">
              This is the record of changes observed from providers. Review state remains on each
              Link and appears in Needs Attention.
            </p>
          </div>
          <Badge variant="outline" className="shrink-0">
            {activities.length} {activities.length === 1 ? "observation" : "observations"}
          </Badge>
        </div>
      </CardHeader>
      <CardContent className="p-4">
        {activities.length === 0 ? (
          <Empty className="border-0 p-4">
            <EmptyDescription>No External Object changes have been observed yet.</EmptyDescription>
          </Empty>
        ) : (
          <div className="grid divide-y divide-border/70">
            {activities.slice(0, 50).map((entry) => (
              <ObservedActivityCard key={entry.activity.id} entry={entry} />
            ))}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

function ObservedActivityCard({ entry }: { entry: ObservedActivity }) {
  const titleChange = entry.activity.changes.find((change) => change.kind === "title");
  const stateChange = entry.activity.changes.find((change) => change.kind === "state");
  const title = titleChange?.current ?? titleChange?.previous ?? entry.object.external_key;
  const state = stateChange?.current ?? stateChange?.previous;

  return (
    <details className="group py-2 text-sm">
      <summary className="flex cursor-pointer list-outside items-start justify-between gap-4 py-2 marker:text-primary max-[560px]:flex-col max-[560px]:gap-1">
        <div className="grid min-w-0 gap-1">
          <strong className="font-medium">{title}</strong>
          <span className="text-xs text-muted-foreground">
            {externalObjectKindLabel(entry.object.kind)} · {entry.object.external_key}
            {state ? ` · observed state ${state}` : ""}
          </span>
        </div>
        <time
          className="shrink-0 text-xs text-muted-foreground"
          dateTime={new Date(entry.activity.observed_at * 1000).toISOString()}
        >
          {new Date(entry.activity.observed_at * 1000).toLocaleString()}
        </time>
      </summary>
      <div className="grid gap-3 pb-3 pl-6 text-xs text-muted-foreground">
        <a
          className="break-all text-primary underline-offset-4 hover:underline"
          href={entry.object.canonical_url}
          target="_blank"
          rel="noreferrer"
        >
          {entry.object.canonical_url}
        </a>
        <ul className="grid list-disc gap-1.5 pl-4">
          {entry.activity.changes.map((change, index) => (
            <li key={`${change.kind}-${change.key ?? ""}-${index}`}>
              {externalChangeDescription(change)}
            </li>
          ))}
        </ul>
      </div>
    </details>
  );
}

function externalChangeDescription(change: ExternalChange): string {
  const label =
    change.kind === "title"
      ? "Title"
      : change.kind === "state"
        ? "State"
        : `Metadata ${change.key ?? "value"}`;
  if (change.previous !== null && change.current !== null) {
    return `${label} changed from ${change.previous} to ${change.current}`;
  }
  if (change.current !== null) return `${label} added as ${change.current}`;
  if (change.previous !== null) return `${label} removed (was ${change.previous})`;
  return `${label} changed`;
}

function auditActionLabel(action: AuditAction): string {
  switch (action.action) {
    case "itemCreated":
      return `Created Item #${action.item_id}`;
    case "itemStatusChanged":
      return `Moved Item #${action.item_id} from ${action.from} to ${action.to}`;
    case "itemNotesChanged":
      return `Updated notes on Item #${action.item_id}`;
    case "itemRemindersChanged":
      return `Updated reminders on Item #${action.item_id}`;
    case "itemDeleted":
      return `Deleted Item #${
        action.summary && "itemId" in action.summary
          ? action.summary.itemId
          : action.item_id
      }${
        action.summary && "itemId" in action.summary
          ? ` · ${itemDeletionSummary(action.summary)}`
          : ""
      }`;
    case "projectDeleted":
      return `Deleted Project #${
        action.summary && "projectId" in action.summary
          ? action.summary.projectId
          : action.project_id
      }${
        action.summary && "projectId" in action.summary
          ? ` · ${parentDeletionSummary(action.summary)}`
          : ""
      }`;
    case "contextDeleted":
      return `Deleted Context #${
        action.summary && "contextId" in action.summary
          ? action.summary.contextId
          : action.context_id
      }${
        action.summary && "contextId" in action.summary
          ? ` · ${parentDeletionSummary(action.summary)}`
          : ""
      }`;
    case "itemRelationChanged":
      return `Updated the relationship between Items #${action.from_item_id} and #${action.to_item_id}`;
    case "workspaceCreated":
      return "Configured Repository access for an Item";
    case "workspaceRemoved":
      return `Removed Item Repository access · ${countLabel(
        action.repository_count,
        "Repository",
      )}`;
    case "workspaceUpdated":
      return "Updated Item Repository access";
    case "runCreated":
      return `Created Run #${action.run_id}`;
    case "runStopped":
      return `Stopped Run #${action.run_id}`;
    case "runDeleted":
      return `Deleted Run #${action.run_id} · no local descendants`;
    case "runStateChanged":
      return `Run #${action.run_id} changed from ${action.from} to ${action.to}`;
    case "runPaneStatusChanged":
      return `Run #${action.run_id} Pane changed from ${action.from} to ${action.to}`;
    case "externalObjectCreated":
      return `Added External Object #${action.external_object_id}`;
    case "externalObjectRefreshed":
      return `Refreshed External Object #${action.external_object_id}`;
    case "linkCreated":
      return `Linked External Object through Link #${action.link_id}`;
    case "linkUpdated":
      return `Updated Link #${action.link_id}`;
    case "linkDeleted":
      return `Removed Link #${action.link_id} · ${
        action.external_object_deleted === true
          ? `orphaned External Object #${action.external_object_id ?? "?"} removed`
          : action.external_object_deleted === false
            ? "shared External Object retained"
            : "External Object cascade details unavailable"
      }`;
    case "externalObjectDeleted":
      return `Removed External Object #${action.external_object_id} locally · ${externalObjectDeletionSummary(action)}`;
    case "contextCreated":
      return `Created Context #${action.context_id}`;
    case "projectCreated":
      return `Created Project #${action.project_id}`;
    case "repositoryRegistered":
      return `Registered Repository #${action.repository_id}`;
    case "repositoryDeleted":
      return `Deleted Repository #${action.repository_id}`;
    case "machineRegistered":
      return `Registered Machine #${action.machine_id}`;
    case "machineObserved":
      return `Observed Machine #${action.machine_id} as ${action.observation}`;
    case "machineDeleted":
      return `Deleted Machine #${action.machine_id} · ${countLabel(action.run_count, "Run")}`;
    case "contextAttentionDefaultChanged":
      return `Updated attention defaults for Context #${action.context_id}`;
    case "resetBoundary":
      return `Reset local data · new Personal Context #${action.context_id}, Default Project #${action.project_id}`;
    default:
      return "Recorded action";
  }
}

function itemDeletionSummary(summary: ItemDeletionResult["summary"]): string {
  return cascadeCounts([
    [summary.reminderCount, "reminder"],
    [summary.relationshipCount, "relationship"],
    [summary.runCount, "Run"],
    [summary.linkCount, "Link"],
    [summary.externalObjectCount, "orphaned External Object"],
    [summary.snapshotCount, "snapshot"],
    [summary.activityCount, "Activity record"],
  ]);
}

function externalObjectDeletionSummary(action: AuditAction): string {
  if (
    action.link_count === null ||
    action.link_count === undefined ||
    action.snapshot_count === null ||
    action.snapshot_count === undefined ||
    action.activity_count === null ||
    action.activity_count === undefined
  ) {
    return "cascade details unavailable";
  }
  return cascadeCounts([
    [action.link_count, "Link"],
    [action.snapshot_count, "snapshot"],
    [action.activity_count, "Activity record"],
  ]);
}

function parentDeletionSummary(summary: ParentDeletionResult["summary"]): string {
  return cascadeCounts([
    [summary.projectCount, "Project"],
    [summary.itemCount, "Item"],
    [summary.repositoryCount, "Repository"],
    [summary.machineCount, "Machine"],
    [summary.runCount, "Run"],
    [summary.reminderCount, "reminder"],
    [summary.relationshipCount, "relationship"],
    [summary.linkCount, "Link"],
    [summary.attentionDefaultCount, "attention default"],
    [summary.externalObjectCount, "orphaned External Object"],
    [summary.snapshotCount, "snapshot"],
    [summary.activityCount, "Activity record"],
  ]);
}

function cascadeCounts(counts: [number, string][]): string {
  const nonEmpty = counts
    .filter(([count]) => count > 0)
    .map(([count, label]) => `${count} ${label}${count === 1 ? "" : "s"}`);
  return nonEmpty.length > 0 ? nonEmpty.join(", ") : "no local descendants";
}

function countLabel(count: number | null | undefined, label: string): string {
  if (count === null || count === undefined) return `${label} count unavailable`;
  if (!count) return `no ${label.toLowerCase()} records`;
  return `${count} ${label}${count === 1 ? "" : "s"}`;
}
