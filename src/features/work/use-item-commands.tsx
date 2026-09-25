import { useState } from "react";
import { ConfirmationDialog } from "../../components/ui/confirmation-dialog";
import { errorMessage } from "../../runtime/errors";
import { type WorkAction, useWorkCommand } from "./work-mutations";

type WorkConfirmation = {
  title: string;
  description: string;
  confirmLabel: string;
  onConfirm: () => void;
};

/**
 * One saving flag and one confirmation slot for everything that acts on an
 * Item from a single surface (its card or its detail panel), so every control
 * on that surface is disabled while any of its commands runs.
 */
export function useItemCommands(onChanged: () => Promise<void>) {
  const workCommand = useWorkCommand();
  const [isSaving, setIsSaving] = useState(false);
  const [confirmation, setConfirmation] = useState<WorkConfirmation>();

  /** Run a command, refresh Work, and alert on failure. */
  async function saveItem<TData>(
    update: WorkAction<TData>,
    invalidate = true,
  ): Promise<TData | undefined> {
    setIsSaving(true);
    try {
      const result = await workCommand.execute(update, invalidate);
      await onChanged();
      return result;
    } catch (saveError) {
      window.alert(errorMessage(saveError));
      return undefined;
    } finally {
      setIsSaving(false);
    }
  }

  /** Hold the saving flag around a task that handles its own errors. */
  async function whileSaving<T>(task: () => Promise<T>): Promise<T> {
    setIsSaving(true);
    try {
      return await task();
    } finally {
      setIsSaving(false);
    }
  }

  function confirm(request: WorkConfirmation) {
    setConfirmation({
      ...request,
      onConfirm: () => {
        setConfirmation(undefined);
        request.onConfirm();
      },
    });
  }

  const confirmationDialog = confirmation && (
    <ConfirmationDialog
      open
      title={confirmation.title}
      description={confirmation.description}
      confirmLabel={confirmation.confirmLabel}
      disabled={isSaving}
      onOpenChange={(open) => {
        if (!open && !isSaving) setConfirmation(undefined);
      }}
      onConfirm={confirmation.onConfirm}
    />
  );

  return {
    workCommand,
    isSaving,
    saveItem,
    whileSaving,
    confirm,
    confirmationDialog,
    onChanged,
  };
}

export type ItemCommands = ReturnType<typeof useItemCommands>;
