import { Badge } from "../../components/ui/badge";
import { Card, CardHeader } from "../../components/ui/card";
import type { ItemView } from "../../runtime/types";
import { ItemActionsMenu } from "./item-actions-menu";
import { type ItemForm, displayItemIdentifier, itemSignals } from "./item-signals";
import { useItemCommands } from "./use-item-commands";

/**
 * The collapsed Item on the board. Everything else lives in the Item modal;
 * the card only surfaces what needs the user's action.
 */
export function ItemCard({
  view,
  onOpen,
  onChanged,
}: {
  view: ItemView;
  onOpen: (form?: ItemForm) => void;
  onChanged: () => Promise<void>;
}) {
  const commands = useItemCommands(onChanged);
  const signals = itemSignals(view);

  return (
    <Card size="sm" className="h-full">
      <CardHeader>
        <div className="flex items-start justify-between gap-2">
          <button
            type="button"
            className="grid min-w-0 flex-1 gap-1 rounded-md text-left outline-none focus-visible:ring-3 focus-visible:ring-ring/50"
            onClick={() => onOpen()}
          >
            <span className="flex flex-wrap items-center gap-2">
              <Badge variant="outline">
                {displayItemIdentifier(view.item.human_identifier)}
              </Badge>
              <Badge variant="secondary">{view.item.status}</Badge>
            </span>
            <span className="font-heading text-base leading-snug font-medium group-data-[size=sm]/card:text-sm">
              {view.item.title}
            </span>
            <span className="text-sm text-muted-foreground">
              {view.context_name} <span>·</span> {view.project_name}
            </span>
            {(signals.grillWaiting || signals.runActive || signals.reminderDue) && (
              <span className="flex flex-wrap gap-1.5">
                {signals.grillWaiting && (
                  <Badge className="bg-amber-500/15 text-amber-800 dark:text-amber-200">
                    Grill waiting for answers
                  </Badge>
                )}
                {signals.runActive && <Badge variant="outline">Run active</Badge>}
                {signals.reminderDue && (
                  <Badge variant="outline">Reminder due</Badge>
                )}
              </span>
            )}
          </button>
          <ItemActionsMenu view={view} commands={commands} onOpenForm={onOpen} />
        </div>
      </CardHeader>
      {commands.confirmationDialog}
    </Card>
  );
}
