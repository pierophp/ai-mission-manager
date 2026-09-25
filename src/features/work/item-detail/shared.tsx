import { type ReactNode, useEffect, useRef } from "react";
import type {
  Context,
  ItemView,
  Machine,
  Repository,
} from "../../../runtime/types";
import type { ItemForm, ItemIntent } from "../item-signals";

/** Where this Item's Runs and Worktrees execute. */
export function itemExecution(
  view: ItemView,
  repositories: Repository[],
  contexts: Context[],
  machines: Machine[],
) {
  const itemRepositories = repositories.filter(
    (repository) => repository.project_id === view.item.project_id,
  );
  const itemContext = contexts.find((context) => context.id === view.context_id);
  const executionMachineId = itemContext?.execution_machine_id ?? undefined;
  const executionMachine = machines.find(
    (machine) => machine.id === executionMachineId,
  );
  return { itemRepositories, itemContext, executionMachineId, executionMachine };
}

/**
 * Open a tab's form when the panel is asked to, once per request. Tabs stay
 * mounted, so a request is never replayed by switching tabs.
 */
export function useFormIntent(
  intent: ItemIntent | undefined,
  forms: readonly ItemForm[],
  open: (form: ItemForm) => void,
) {
  const handled = useRef<number>(undefined);
  const openRef = useRef(open);
  openRef.current = open;

  useEffect(() => {
    if (!intent || handled.current === intent.nonce) return;
    if (!forms.includes(intent.form)) return;
    handled.current = intent.nonce;
    openRef.current(intent.form);
  }, [intent, forms]);
}

export function SectionLabel({ children }: { children: ReactNode }) {
  return (
    <span className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
      {children}
    </span>
  );
}
