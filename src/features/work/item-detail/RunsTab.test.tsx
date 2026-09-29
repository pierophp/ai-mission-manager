// @vitest-environment happy-dom

import { act, createElement } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ItemCommands } from "../use-item-commands";
import { RunsTab } from "./RunsTab";
import type { ItemView } from "../../../runtime/types";
import { workActions } from "../work-mutations";

const actionMocks = vi.hoisted(() => ({
  prepareDirectRun: vi.fn(),
  composeGrillPrompt: vi.fn(),
  composeRunPrompt: vi.fn(),
  startGrillRun: vi.fn(),
  startDirectRun: vi.fn(),
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
    notes: "Start from the SSH incident.",
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
    gh_executable_path: null,
    twg_executable_path: null,
    az_executable_path: null,
    atlassian_site: null,
    azure_devops_organization: null,
    bitbucket_workspace: null,
  },
];

describe("RunsTab", () => {
  let container: HTMLDivElement;
  let root: ReturnType<typeof createRoot>;
  let execute: (action: () => Promise<unknown>) => Promise<unknown>;

  beforeEach(() => {
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    actionMocks.prepareDirectRun.mockReturnValue(async () => preview);
    actionMocks.composeGrillPrompt.mockReturnValue(
      async () => "Composed Grill prompt",
    );
    actionMocks.composeRunPrompt.mockReturnValue(
      async () => "Composed Custom prompt",
    );
    actionMocks.startGrillRun.mockReturnValue(async () => ({ id: 2 }));
    actionMocks.startDirectRun.mockReturnValue(async () => ({ id: 3 }));

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

  async function openLaunchForm(itemView: ItemView = view) {
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
          view: itemView,
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
      buttonNamed("Start Run")?.click();
    });
  }

  function buttonNamed(name: string) {
    return [...container.querySelectorAll("button")].find(
      (button) => button.textContent?.trim().startsWith(name),
    );
  }

  function initialPrompt() {
    return container.querySelector<HTMLTextAreaElement>("textarea")!;
  }

  function profileOption(value: string) {
    return container.querySelector<HTMLInputElement>(
      `input[name="run-execution-profile"][value="${value}"]`,
    )!;
  }

  async function submit() {
    await act(async () => {
      container
        .querySelector("form")
        ?.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    });
  }

  it("defaults to Grill with the Item's Notes and keeps the composed prompt folded", async () => {
    await openLaunchForm();

    expect(profileOption("grill").checked).toBe(true);
    expect(initialPrompt().value).toBe("Start from the SSH incident.");
    expect(container.querySelector('textarea[aria-label="Composed prompt"]')).toBeNull();

    await submit();

    expect(workActions.composeGrillPrompt).toHaveBeenCalledWith(
      1,
      { agent: "claude", model: "claude-sonnet-4-5", effort: "high" },
      "portuguese",
      "Start from the SSH incident.",
    );
    expect(workActions.startGrillRun).toHaveBeenCalledWith(
      expect.objectContaining({ prompt: "Composed Grill prompt" }),
    );
  });

  it("starts with the prompt edited in the preview", async () => {
    await openLaunchForm();

    await act(async () => {
      buttonNamed("Preview prompt")?.click();
    });
    const composedPrompt = container.querySelector<HTMLTextAreaElement>(
      'textarea[aria-label="Composed prompt"]',
    );
    expect(composedPrompt?.value).toBe("Composed Grill prompt");
    await act(async () => {
      setTextareaValue(composedPrompt!, "Edited composed Grill prompt");
    });
    await submit();

    expect(workActions.composeGrillPrompt).toHaveBeenCalledTimes(1);
    expect(workActions.startGrillRun).toHaveBeenCalledWith(
      expect.objectContaining({ prompt: "Edited composed Grill prompt" }),
    );
  });

  it("lets Custom wait for its prompt instead of failing on selection", async () => {
    await openLaunchForm({ ...view, item: { ...view.item, notes: "" } });

    await act(async () => {
      profileOption("custom").click();
    });

    expect(workActions.composeRunPrompt).not.toHaveBeenCalled();
    expect(container.querySelector("form")).not.toBeNull();
    const start = () =>
      container.querySelector<HTMLButtonElement>('button[type="submit"]')!;
    expect(start().disabled).toBe(true);

    await act(async () => {
      setTextareaValue(initialPrompt(), "Rename the module");
    });
    expect(start().disabled).toBe(false);
    await submit();

    expect(workActions.composeRunPrompt).toHaveBeenCalledWith(
      1,
      "custom",
      { includeObjective: true, externalObjectIds: [] },
      "portuguese",
      "Rename the module",
    );
    expect(workActions.startDirectRun).toHaveBeenCalledWith(
      expect.objectContaining({
        executionProfile: "custom",
        prompt: "Composed Custom prompt",
        configuration: {
          agent: "claude",
          model: "claude-sonnet-4-5",
          effort: "high",
        },
      }),
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
