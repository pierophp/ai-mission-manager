import { type FormEvent, useEffect, useState } from "react";
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
import { Input } from "../../components/ui/input";
import { Textarea } from "../../components/ui/textarea";
import { currentMinute } from "../../runtime/time";
import { errorMessage } from "../../runtime/errors";
import type {
  AttentionEntry,
  ExternalChangePolicy,
  ExternalLinkView,
  ItemView,
  RunSuggestion,
} from "../../runtime/types";
import { ItemCard } from "./item-card";
import type { ItemForm } from "./item-signals";
import { displayItemIdentifier } from "./item-signals";
import { externalObjectKindLabel, formatSnapshotAge } from "./work-utils";
import { useWorkCommand, workActions } from "./work-mutations";

export function HomeColumn({
  title,
  hint,
  items,
  onOpenItem,
  onChanged,
}: {
  title: string;
  hint: string;
  items: ItemView[];
  onOpenItem: (itemId: number, form?: ItemForm) => void;
  onChanged: () => Promise<void>;
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
              onOpen={(form) => onOpenItem(view.item.id, form)}
              onChanged={onChanged}
            />
          ))}
        </div>
      )}
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
  onStop,
  onDelete,
}: {
  suggestion: RunSuggestion;
  disabled: boolean;
  onAttach: (suggestion: RunSuggestion) => Promise<void>;
  onStop: (suggestion: RunSuggestion) => void;
  onDelete: (suggestion: RunSuggestion) => void;
}) {
  return (
    <Card size="sm">
      <CardContent className="grid gap-3 pt-4 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)] md:items-center xl:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto]">
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
              ? suggestion.worktreeId
                ? "Registered Worktree"
                : "Registered direct checkout"
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
        <div className="flex flex-wrap justify-end gap-2 md:col-span-2 xl:col-span-1">
          <Button
            type="button"
            size="sm"
            variant="outline"
            disabled={disabled}
            onClick={() => void onAttach(suggestion)}
          >
            Attach Run
          </Button>
          <Button
            type="button"
            size="sm"
            variant="outline"
            disabled={disabled}
            onClick={() => onStop(suggestion)}
          >
            Stop
          </Button>
          <Button
            type="button"
            size="sm"
            variant="destructive"
            disabled={disabled}
            onClick={() => onDelete(suggestion)}
          >
            Delete
          </Button>
        </div>
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


export function SearchResult({
  view,
  onOpen,
}: {
  view: ItemView;
  onOpen: () => void;
}) {
  return (
    <Card size="sm">
      <CardContent className="pt-4">
        <button
          type="button"
          className="flex w-full items-center gap-3 rounded-md text-left outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
          onClick={onOpen}
        >
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
        </button>
      </CardContent>
    </Card>
  );
}
