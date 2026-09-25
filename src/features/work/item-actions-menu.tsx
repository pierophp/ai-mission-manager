import { useState } from "react";
import { EllipsisVerticalIcon } from "lucide-react";
import { Button } from "../../components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "../../components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "../../components/ui/dropdown-menu";
import { errorMessage } from "../../runtime/errors";
import type {
  ItemDeletionPreview,
  ItemStatus,
  ItemView,
} from "../../runtime/types";
import {
  type ItemForm,
  activeRuns,
  displayItemIdentifier,
  isRunFinished,
} from "./item-signals";
import type { ItemCommands } from "./use-item-commands";
import { workActions } from "./work-mutations";

const itemStatuses: ItemStatus[] = ["Inbox", "Active", "Waiting", "Done"];

/**
 * The Item's actions. Actions that need a form hand off to the detail panel
 * through `onOpenForm`; status changes and deletion act here, behind
 * confirmation dialogs only.
 */
export function ItemActionsMenu({
  view,
  commands,
  onOpenForm,
}: {
  view: ItemView;
  commands: ItemCommands;
  onOpenForm: (form: ItemForm) => void;
}) {
  const { isSaving, saveItem, whileSaving, confirm, workCommand, onChanged } =
    commands;
  const [deletionPreview, setDeletionPreview] = useState<ItemDeletionPreview>();
  const displayIdentifier = displayItemIdentifier(view.item.human_identifier);
  const activeGrillRun = view.runs.find(
    (run) => run.execution_profile === "grill" && !isRunFinished(run),
  );

  function handleItemStatusChange(nextStatus: ItemStatus) {
    if (nextStatus === "Done" && view.item.status !== "Done") {
      const running = activeRuns(view);
      if (running.length > 0) {
        confirm({
          title: "Complete this Item with active Runs?",
          description: `${running.length} Run${running.length === 1 ? " is" : "s are"} still active. Completing the Item will not stop them.`,
          confirmLabel: "Complete Item",
          onConfirm: () =>
            void saveItem(workActions.setItemStatus(view.item.id, nextStatus)),
        });
        return;
      }
    }
    void saveItem(workActions.setItemStatus(view.item.id, nextStatus));
  }

  async function handlePrepareItemDeletion() {
    await whileSaving(async () => {
      try {
        setDeletionPreview(
          await workCommand.execute(
            workActions.prepareItemDeletion(view.item.id),
            false,
          ),
        );
      } catch (previewError) {
        window.alert(errorMessage(previewError));
      }
    });
  }

  async function handleDeleteItem() {
    if (!deletionPreview || deletionPreview.blockers.length > 0) return;
    await whileSaving(async () => {
      try {
        const result = await workCommand.execute(
          workActions.deleteItem(view.item.id),
        );
        setDeletionPreview(undefined);
        await onChanged();
        const summary = result.summary;
        window.alert(
          `Deleted ${displayItemIdentifier(deletionPreview.plan.humanIdentifier)}.\n\nRemoved ${summary.reminderCount} reminder(s), ${summary.relationshipCount} relationship(s), ${summary.runCount} Run(s), ${summary.linkCount} Link(s), and ${summary.externalObjectCount} orphaned External Object(s).`,
        );
      } catch (deleteError) {
        setDeletionPreview(undefined);
        window.alert(errorMessage(deleteError));
      }
    });
  }

  return (
    <>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            type="button"
            size="icon-sm"
            variant="ghost"
            aria-label={`More actions for ${displayIdentifier}`}
            title="More actions"
            disabled={isSaving}
            onClick={(event) => event.stopPropagation()}
          >
            <EllipsisVerticalIcon aria-hidden="true" />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="w-48">
          <DropdownMenuLabel>Item actions</DropdownMenuLabel>
          <DropdownMenuItem
            disabled={isSaving || Boolean(activeGrillRun)}
            onSelect={() => onOpenForm("grill")}
          >
            {activeGrillRun ? "Grill Run already active" : "Start Grill Run"}
          </DropdownMenuItem>
          <DropdownMenuItem
            disabled={isSaving || !view.workspaces[0]}
            onSelect={() => onOpenForm("direct-run")}
          >
            Start Direct Run
          </DropdownMenuItem>
          <DropdownMenuItem
            disabled={isSaving}
            onSelect={() => onOpenForm("rename")}
          >
            Rename title
          </DropdownMenuItem>
          <DropdownMenuItem
            disabled={isSaving}
            onSelect={() => onOpenForm("reminder")}
          >
            Add reminder
          </DropdownMenuItem>
          <DropdownMenuSeparator />
          <DropdownMenuItem
            disabled={isSaving}
            onSelect={() => onOpenForm("link")}
          >
            Add external link
          </DropdownMenuItem>
          <DropdownMenuItem
            disabled={isSaving}
            onSelect={() => onOpenForm("issue")}
          >
            Create GitHub Issue
          </DropdownMenuItem>
          <DropdownMenuSub>
            <DropdownMenuSubTrigger>Change status</DropdownMenuSubTrigger>
            <DropdownMenuSubContent>
              <DropdownMenuRadioGroup
                value={view.item.status}
                onValueChange={(nextStatus) =>
                  handleItemStatusChange(nextStatus as ItemStatus)
                }
              >
                {itemStatuses.map((status) => (
                  <DropdownMenuRadioItem value={status} key={status}>
                    {status}
                  </DropdownMenuRadioItem>
                ))}
              </DropdownMenuRadioGroup>
            </DropdownMenuSubContent>
          </DropdownMenuSub>
          <DropdownMenuSeparator />
          <DropdownMenuItem
            variant="destructive"
            disabled={isSaving}
            onSelect={() => void handlePrepareItemDeletion()}
          >
            Delete Item
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
      {deletionPreview && (
        <Dialog
          open
          onOpenChange={(open) => {
            if (!open && !isSaving) setDeletionPreview(undefined);
          }}
        >
          <DialogContent className="max-h-[calc(100vh-2rem)] overflow-y-auto sm:max-w-2xl">
            <DialogHeader>
              <DialogTitle>
                Delete {displayItemIdentifier(deletionPreview.plan.humanIdentifier)}?
              </DialogTitle>
              <DialogDescription>
                This removes the Item and its local descendants. External Issues
                and pull requests are never changed.
              </DialogDescription>
            </DialogHeader>
            <div className="grid gap-4 text-sm">
              <ul className="grid gap-1 pl-5">
                <li>{deletionPreview.plan.reminderCount} reminder(s)</li>
                <li>
                  {deletionPreview.plan.relationshipCount} Item relationship(s)
                </li>
                <li>{deletionPreview.plan.runIds.length} Run(s)</li>
                <li>{deletionPreview.plan.linkIds.length} Link(s)</li>
                <li>
                  {deletionPreview.plan.orphanedExternalObjectIds.length}{" "}
                  orphaned External Object(s),{" "}
                  {deletionPreview.plan.orphanedSnapshotCount} snapshot(s), and{" "}
                  {deletionPreview.plan.orphanedActivityCount} Activity record(s)
                </li>
              </ul>
              {deletionPreview.blockers.length > 0 && (
                <div className="grid gap-1 rounded-md border border-destructive/30 bg-destructive/5 p-3 text-destructive">
                  <strong>Deletion blocked</strong>
                  {deletionPreview.blockers.map((blocker) => (
                    <span key={blocker}>{blocker}</span>
                  ))}
                  <span>Resolve each blocker, then create a fresh preview.</span>
                </div>
              )}
            </div>
            <DialogFooter>
              <Button
                type="button"
                variant="outline"
                disabled={isSaving}
                onClick={() => setDeletionPreview(undefined)}
              >
                Cancel
              </Button>
              <Button
                type="button"
                variant="destructive"
                disabled={isSaving || deletionPreview.blockers.length > 0}
                onClick={() => void handleDeleteItem()}
              >
                {isSaving ? "Deleting…" : "Confirm logical deletion"}
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      )}
    </>
  );
}
