import { describe, expect, it } from "vitest";
import type { RunLaunchOptions } from "../../../runtime/types";
import {
  changeDraftProfile,
  changeDraftWorkflow,
  checkoutApprovalsReady,
  draftForProfile,
  initialLaunchDraft,
  launchDraftSignature,
  previewIsCurrent,
  updateDraftConfiguration,
} from "./run-launch-draft";

const options: RunLaunchOptions = {
  defaultWorkflow: "pstack",
  workflows: [
    {
      workflow: "matt-pocock",
      defaultProfile: "grill",
      profiles: [
        { executionProfile: "grill", configuration: { agent: "claude", model: "grill", effort: "high" }, requiresInitialPrompt: true },
        { executionProfile: "implement", configuration: { agent: "codex", model: "implement", effort: "medium" }, requiresInitialPrompt: false },
        { executionProfile: "custom", configuration: { agent: "claude", model: "custom", effort: "high" }, requiresInitialPrompt: true },
      ],
    },
    {
      workflow: "pstack",
      defaultProfile: "autonomous",
      profiles: [
        { executionProfile: "autonomous", configuration: { agent: "claude", model: "pstack", effort: "high" }, requiresInitialPrompt: false },
        { executionProfile: "pstack-review", configuration: { agent: "codex", model: "review", effort: "high" }, requiresInitialPrompt: true },
      ],
    },
  ],
};

describe("Run launch draft", () => {
  it("uses projected defaults and falls back to a workflow offering Implement when needed", () => {
    expect(initialLaunchDraft(options, "checkout", "Notes")).toMatchObject({
      workflow: "pstack", profile: "autonomous", configuration: { model: "pstack" }, initialPrompt: "Notes",
    });
    expect(draftForProfile(options, "checkout", "implement")).toMatchObject({
      workflow: "matt-pocock", profile: "implement", configuration: { model: "implement" },
    });
  });

  it("resets a workflow change to the projected defaults and preserves touched configuration across profile changes", () => {
    const initial = initialLaunchDraft(options, "checkout");
    const changed = changeDraftWorkflow(options, initial, "matt-pocock");
    expect(changed).toMatchObject({ workflow: "matt-pocock", profile: "grill", configuration: { model: "grill" } });
    const touched = updateDraftConfiguration(changed, { agent: "codex", model: "chosen", effort: "low" });
    expect(changeDraftProfile(options, touched, "implement").configuration.model).toBe("chosen");
  });

  it("includes workflow in preview staleness and checks both checkout approvals", () => {
    const draft = initialLaunchDraft(options, "checkout", "Notes");
    const signature = launchDraftSignature(draft);
    expect(previewIsCurrent(signature, draft)).toBe(true);
    expect(previewIsCurrent(signature, { ...draft, workflow: "matt-pocock" })).toBe(false);
    expect(checkoutApprovalsReady({ dirtyRepositoryCount: 1, sharedPathCount: 1, dirtyConfirmed: true, sharedConfirmed: false })).toBe(false);
    expect(checkoutApprovalsReady({ dirtyRepositoryCount: 1, sharedPathCount: 1, dirtyConfirmed: true, sharedConfirmed: true })).toBe(true);
  });
});
