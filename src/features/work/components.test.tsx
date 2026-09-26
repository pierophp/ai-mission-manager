import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ExternalLinkCard } from "./components";

describe("ExternalLinkCard", () => {
  it("shows concise labels for spec and ticket link types", () => {
    const html = renderToStaticMarkup(
      createElement(ExternalLinkCard, {
        externalLink: {
          link: {
            id: 1,
            item_id: 1,
            external_object_id: 1,
            reviewed_activity_id: 0,
            attention_policy: null,
            watch_until: null,
            review_at: null,
            purpose: "to-spec",
            spec_external_object_id: null,
            provenance: null,
          },
          object: {
            id: 1,
            provider: "github",
            kind: "issue",
            external_key: "acme/app#1",
            canonical_url: "https://github.com/acme/app/issues/1",
          },
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
    );

    expect(html).toContain(">Spec</option>");
    expect(html).toContain(">Tickets</option>");
    expect(html).not.toContain("To spec");
    expect(html).not.toContain("To tickets");
  });
});
