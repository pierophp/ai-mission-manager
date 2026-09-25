import { type FormEvent, useEffect, useState } from "react";
import { Badge } from "../../../components/ui/badge";
import { Button } from "../../../components/ui/button";
import { Input } from "../../../components/ui/input";
import {
  NativeSelect,
  NativeSelectOption,
} from "../../../components/ui/native-select";
import { Textarea } from "../../../components/ui/textarea";
import type { ItemRelationKind, ItemView } from "../../../runtime/types";
import { type ItemIntent, displayItemIdentifier } from "../item-signals";
import type { ItemCommands } from "../use-item-commands";
import { workActions } from "../work-mutations";
import { relationKindLabel, relationshipLabel } from "../work-utils";
import { SectionLabel, useFormIntent } from "./shared";

const relationKinds: ItemRelationKind[] = ["Blocks", "BlockedBy", "RelatedTo"];
const overviewForms = ["reminder"] as const;

export function OverviewTab({
  view,
  allItems,
  commands,
  intent,
  onNotesDirtyChange,
}: {
  view: ItemView;
  allItems: ItemView[];
  commands: ItemCommands;
  intent: ItemIntent | undefined;
  onNotesDirtyChange: (dirty: boolean) => void;
}) {
  const { isSaving, saveItem } = commands;
  const [notes, setNotes] = useState(view.item.notes);
  const [isAddingReminder, setIsAddingReminder] = useState(false);
  const [reminderAt, setReminderAt] = useState("");
  const [relationKind, setRelationKind] = useState<ItemRelationKind>("Blocks");
  const [targetItemId, setTargetItemId] = useState<number>();
  const notesDirty = notes !== view.item.notes;

  useEffect(() => {
    setNotes(view.item.notes);
  }, [view.item.notes]);

  useEffect(() => {
    onNotesDirtyChange(notesDirty);
  }, [notesDirty, onNotesDirtyChange]);

  useEffect(() => () => onNotesDirtyChange(false), [onNotesDirtyChange]);

  useFormIntent(intent, overviewForms, () => setIsAddingReminder(true));

  const visibleTargets = allItems.filter(
    (candidate) =>
      candidate.item.id !== view.item.id &&
      candidate.context_name === view.context_name,
  );

  async function handleAddReminder(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!reminderAt) return;
    const result = await saveItem(
      workActions.addReminder(view.item.id, reminderAt),
    );
    if (!result) return;
    setReminderAt("");
    setIsAddingReminder(false);
  }

  async function handleRelation(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!targetItemId) return;
    await saveItem(
      workActions.setRelation(view.item.id, targetItemId, relationKind),
    );
    setTargetItemId(undefined);
  }

  return (
    <div className="grid gap-6">
      <section className="grid gap-2">
        <label className="grid gap-1.5 text-sm font-medium">
          <span>Notes</span>
          <Textarea
            value={notes}
            onChange={(event) => setNotes(event.target.value)}
            placeholder="Add a useful handoff note"
            rows={5}
            disabled={isSaving}
          />
        </label>
        <div className="flex items-center gap-3">
          <Button
            size="sm"
            type="button"
            variant="outline"
            disabled={isSaving || !notesDirty}
            onClick={() =>
              void saveItem(workActions.setItemNotes(view.item.id, notes))
            }
          >
            Save notes
          </Button>
          {notesDirty && (
            <span className="text-xs text-muted-foreground">Unsaved changes</span>
          )}
        </div>
      </section>

      <section className="grid gap-2">
        <div className="flex items-center justify-between gap-2">
          <SectionLabel>Reminders</SectionLabel>
          {!isAddingReminder && (
            <Button
              type="button"
              size="xs"
              variant="ghost"
              disabled={isSaving}
              onClick={() => setIsAddingReminder(true)}
            >
              Add reminder
            </Button>
          )}
        </div>
        {isAddingReminder && (
          <form
            className="flex flex-wrap items-end gap-2 rounded-md border p-3"
            onSubmit={(event) => void handleAddReminder(event)}
          >
            <label className="grid min-w-56 flex-1 gap-1.5 text-sm font-medium">
              <span>Reminder date and time</span>
              <Input
                autoFocus
                type="datetime-local"
                value={reminderAt}
                onChange={(event) => setReminderAt(event.target.value)}
                disabled={isSaving}
              />
            </label>
            <Button type="submit" size="sm" disabled={isSaving || !reminderAt}>
              {isSaving ? "Saving…" : "Add reminder"}
            </Button>
            <Button
              type="button"
              size="sm"
              variant="ghost"
              disabled={isSaving}
              onClick={() => {
                setIsAddingReminder(false);
                setReminderAt("");
              }}
            >
              Cancel
            </Button>
          </form>
        )}
        {view.item.reminders.length === 0 ? (
          !isAddingReminder && (
            <span className="text-sm text-muted-foreground">None yet</span>
          )
        ) : (
          <div className="flex flex-wrap gap-2">
            {view.item.reminders.map((reminder) => (
              <Badge
                variant="secondary"
                className="h-auto gap-1 py-1"
                key={reminder.id}
              >
                {reminder.remind_at}
                <Button
                  type="button"
                  size="xs"
                  variant="ghost"
                  disabled={isSaving}
                  onClick={() =>
                    void saveItem(
                      workActions.removeReminder(view.item.id, reminder.id),
                    )
                  }
                >
                  Remove
                </Button>
              </Badge>
            ))}
          </div>
        )}
      </section>

      <section className="grid gap-2">
        <SectionLabel>Relationships</SectionLabel>
        {view.relationships.length === 0 ? (
          <span className="text-sm text-muted-foreground">None yet</span>
        ) : (
          <div className="flex flex-wrap gap-2">
            {view.relationships.map((relation) => {
              const otherId =
                relation.from_item_id === view.item.id
                  ? relation.to_item_id
                  : relation.from_item_id;
              const other = allItems.find(
                (candidate) => candidate.item.id === otherId,
              );
              return (
                <Badge
                  variant="secondary"
                  key={`${relation.from_item_id}-${relation.to_item_id}-${relation.kind}`}
                >
                  {relationshipLabel(relation, view.item.id)}{" "}
                  {displayItemIdentifier(
                    other?.item.human_identifier ?? `MC-${otherId}`,
                  )}
                </Badge>
              );
            })}
          </div>
        )}
        {visibleTargets.length > 0 && (
          <form
            className="grid gap-2 sm:grid-cols-[1fr_2fr_auto] sm:items-end"
            onSubmit={(event) => void handleRelation(event)}
          >
            <NativeSelect
              aria-label="Relationship kind"
              value={relationKind}
              onChange={(event) =>
                setRelationKind(event.target.value as ItemRelationKind)
              }
              disabled={isSaving}
            >
              {relationKinds.map((kind) => (
                <NativeSelectOption value={kind} key={kind}>
                  {relationKindLabel(kind)}
                </NativeSelectOption>
              ))}
            </NativeSelect>
            <NativeSelect
              aria-label="Related Item"
              value={targetItemId ?? ""}
              onChange={(event) => setTargetItemId(Number(event.target.value))}
              disabled={isSaving}
            >
              <NativeSelectOption value="">Choose an Item</NativeSelectOption>
              {visibleTargets.map((candidate) => (
                <NativeSelectOption
                  value={candidate.item.id}
                  key={candidate.item.id}
                >
                  {displayItemIdentifier(candidate.item.human_identifier)} ·{" "}
                  {candidate.item.title}
                </NativeSelectOption>
              ))}
            </NativeSelect>
            <Button
              type="submit"
              variant="outline"
              disabled={isSaving || !targetItemId}
            >
              Link
            </Button>
          </form>
        )}
      </section>
    </div>
  );
}
