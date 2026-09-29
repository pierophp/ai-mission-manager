import { describe, expect, it } from "vitest";
import { orderImplementationTickets, ticketIsOpen } from "./spec-queue";

describe("Implementation Queue ticket ordering", () => {
  it("keeps captured order while moving open blockers before their dependents", () => {
    const tickets = [
      { number: 1, title: "Dependent", state: "open", url: "https://tracker.test/1", ordinal: 1, blockedBy: ["https://tracker.test/2/"] },
      { number: 2, title: "Unrelated", state: "open", url: "https://tracker.test/3", ordinal: 2, blockedBy: [] },
      { number: 3, title: "Blocker", state: "open", url: "https://tracker.test/2", ordinal: 4, blockedBy: [] },
    ];

    expect(orderImplementationTickets(tickets, [tickets[0].url]).map((ticket) => ticket.title)).toEqual([
      "Blocker",
      "Dependent",
    ]);
    expect(orderImplementationTickets(tickets, [tickets[0].url, tickets[1].url]).map((ticket) => ticket.title)).toEqual([
      "Unrelated",
      "Blocker",
      "Dependent",
    ]);
  });

  it("treats provider-specific terminal states as closed", () => {
    expect(ticketIsOpen("Active")).toBe(true);
    expect(ticketIsOpen("To Do")).toBe(true);
    expect(ticketIsOpen("Resolved")).toBe(false);
    expect(ticketIsOpen("Closed")).toBe(false);
  });
});
