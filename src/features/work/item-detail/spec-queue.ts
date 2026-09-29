import type { SubIssue } from "../../../runtime/types";

export type ImplementationTicket = SubIssue & {
  blockedBy: string[];
  ordinal: number;
};

export function ticketIsOpen(state: string): boolean {
  return !["closed", "done", "resolved", "completed", "removed", "cancelled", "canceled"].includes(
    state.trim().toLowerCase(),
  );
}

function normalizedUrl(url: string): string {
  return url.split(/[?#]/, 1)[0].replace(/\/$/, "");
}

/** Include open blockers and return the selected tickets in stable dependency order. */
export function orderImplementationTickets(
  tickets: ImplementationTicket[],
  selectedUrls: string[],
): ImplementationTicket[] {
  const byUrl = new Map(tickets.map((ticket) => [normalizedUrl(ticket.url), ticket]));
  const included = new Set(selectedUrls.map(normalizedUrl));
  const includeBlockers = (ticket: ImplementationTicket) => {
    for (const blockedBy of ticket.blockedBy) {
      const dependencyUrl = normalizedUrl(blockedBy);
      const dependency = byUrl.get(dependencyUrl);
      if (dependency && ticketIsOpen(dependency.state) && !included.has(dependencyUrl)) {
        included.add(dependencyUrl);
        includeBlockers(dependency);
      }
    }
  };
  for (const url of [...included]) {
    const ticket = byUrl.get(url);
    if (ticket) includeBlockers(ticket);
  }

  const remaining = new Map(
    tickets
      .filter((ticket) => ticketIsOpen(ticket.state) && included.has(normalizedUrl(ticket.url)))
      .map((ticket) => [normalizedUrl(ticket.url), ticket]),
  );
  const ordered: ImplementationTicket[] = [];
  while (remaining.size) {
    const next = [...remaining.values()].find((ticket) =>
      ticket.blockedBy.every((url) => !remaining.has(normalizedUrl(url))),
    ) ?? remaining.values().next().value;
    if (!next) break;
    ordered.push(next);
    remaining.delete(normalizedUrl(next.url));
  }
  return ordered;
}
