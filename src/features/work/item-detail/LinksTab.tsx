import { type FormEvent, useState } from "react";
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "../../../components/ui/alert";
import { Button } from "../../../components/ui/button";
import { Input } from "../../../components/ui/input";
import {
  NativeSelect,
  NativeSelectOption,
} from "../../../components/ui/native-select";
import { Textarea } from "../../../components/ui/textarea";
import { errorMessage } from "../../../runtime/errors";
import type {
  ExternalObjectDeletionPreview,
  ItemView,
  Repository,
} from "../../../runtime/types";
import { ExternalLinkCard } from "../components";
import {
  type ItemForm,
  type ItemIntent,
  displayItemIdentifier,
} from "../item-signals";
import type { ItemCommands } from "../use-item-commands";
import { workActions } from "../work-mutations";
import { externalObjectKindLabel } from "../work-utils";
import { useFormIntent } from "./shared";

const linkForms = ["link", "issue"] as const;

export function LinksTab({
  view,
  repositories,
  commands,
  intent,
}: {
  view: ItemView;
  repositories: Repository[];
  commands: ItemCommands;
  intent: ItemIntent | undefined;
}) {
  const { isSaving, saveItem, whileSaving, confirm, workCommand, onChanged } =
    commands;
  const [openForm, setOpenForm] = useState<"link" | "issue">();
  const [externalUrl, setExternalUrl] = useState("");
  const [issueRepository, setIssueRepository] = useState<number>();
  const [issueTitle, setIssueTitle] = useState(view.item.title);
  const [issueBody, setIssueBody] = useState(view.item.notes);
  const [externalObjectDeletionPreview, setExternalObjectDeletionPreview] =
    useState<ExternalObjectDeletionPreview>();
  const itemRepositories = repositories.filter(
    (repository) => repository.project_id === view.item.project_id,
  );

  function openLinkForm(form: ItemForm) {
    if (form === "issue") {
      setIssueRepository(itemRepositories[0]?.id);
      setIssueTitle(view.item.title);
      setIssueBody(view.item.notes);
      setOpenForm("issue");
    } else if (form === "link") {
      setOpenForm("link");
    }
  }

  useFormIntent(intent, linkForms, openLinkForm);

  async function handleExternalLink(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!externalUrl.trim()) return;
    await whileSaving(async () => {
      try {
        const result = await workCommand.execute(
          workActions.linkExternalObject(view.item.id, externalUrl),
        );
        setExternalUrl("");
        setOpenForm(undefined);
        await onChanged();
        if (result.warning) window.alert(result.warning);
      } catch (linkError) {
        window.alert(errorMessage(linkError));
      }
    });
  }

  async function handleCreateIssue(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (issueRepository === undefined || !issueTitle.trim()) return;
    await whileSaving(async () => {
      try {
        const result = await workCommand.execute(
          workActions.createGithubIssue(
            view.item.id,
            issueRepository,
            issueTitle,
            issueBody,
          ),
        );
        setOpenForm(undefined);
        setIssueRepository(undefined);
        await onChanged();
        if (result.warning) window.alert(result.warning);
      } catch (createError) {
        window.alert(errorMessage(createError));
      }
    });
  }

  async function refreshExternalObject(externalObjectId: number) {
    await whileSaving(async () => {
      try {
        await workCommand.execute(
          workActions.refreshExternalObject(externalObjectId),
        );
        await onChanged();
      } catch (refreshError) {
        window.alert(errorMessage(refreshError));
      }
    });
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
    confirm({
      title: "Remove this Link from the Item?",
      description:
        "Its Link-scoped attention state will be removed. The GitHub Issue, pull request, or other provider-owned object will not be deleted.",
      confirmLabel: "Remove Link",
      onConfirm: () => void executeUnlinkExternalLink(linkId),
    });
  }

  async function handlePrepareExternalObjectDeletion(externalObjectId: number) {
    await whileSaving(async () => {
      try {
        setExternalObjectDeletionPreview(
          await workCommand.execute(
            workActions.prepareExternalObjectDeletion(externalObjectId),
            false,
          ),
        );
      } catch (previewError) {
        window.alert(errorMessage(previewError));
      }
    });
  }

  function handleDeleteExternalObject() {
    if (!externalObjectDeletionPreview) return;
    const { plan } = externalObjectDeletionPreview;
    confirm({
      title: `Remove this ${externalObjectKindLabel(plan.kind)} locally?`,
      description: `This will remove ${plan.linkIds.length} Link(s), ${plan.snapshotCount} snapshot(s), and ${plan.activityCount} Activity record(s). Provider-owned objects are never deleted.`,
      confirmLabel: "Remove locally",
      onConfirm: () => void executeDeleteExternalObject(plan.externalObjectId),
    });
  }

  async function executeDeleteExternalObject(externalObjectId: number) {
    await whileSaving(async () => {
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
      }
    });
  }

  return (
    <div className="grid gap-4">
      {!openForm && (
        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            size="sm"
            variant="outline"
            disabled={isSaving}
            onClick={() => openLinkForm("link")}
          >
            Add external link
          </Button>
          <Button
            type="button"
            size="sm"
            variant="outline"
            disabled={isSaving}
            onClick={() => openLinkForm("issue")}
          >
            Create GitHub Issue
          </Button>
        </div>
      )}

      {openForm === "link" && (
        <form
          className="grid gap-3 rounded-lg border border-primary/30 bg-primary/5 p-4"
          onSubmit={(event) => void handleExternalLink(event)}
        >
          <div>
            <h4 className="m-0 text-base font-medium">Add external link</h4>
            <p className="mt-1 text-sm text-muted-foreground">
              Link a GitHub Issue, pull request, or other external URL to this Item.
            </p>
          </div>
          <label className="grid gap-1.5 text-sm font-medium">
            <span>External URL</span>
            <Input
              autoFocus
              value={externalUrl}
              onChange={(event) => setExternalUrl(event.target.value)}
              placeholder="Paste a GitHub issue, pull request, or URL"
              disabled={isSaving}
            />
          </label>
          <div className="flex flex-wrap gap-2">
            <Button type="submit" disabled={isSaving || !externalUrl.trim()}>
              {isSaving ? "Adding…" : "Add link"}
            </Button>
            <Button
              type="button"
              variant="ghost"
              disabled={isSaving}
              onClick={() => setOpenForm(undefined)}
            >
              Cancel
            </Button>
          </div>
        </form>
      )}

      {openForm === "issue" && (
        <form
          className="grid gap-3 rounded-lg border border-primary/30 bg-primary/5 p-4"
          onSubmit={(event) => void handleCreateIssue(event)}
        >
          <div>
            <h4 className="m-0 text-base font-medium">Create GitHub Issue</h4>
            <p className="mt-1 text-sm text-muted-foreground">
              Nothing is sent until you confirm. The existing Item remains
              unchanged, and the created Issue will be linked to it.
            </p>
          </div>
          {itemRepositories.length > 0 ? (
            <label className="grid gap-1.5 text-sm font-medium">
              <span>Repository</span>
              <NativeSelect
                autoFocus
                value={issueRepository ?? ""}
                onChange={(event) =>
                  setIssueRepository(Number(event.target.value) || undefined)
                }
                disabled={isSaving}
              >
                {itemRepositories.map((repository) => (
                  <NativeSelectOption value={repository.id} key={repository.id}>
                    {repository.name} · {repository.remote_url}
                  </NativeSelectOption>
                ))}
              </NativeSelect>
            </label>
          ) : (
            <p className="text-sm text-muted-foreground">
              Register a Repository for this Item&apos;s Project before creating
              a GitHub Issue.
            </p>
          )}
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
              rows={5}
              placeholder="Optional public context"
              disabled={isSaving}
            />
          </label>
          <div className="flex flex-wrap gap-2">
            <Button
              type="submit"
              disabled={
                isSaving || issueRepository === undefined || !issueTitle.trim()
              }
            >
              {isSaving ? "Creating…" : "Confirm and create Issue"}
            </Button>
            <Button
              type="button"
              variant="ghost"
              disabled={isSaving}
              onClick={() => setOpenForm(undefined)}
            >
              Cancel
            </Button>
          </div>
        </form>
      )}

      {view.links.length === 0 && !openForm && (
        <span className="text-sm text-muted-foreground">
          No external links yet.
        </span>
      )}
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
          onSetSpec={async (isSpec) => {
            await saveItem(
              workActions.setLinkSpec(externalLink.link.id, isSpec),
            );
          }}
          onSavePolicy={async (policy) => {
            await saveItem(
              workActions.setLinkAttentionPolicy(externalLink.link.id, policy),
            );
          }}
          onMarkReviewed={async () => {
            await saveItem(workActions.markLinkReviewed(externalLink.link.id));
          }}
          onSaveWatchUntil={async (watchUntil) => {
            await saveItem(
              workActions.setLinkWatchUntil(externalLink.link.id, watchUntil),
            );
          }}
          onSaveReviewAt={async (reviewAt) => {
            await saveItem(
              workActions.setLinkReviewAt(externalLink.link.id, reviewAt),
            );
          }}
          onClearReviewAt={async () => {
            await saveItem(workActions.clearLinkReviewAt(externalLink.link.id));
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
              This removes the local External Object record and every Link to
              it. It does not call GitHub or any other provider, so
              provider-owned Issues, pull requests, and other objects are never
              deleted.
            </p>
            <ul className="grid gap-1 pl-5">
              <li>{externalObjectDeletionPreview.plan.linkIds.length} Link(s)</li>
              <li>
                {externalObjectDeletionPreview.plan.snapshotCount} snapshot(s)
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
              onClick={handleDeleteExternalObject}
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
    </div>
  );
}
