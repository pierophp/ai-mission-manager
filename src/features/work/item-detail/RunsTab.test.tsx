// @vitest-environment happy-dom

import { act, createElement } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ItemCommands } from "../use-item-commands";
import { RunsTab } from "./RunsTab";
import type {
  ItemView as GeneratedItemView,
  Run as GeneratedRun,
} from "../../../runtime/bindings";
import type { Context, ItemView, Machine, Run, RunLaunchOptions } from "../../../runtime/types";
import type { ExecutionProfile } from "../../../runtime/execution-types";

const commandMock = vi.hoisted(() => ({ command: vi.fn() }));

vi.mock("../../../runtime/command", () => ({ command: commandMock.command }));

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

const generatedView: GeneratedItemView = {
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
  run_projections: [],
  run_signals: { grillWaiting: false, runActive: false },
  implementation_queues: [],
  links: [],
};
const view: ItemView = {
  ...generatedView,
  links: [],
  runs: generatedView.runs.map(toUiRun),
  worktrees: generatedView.worktrees.map((worktree) => ({
    id: worktree.id,
    workspace_id: worktree.workspaceId,
    repository_id: worktree.repositoryId,
    machine_id: worktree.machineId,
    path: worktree.path,
    branch: worktree.branch,
    base_branch: worktree.baseBranch,
    is_dirty: worktree.isDirty,
  })),
  workspaces: generatedView.workspaces.map((workspace) => ({
    ...workspace,
    repositories: workspace.repositories.map((repository) => ({
      repository_id: repository.repositoryId,
      branch: repository.branch,
      base_branch: repository.baseBranch,
    })),
  })),
  implementation_queues: (generatedView.implementation_queues ?? []).map(
    (queue) => ({
      ...queue,
      pausedReason: queue.pausedReason ?? null,
      entries: queue.entries.map((entry) => ({
        ...entry,
        done: required(entry.done, "implementation queue done state"),
      })),
    }),
  ),
};

function required<T>(value: T | undefined, label: string): T {
  if (value === undefined) throw new Error(`Generated fixture omits ${label}`);
  return value;
}

function toUiRun(run: GeneratedRun): Run {
  return {
    ...run,
    cli_configuration_profile: run.cli_configuration_profile ?? null,
    workflow: required(run.workflow, "workflow"),
    model: run.model ?? null,
    effort: run.effort ?? null,
    transcript: run.transcript ?? "",
    reported_pull_requests: run.reported_pull_requests ?? [],
    attention_summary: run.attention_summary ?? null,
    grill_question_group: run.grill_question_group
      ? {
          ...run.grill_question_group,
          round: required(run.grill_question_group.round, "grill round"),
        }
      : null,
    grill_answers: run.grill_answers ?? [],
    grill_decisions: run.grill_decisions ?? [],
    grill_response: run.grill_response ?? null,
    grill_phase: run.grill_phase ?? null,
    grill_action: run.grill_action ?? null,
    plan_phase: run.plan_phase ?? null,
    plan_path: run.plan_path ?? null,
  };
}

const mattPocockProfiles: ExecutionProfile[] = [
  "grill",
  "investigate",
  "implement",
  "review",
  "custom",
];
const pstackProfiles: ExecutionProfile[] = [
  "autonomous",
  "plan",
  "pstack-review",
  "custom",
];

const contexts: Context[] = [
  {
    id: 1,
    name: "Default",
    execution_machine_id: null,
    claude_profile_id: null,
    codex_profile_id: null,
    check_dirty_checkouts: false,
    grill_defaults: {
      agent: "claude",
      model: "claude-sonnet-4-5",
      effort: "high",
    },
    implement_defaults: {
      agent: "claude",
      model: "claude-sonnet-4-5",
      effort: "high",
    },
    default_workflow: "matt-pocock",
    pstack_defaults: {
      agent: "claude",
      model: "claude-sonnet-4-5",
      effort: "high",
    },
    pstack_roles: [
      { role: "code-delegate", configuration: { agent: "claude", model: "claude-opus-5", effort: "high" } },
      { role: "judge-and-prose", configuration: { agent: "codex", model: "gpt-6-sol", effort: "high" } },
      { role: "review-panel", configuration: { agent: "codex", model: "gpt-6-sol", effort: "high" } },
      { role: "explorers", configuration: { agent: "claude", model: "claude-sonnet-5", effort: "medium" } },
    ],
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

  beforeEach(() => {
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    const launchOptions: RunLaunchOptions = {
      defaultWorkflow: "matt-pocock",
      workflows: [
        {
          workflow: "matt-pocock",
          defaultProfile: "grill",
          profiles: (mattPocockProfiles).map(
            (executionProfile) => ({
              executionProfile,
              configuration: contexts[0].grill_defaults,
              requiresInitialPrompt: executionProfile === "custom",
            }),
          ),
        },
        {
          workflow: "pstack",
          defaultProfile: "autonomous",
          profiles: pstackProfiles.map(
            (executionProfile) => ({
              executionProfile,
              configuration: required(contexts[0].pstack_defaults, "pstack defaults"),
              requiresInitialPrompt: executionProfile === "custom",
            }),
          ),
        },
      ],
    };
    commandMock.command.mockReset().mockImplementation(async (name: string) => {
      if (name === "getRunLaunchOptions") return launchOptions;
      if (name === "prepareDirectRun") return preview;
      if (name === "composeGrillPrompt") return "Composed Grill prompt";
      if (name === "composeRunPrompt") return "Composed Custom prompt";
      if (name === "startRun") return { id: 2 };
      if (name === "goPlan") return { id: 1 };
      return undefined;
    });

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
    const commands = itemCommands();

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

    expect(commandMock.command).toHaveBeenCalledWith(
      "composeGrillPrompt",
      1,
      { agent: "claude", model: "claude-sonnet-4-5", effort: "high" },
      "portuguese",
      "Start from the SSH incident.",
    );
    expect(commandMock.command).toHaveBeenCalledWith(
      "startRun",
      expect.objectContaining({
        strategy: expect.objectContaining({ kind: "grill", prompt: "Composed Grill prompt" }),
      }),
    );
  });

  it("renders a pstack Run as Needs input with its terminal and no Grill controls", async () => {
    const pstackRun: GeneratedRun = {
      id: 42,
      item_id: 1,
      workspace_id: 1,
      repository_id: 1,
      worktree_id: null,
      machine_id: 1,
      agent: "claude",
      cli_configuration_profile: null,
      execution_profile: "custom",
      workflow: "pstack",
      model: "claude-sonnet-4-5",
      effort: "high",
      skill_snapshot: null,
      prompt: "Handle the request",
      working_directory: "/tmp/app",
      session_name: "run-42",
      pane_id: "%42",
      started_at: 1,
      state: "blocked",
      pane_status: "available",
      direct_checkouts: [],
      transcript: "",
      reported_pull_requests: ["https://github.com/acme/service/pull/7"],
      attention_summary: "Review the migration rollback path.",
      grill_question_group: null,
      grill_answers: [],
      grill_decisions: [],
      grill_response: null,
      grill_phase: null,
      grill_action: null,
      plan_phase: null,
      plan_path: null,
    };
    const runView: ItemView = {
      ...view,
      runs: [toUiRun(pstackRun)],
      run_projections: [{
        runId: 42,
        status: "active",
        phase: "blocked",
        continuations: { goPlan: false, grillActions: [], stop: true, finish: true, delete: false },
      }],
    };
    const commands = itemCommands();

    await act(async () => {
      root.render(
        createElement(RunsTab, {
          view: runView,
          repositories: [],
          machines: [],
          grillModelCatalog: [],
          commands,
          intent: undefined,
          grillDrafts: {},
          onGrillDraftsChange: vi.fn(),
          onOpenTerminal: vi.fn(),
        }),
      );
    });

    expect(container.textContent).toContain("Needs input");
    expect(container.textContent).toContain("Open embedded terminal");
    expect(container.textContent).toContain("Pull Requests reported by this Run");
    expect(container.querySelector('a[href="https://github.com/acme/service/pull/7"]')).not.toBeNull();
    expect(container.textContent).toContain("Review the migration rollback path.");
    expect(container.querySelector('[aria-label="Grill questions for Run #42"]')).toBeNull();
    expect(container.textContent).not.toContain("Skip to");
    expect(container.textContent).not.toContain("choose the next action");
  });

  it("shows the plan link and Go only while a Plan Run awaits Go", async () => {
    const planRun: GeneratedRun = {
      id: 7, item_id: 1, workspace_id: 1, repository_id: 1, worktree_id: null,
      machine_id: 1, agent: "claude", cli_configuration_profile: null,
      execution_profile: "plan", workflow: "pstack", model: null, effort: null,
      skill_snapshot: null, prompt: "Plan prompt", working_directory: "/tmp/app",
      session_name: "plan-7", pane_id: "%7", started_at: 1, state: "finished",
      pane_status: "available", direct_checkouts: [], transcript: "",
      reported_pull_requests: [], attention_summary: null, grill_question_group: null,
      grill_answers: [], grill_decisions: [], grill_response: null, grill_phase: null,
      grill_action: null, plan_phase: "awaitingGo", plan_path: "docs/plan.md",
    };
    const commands = itemCommands();
    const localMachine: Machine = {
      id: 1,
      context_id: 1,
      name: "Laptop",
      socket_name: "mm",
      transport: { kind: "local" },
      last_observed: "available",
      last_observed_at: null,
    };
    const renderRun = async (run: Run, machines: Machine[] = [localMachine]) => act(async () => {
      const awaitingGo = run.plan_phase === "awaitingGo";
      root.render(createElement(RunsTab, {
        view: {
          ...view,
          runs: [run],
          run_projections: [{
            runId: run.id,
            status: awaitingGo ? "active" : "active",
            phase: awaitingGo ? "awaitingGo" : "working",
            continuations: {
              goPlan: awaitingGo,
              grillActions: [],
              stop: true,
              finish: true,
              delete: false,
            },
          }],
        } satisfies ItemView,
        repositories: [], machines, grillModelCatalog: [], commands,
        intent: undefined, grillDrafts: {}, onGrillDraftsChange: vi.fn(), onOpenTerminal: vi.fn(),
      }));
    });

    await renderRun(toUiRun(planRun));
    expect(container.querySelector('[aria-label="Plan ready for Run #7"]')).not.toBeNull();
    expect(buttonNamed("docs/plan.md")?.title).toBe("/tmp/app/docs/plan.md");
    expect(buttonNamed("Go")).not.toBeNull();

    await renderRun(toUiRun(planRun), [{
      ...localMachine,
      transport: {
        kind: "ssh",
        host: "example.test",
        user: null,
        port: null,
        identityFile: null,
        knownHostsFile: null,
        strictHostKeyChecking: null,
      },
    }]);
    expect(buttonNamed("docs/plan.md")).toBeUndefined();
    expect(container.querySelector('[aria-label="Plan ready for Run #7"] code')?.textContent).toBe("/tmp/app/docs/plan.md");

    await renderRun({ ...toUiRun(planRun), state: "working", plan_phase: "executing" });
    expect(container.querySelector('[aria-label="Plan ready for Run #7"]')).toBeNull();
    expect(buttonNamed("Go")).toBeUndefined();

    await renderRun({ ...toUiRun(planRun), plan_phase: "executing" });
    expect(buttonNamed("Go")).toBeUndefined();
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

    expect(commandMock.command).toHaveBeenCalledWith("composeGrillPrompt", expect.anything(), expect.anything(), expect.anything(), expect.anything());
    expect(commandMock.command).toHaveBeenCalledWith(
      "startRun",
      expect.objectContaining({
        strategy: expect.objectContaining({ kind: "grill", prompt: "Edited composed Grill prompt" }),
      }),
    );
  });

  it("lets Custom wait for its prompt instead of failing on selection", async () => {
    await openLaunchForm({ ...view, item: { ...view.item, notes: "" } });

    await act(async () => {
      profileOption("custom").click();
    });

    expect(commandMock.command).not.toHaveBeenCalledWith("composeRunPrompt", expect.anything());
    expect(container.querySelector("form")).not.toBeNull();
    const start = () =>
      container.querySelector<HTMLButtonElement>('button[type="submit"]')!;
    expect(start().disabled).toBe(true);

    await act(async () => {
      setTextareaValue(initialPrompt(), "Rename the module");
    });
    expect(start().disabled).toBe(false);
    await submit();

    expect(commandMock.command).toHaveBeenCalledWith(
      "composeRunPrompt",
      1,
      "custom",
      { includeObjective: true, externalObjectIds: [] },
      "portuguese",
      "Rename the module",
      "matt-pocock",
    );
    expect(commandMock.command).toHaveBeenCalledWith(
      "startRun",
      expect.objectContaining({
        strategy: expect.objectContaining({
          kind: "direct",
          execution_profile: "custom",
          prompt: "Composed Custom prompt",
          configuration: {
          agent: "claude",
          model: "claude-sonnet-4-5",
          effort: "high",
          },
        }),
      }),
    );
  });

  it("switches to the pstack Autonomous profile and offers Plan", async () => {
    await openLaunchForm();
    const offered = () =>
      [...container.querySelectorAll<HTMLInputElement>('input[name="run-execution-profile"]')].map(
        (input) => input.value,
      );
    expect(offered()).toEqual(["grill", "investigate", "implement", "review", "custom"]);
    const workflow = [...container.querySelectorAll("label")]
      .find((label) => label.textContent?.includes("Workflow"))
      ?.querySelector("select") ?? null;
    expect(workflow).not.toBeNull();
    const setter = Object.getOwnPropertyDescriptor(
      HTMLSelectElement.prototype,
      "value",
    )?.set;
    await act(async () => {
      setter?.call(workflow, "pstack");
      workflow?.dispatchEvent(new Event("change", { bubbles: true }));
    });

    expect(profileOption("autonomous").checked).toBe(true);
    expect(offered()).toEqual(["autonomous", "plan", "pstack-review", "custom"]);
    await submit();
    expect(commandMock.command).toHaveBeenCalledWith(
      "startRun",
      expect.objectContaining({
        strategy: expect.objectContaining({
          kind: "direct",
          workflow: "pstack",
          execution_profile: "autonomous",
          configuration: contexts[0].pstack_defaults,
        }),
      }),
    );
  });
});

function itemCommands(): ItemCommands {
  return {
    workCommand: {
      isPending: false,
      execute: async (action) => action(),
    },
    isSaving: false,
    saveItem: async (action) => action(),
    whileSaving: async (task) => task(),
    confirm: () => {},
    confirmationDialog: undefined,
    onChanged: async () => {},
  };
}

function setTextareaValue(textarea: HTMLTextAreaElement, value: string) {
  const setter = Object.getOwnPropertyDescriptor(
    HTMLTextAreaElement.prototype,
    "value",
  )?.set;
  setter?.call(textarea, value);
  textarea.dispatchEvent(new Event("input", { bubbles: true }));
  textarea.dispatchEvent(new Event("change", { bubbles: true }));
}
