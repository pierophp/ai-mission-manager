// @vitest-environment happy-dom

import { act, createElement } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ItemCommands } from "../use-item-commands";
import { RunsTab } from "./RunsTab";
import type { ItemView } from "../../../runtime/types";
import { workActions } from "../work-mutations";

const actionMocks = vi.hoisted(() => ({
  prepareGrillRun: vi.fn(),
  composeGrillPrompt: vi.fn(),
  startGrillRun: vi.fn(),
}));

vi.mock("../work-mutations", () => ({ workActions: actionMocks }));

const preview = {
  workspaceId: 1,
  machineId: 1,
  machineName: "Local",
  workingDirectory: "/tmp/app",
  checkouts: [{ repositoryId: 1, path: "/tmp/app", branch: "main" }],
  checkoutDetails: [
    {
      repositoryId: 1,
      repositoryName: "app",
      path: "/tmp/app",
      branch: "main",
      isDirty: false,
    },
  ],
  currentBranches: ["main"],
  dirtyRepositoryIds: [],
  sharedRuns: [],
  sharedPaths: [],
};

const view = {
  item: {
    id: 1,
    human_identifier: "APP-1",
    title: "Review the approach",
    project_id: 1,
    status: "Active",
    notes: "",
    reminders: [],
  },
  context_id: 1,
  context_name: "Default",
  project_name: "App",
  relationships: [],
  workspaces: [
    { id: 1, item_id: 1, repositories: [], preparation_state: "ready" },
  ],
  worktrees: [],
  runs: [],
  implementation_queues: [],
  links: [],
} as ItemView;

const contexts = [
  {
    id: 1,
    name: "Default",
    execution_machine_id: null,
    claude_profile_id: null,
    codex_profile_id: null,
    check_dirty_checkouts: false,
    grill_defaults: {
      agent: "claude" as const,
      model: "claude-sonnet-4-5",
      effort: "high",
    },
    implement_defaults: {
      agent: "claude" as const,
      model: "claude-sonnet-4-5",
      effort: "high",
    },
  },
];

describe("RunsTab", () => {
  let container: HTMLDivElement;
  let root: ReturnType<typeof createRoot>;
  let execute: (action: () => Promise<unknown>) => Promise<unknown>;

  beforeEach(() => {
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    actionMocks.prepareGrillRun.mockReturnValue(async () => preview);
    actionMocks.composeGrillPrompt.mockReturnValue(
      async () => "Composed Grill prompt",
    );
    actionMocks.startGrillRun.mockReturnValue(async () => ({ id: 2 }));

    execute = vi.fn(async (action: () => Promise<unknown>) => action());
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
  });

  afterEach(async () => {
    await act(async () => root.unmount());
    container.remove();
    vi.unstubAllGlobals();
    vi.clearAllMocks();
  });

  it("defaults to Grill and starts with the edited composed prompt", async () => {
    const commands = {
      workCommand: { execute },
      isSaving: false,
      saveItem: (action: () => Promise<unknown>) => execute(action),
      whileSaving: (task: () => Promise<unknown>) => task(),
      confirm: vi.fn(),
      confirmationDialog: null,
      onChanged: async () => {},
    } as unknown as ItemCommands;

    await act(async () => {
      root.render(
        createElement(RunsTab, {
          view,
          repositories: [
            {
              id: 1,
              project_id: 1,
              name: "app",
              remote_url: "https://example.com/app.git",
              base_branch: "main",
            },
          ],
          machines: [],
          contexts,
          grillModelCatalog: [
            {
              agent: "claude",
              models: [
                {
                  id: "claude-sonnet-4-5",
                  label: "Sonnet",
                  efforts: [{ id: "high", label: "High" }],
                },
              ],
            },
          ],
          commands,
          intent: undefined,
          grillDrafts: {},
          onGrillDraftsChange: vi.fn(),
          onOpenTerminal: vi.fn(),
        }),
      );
    });

    await act(async () => {
      [...container.querySelectorAll("button")]
        .find((button) => button.textContent?.trim() === "Start Run")
        ?.click();
    });

    const profile = container.querySelector("select");
    expect(profile?.value).toBe("grill");
    expect([...profile!.options].map((option) => option.textContent)).toContain(
      "Implement",
    );
    expect([...profile!.options].map((option) => option.textContent)).toContain(
      "Grill",
    );

    const initialPrompt = [...container.querySelectorAll("textarea")].find(
      (textarea) => textarea.placeholder.startsWith("What decision"),
    );
    expect(initialPrompt).toBeDefined();
    await act(async () => {
      setTextareaValue(initialPrompt!, "Stress-test the launch plan");
    });

    await act(async () => {
      [...container.querySelectorAll("button")]
        .find(
          (button) => button.textContent?.trim() === "Preview composed prompt",
        )
        ?.click();
    });

    const composedPrompt = [...container.querySelectorAll("textarea")].find(
      (textarea) => textarea.value === "Composed Grill prompt",
    );
    expect(composedPrompt).toBeDefined();
    await act(async () => {
      setTextareaValue(composedPrompt!, "Edited composed Grill prompt");
    });

    await act(async () => {
      container
        .querySelector("form")
        ?.dispatchEvent(
          new Event("submit", { bubbles: true, cancelable: true }),
        );
    });

    expect(workActions.startGrillRun).toHaveBeenCalledWith(
      expect.objectContaining({ prompt: "Edited composed Grill prompt" }),
    );
  });
});

function setTextareaValue(textarea: HTMLTextAreaElement, value: string) {
  const setter = Object.getOwnPropertyDescriptor(
    HTMLTextAreaElement.prototype,
    "value",
  )?.set;
  setter?.call(textarea, value);
  textarea.dispatchEvent(new Event("input", { bubbles: true }));
  textarea.dispatchEvent(new Event("change", { bubbles: true }));
}
