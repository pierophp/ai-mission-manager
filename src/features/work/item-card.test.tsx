import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, expect, it } from "vitest";
import type { ItemView } from "../../runtime/types";
import { ItemCard } from "./item-card";

function itemViewWithRun(state: "working" | "finished"): ItemView {
  return {
    item: {
      id: 1,
      human_identifier: "APP-1",
      title: "Build the feature",
      project_id: 1,
      status: "Active",
      notes: "",
      reminders: [],
    },
    context_id: 1,
    context_name: "Product",
    project_name: "App",
    relationships: [],
    workspaces: [],
    worktrees: [],
    runs: [
      {
        id: 1,
        item_id: 1,
        workspace_id: null,
        repository_id: null,
        worktree_id: null,
        machine_id: 1,
        agent: "claude",
        cli_configuration_profile: null,
        execution_profile: "implement",
        model: null,
        effort: null,
        skill_snapshot: null,
        prompt: "",
        working_directory: "/app",
        session_name: "app",
        pane_id: "%1",
        started_at: 0,
        state,
        pane_status: "available",
        direct_checkouts: [],
        transcript: "",
        grill_question_group: null,
        grill_answers: [],
        grill_decisions: [],
        grill_response: null,
        grill_phase: null,
        grill_action: null,
      },
    ],
    implementation_queues: [],
    links: [],
  };
}

function renderItemCard(state: "working" | "finished") {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return renderToStaticMarkup(
    createElement(
      QueryClientProvider,
      { client },
      createElement(ItemCard, {
        view: itemViewWithRun(state),
        onOpen: () => {},
        onChanged: async () => {},
      }),
    ),
  );
}

describe("ItemCard", () => {
  it("highlights a card with an active Run", () => {
    const html = renderItemCard("working");

    expect(html).toContain("bg-primary/5");
    expect(html).toContain("ring-primary/30");
    expect(html).toContain("Run active");
  });

  it("keeps the default card color when its Run is finished", () => {
    const html = renderItemCard("finished");

    expect(html).not.toContain("bg-primary/5");
    expect(html).not.toContain("Run active");
  });
});
