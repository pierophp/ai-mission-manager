import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, expect, it } from "vitest";
import { ExternalLinkCard } from "./components";
import type { ExternalObject } from "../../runtime/types";

function renderLinkCard(object: ExternalObject) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return renderToStaticMarkup(
    createElement(
      QueryClientProvider,
      { client },
      createElement(ExternalLinkCard, {
        externalLink: {
          link: {
            id: 1,
            item_id: 1,
            external_object_id: object.id,
            reviewed_activity_id: 0,
            attention_policy: null,
            watch_until: null,
            review_at: null,
            purpose: "others",
            spec_external_object_id: null,
            provenance: null,
          },
          object,
          snapshot: null,
          attention_policy: { title: false, state: false, metadata: false },
          attention_entry: null,
        },
        isSaving: false,
        onRefresh: async () => {},
        onUnlink: async () => {},
        onPrepareDeleteObject: async () => {},
        onSetPurpose: async () => {},
        specs: [],
        onSavePolicy: async () => {},
        onMarkReviewed: async () => {},
        onSaveWatchUntil: async () => {},
        onSaveReviewAt: async () => {},
        onClearReviewAt: async () => {},
        onAddComment: async () => {},
      }),
    ),
  );
}

describe("ExternalLinkCard", () => {
  it("shows concise labels for spec and ticket link types", () => {
    const html = renderLinkCard({
      id: 1,
      provider: "github",
      kind: "issue",
      external_key: "acme/app#1",
      canonical_url: "https://github.com/acme/app/issues/1",
    });

    expect(html).toContain(">Spec</option>");
    expect(html).toContain(">Tickets</option>");
    expect(html).not.toContain("To spec");
    expect(html).not.toContain("To tickets");
  });

  it.each([
    {
      label: "Confluence page",
      object: {
        id: 2,
        provider: "atlassian",
        kind: "document",
        external_key: "confluence:acme#123",
        canonical_url: "https://acme.atlassian.net/wiki/spaces/ENG/pages/123/spec",
      },
    },
    {
      label: "Jira issue",
      object: {
        id: 5,
        provider: "atlassian",
        kind: "issue",
        external_key: "jira:acme#APP-42",
        canonical_url: "https://acme.atlassian.net/browse/APP-42",
      },
    },
  ] as const)("allows a $label to be a Spec", ({ object }) => {
    const html = renderLinkCard(object);
    const option = html.match(/<option[^>]*value="to-spec"[^>]*>Spec<\/option>/)?.[0];
    expect(option).toBeDefined();
    expect(option).not.toContain("disabled");
  });

  it("keeps Azure DevOps work items as regular Links", () => {
    const html = renderLinkCard({
      id: 3,
      provider: "azure_dev_ops",
      kind: "issue",
      external_key: "ado:acme/apps#42",
      canonical_url: "https://dev.azure.com/acme/apps/_workitems/edit/42",
    });
    for (const purpose of ["to-spec", "to-tickets"]) {
      const option = html.match(new RegExp(`<option[^>]*value="${purpose}"[^>]*>.*?<\\/option>`))?.[0];
      expect(option).toBeDefined();
      expect(option).toContain("disabled");
    }
    expect(html).toContain('value="others"');
  });

  it("keeps pull requests ineligible as Specs", () => {
    const html = renderLinkCard({
      id: 4,
      provider: "azure_dev_ops",
      kind: "pull_request",
      external_key: "ado:acme/apps#8",
      canonical_url: "https://dev.azure.com/acme/apps/_git/app/pullrequest/8",
    });
    const option = html.match(/<option[^>]*value="to-spec"[^>]*>Spec<\/option>/)?.[0];
    expect(option).toContain("disabled");
  });
});
