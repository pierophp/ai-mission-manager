export type WorkSearch = {
  contextId?: number;
  q?: string;
};

type UnknownSearch = Record<string, unknown>;

function positiveInteger(value: unknown): number | undefined {
  const candidate = typeof value === "number" ? value : Number(value);
  return Number.isInteger(candidate) && candidate > 0 ? candidate : undefined;
}
/**
 * Normalize the small amount of navigable state owned by Work.
 *
 * TanStack Router gives validators the decoded search object, but values from
 * a hand-written URL can still be malformed. Invalid values are defaults, so
 * a bad link cannot put the Work view into an impossible state.
 */
export function parseWorkSearch(search: UnknownSearch): WorkSearch {
  const contextId = positiveInteger(search.contextId);
  const query = typeof search.q === "string" ? search.q : undefined;

  return {
    ...(contextId === undefined ? {} : { contextId }),
    ...(query?.trim() ? { q: query } : {}),
  };
}
