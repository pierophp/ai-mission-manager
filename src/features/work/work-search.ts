import { type ItemDetailTab, itemDetailTabs } from "./item-signals";

export type WorkSearch = {
  contextId?: number;
  q?: string;
  item?: number;
  tab?: ItemDetailTab;
};

type UnknownSearch = Record<string, unknown>;

function positiveInteger(value: unknown): number | undefined {
  const candidate = typeof value === "number" ? value : Number(value);
  return Number.isInteger(candidate) && candidate > 0 ? candidate : undefined;
}

function itemDetailTab(value: unknown): ItemDetailTab | undefined {
  return itemDetailTabs.find((tab) => tab === value);
}

/**
 * Normalize the small amount of navigable state owned by Work.
 *
 * TanStack Router gives validators the decoded search object, but values from
 * a hand-written URL can still be malformed. Invalid values are defaults, so
 * a bad link cannot put the Work view into an impossible state. A tab only
 * means something while an Item is open.
 */
export function parseWorkSearch(search: UnknownSearch): WorkSearch {
  const contextId = positiveInteger(search.contextId);
  const query = typeof search.q === "string" ? search.q : undefined;
  const item = positiveInteger(search.item);
  const tab = item === undefined ? undefined : itemDetailTab(search.tab);

  return {
    ...(contextId === undefined ? {} : { contextId }),
    ...(query?.trim() ? { q: query } : {}),
    ...(item === undefined ? {} : { item }),
    ...(tab === undefined ? {} : { tab }),
  };
}
