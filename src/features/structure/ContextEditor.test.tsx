// @vitest-environment happy-dom
import { act, createElement } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ContextEditor } from "./ContextEditor";

describe("ContextEditor", () => {
  let container: HTMLDivElement;
  let root: ReturnType<typeof createRoot>;

  beforeEach(() => {
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
  });

  afterEach(() => {
    act(() => root.unmount());
    container.remove();
    vi.unstubAllGlobals();
  });

  it("preloads settings, shows Attention policies, and keeps the draft after a failed save", async () => {
    act(() =>
      root.render(
        createElement(ContextEditor, {
          context: {
            id: 4,
            name: "Research",
            execution_machine_id: 7,
            claude_profile_id: 12,
            codex_profile_id: null,
            check_dirty_checkouts: false,
            grill_defaults: {
              agent: "claude",
              model: "claude-opus-5",
              effort: "medium",
            },
            implement_defaults: {
              agent: "codex",
              model: "gpt-6-sol",
              effort: "high",
            },
            default_workflow: "matt-pocock",
            pstack_defaults: {
              agent: "claude",
              model: "claude-sonnet-5",
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
          attentionDefaults: ["issue", "pull_request", "generic"].map(
            (object_kind) => ({
              context_id: 4,
              object_kind: object_kind as "issue" | "pull_request" | "generic",
              policy: { title: true, state: false, metadata: true },
            }),
          ),
          machines: [
            {
              id: 7,
              context_id: 4,
              name: "Build Mac",
              socket_name: "amm",
              transport: { kind: "local" },
              last_observed: "available",
              last_observed_at: null,
            },
          ],
          profiles: [
            {
              profile: {
                id: 12,
                machineId: 7,
                provider: "claude",
                name: "Work",
                directory: "/profiles/work",
                appManaged: true,
              },
              signInCommand: null,
            },
          ],
          catalog: [
            {
              agent: "claude",
              models: [
                {
                  id: "claude-opus-5",
                  label: "Opus",
                  efforts: [{ id: "medium", label: "Medium" }],
                },
              ],
            },
            {
              agent: "codex",
              models: [
                {
                  id: "gpt-6-sol",
                  label: "GPT-6 Sol",
                  efforts: [{ id: "high", label: "High" }],
                },
              ],
            },
          ],
          isSaving: false,
          onSave: vi.fn(async () => {
            throw new Error("Name conflicts with another Context");
          }),
          onCancel: vi.fn(),
          onDirtyChange: vi.fn(),
        }),
      ),
    );

    expect(container.textContent).toContain("Research");
    expect(container.textContent).toContain("Build Mac");
    expect(container.textContent).toContain("Work");
    expect(container.querySelector<HTMLInputElement>("input")?.value).toBe(
      "Research",
    );
    expect(container.querySelector('[role="tablist"]')?.textContent).toContain(
      "Needs Attention",
    );

    const attentionTab = container.querySelector<HTMLButtonElement>(
      '[role="tab"][data-state="inactive"]',
    );
    expect(attentionTab).not.toBeNull();
    act(() =>
      attentionTab?.dispatchEvent(new MouseEvent("click", { bubbles: true })),
    );

    expect(container.textContent).toContain("Issue");
    expect(container.textContent).toContain("Pull Request");
    expect(container.textContent).toContain("Generic External Object");

    const nameInput = container.querySelector<HTMLInputElement>("input");
    expect(nameInput).not.toBeNull();
    Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
      "value",
    )?.set?.call(nameInput, "Draft name");
    act(() => nameInput?.dispatchEvent(new Event("input", { bubbles: true })));
    const form = container.querySelector("form");
    await act(async () => {
      form?.dispatchEvent(
        new Event("submit", { bubbles: true, cancelable: true }),
      );
    });

    expect(nameInput?.value).toBe("Draft name");
    expect(container.textContent).toContain(
      "Name conflicts with another Context",
    );
  });

  it("starts a new Context with current defaults and saves the full configuration", async () => {
    const onSave = vi.fn(async () => {});
    act(() =>
      root.render(
        createElement(ContextEditor, {
          context: undefined,
          attentionDefaults: [],
          machines: [],
          profiles: [],
          catalog: [],
          isSaving: false,
          onSave,
          onCancel: vi.fn(),
          onDirtyChange: vi.fn(),
        }),
      ),
    );

    expect(container.textContent).toContain("Create Context");
    expect(container.textContent).toContain(
      "Runs cannot start until this Context has an execution Machine.",
    );
    const nameInput = container.querySelector<HTMLInputElement>("input");
    Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
      "value",
    )?.set?.call(nameInput, "Research");
    act(() => nameInput?.dispatchEvent(new Event("input", { bubbles: true })));
    await act(async () => {
      container
        .querySelector("form")
        ?.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    });

    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({
        name: "Research",
        executionMachineId: null,
        claudeProfileId: null,
        codexProfileId: null,
        checkDirtyCheckouts: true,
        grillDefaults: {
          agent: "claude",
          model: "claude-sonnet-5",
          effort: "high",
        },
        implementDefaults: {
          agent: "claude",
          model: "claude-sonnet-5",
          effort: "high",
        },
        attentionDefaults: expect.arrayContaining([
          expect.objectContaining({
            object_kind: "issue",
            policy: { title: true, state: true, metadata: true },
          }),
          expect.objectContaining({
            object_kind: "pull_request",
            policy: { title: true, state: true, metadata: true },
          }),
          expect.objectContaining({
            object_kind: "generic",
            policy: { title: true, state: true, metadata: true },
          }),
        ]),
      }),
    );
  });

  it("saves one complete Context configuration and filters profiles by provider", async () => {
    const onSave = vi.fn(async () => {});
    act(() =>
      root.render(
        createElement(ContextEditor, {
          context: {
            id: 4,
            name: "Research",
            execution_machine_id: 7,
            claude_profile_id: 12,
            codex_profile_id: 13,
            check_dirty_checkouts: false,
            grill_defaults: {
              agent: "claude",
              model: "claude-opus-5",
              effort: "medium",
            },
            implement_defaults: {
              agent: "codex",
              model: "gpt-6-sol",
              effort: "high",
            },
            default_workflow: "matt-pocock",
            pstack_defaults: {
              agent: "claude",
              model: "claude-sonnet-5",
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
          attentionDefaults: ["issue", "pull_request", "generic"].map(
            (object_kind) => ({
              context_id: 4,
              object_kind: object_kind as "issue" | "pull_request" | "generic",
              policy: { title: true, state: false, metadata: true },
            }),
          ),
          machines: [
            {
              id: 7,
              context_id: 4,
              name: "Build Mac",
              socket_name: "amm",
              transport: { kind: "local" },
              last_observed: "available",
              last_observed_at: null,
            },
          ],
          profiles: [
            {
              profile: {
                id: 12,
                machineId: 7,
                provider: "claude",
                name: "Claude Work",
                directory: "/profiles/claude",
                appManaged: true,
              },
              signInCommand: null,
            },
            {
              profile: {
                id: 13,
                machineId: 7,
                provider: "codex",
                name: "Codex Work",
                directory: "/profiles/codex",
                appManaged: true,
              },
              signInCommand: null,
            },
            {
              profile: {
                id: 14,
                machineId: 8,
                provider: "claude",
                name: "Other Machine",
                directory: "/profiles/other",
                appManaged: true,
              },
              signInCommand: null,
            },
          ],
          catalog: [
            {
              agent: "claude",
              models: [
                {
                  id: "claude-opus-5",
                  label: "Opus",
                  efforts: [{ id: "medium", label: "Medium" }],
                },
              ],
            },
            {
              agent: "codex",
              models: [
                {
                  id: "gpt-6-sol",
                  label: "GPT-6 Sol",
                  efforts: [{ id: "high", label: "High" }],
                },
              ],
            },
          ],
          isSaving: false,
          onSave,
          onCancel: vi.fn(),
          onDirtyChange: vi.fn(),
        }),
      ),
    );

    const profileSelect = (provider: string) =>
      Array.from(container.querySelectorAll("label"))
        .find((label) =>
          label.textContent?.includes(
            `${provider} Agent CLI Configuration Profile`,
          ),
        )
        ?.querySelector("select");
    const claudeOptions = Array.from(
      profileSelect("Claude Code")?.options ?? [],
    ).map((option) => option.textContent);
    const codexOptions = Array.from(profileSelect("Codex")?.options ?? []).map(
      (option) => option.textContent,
    );
    expect(claudeOptions).toContain("Claude Work");
    expect(claudeOptions).not.toContain("Codex Work");
    expect(claudeOptions).not.toContain("Other Machine");
    expect(codexOptions).toContain("Codex Work");
    expect(codexOptions).not.toContain("Claude Work");

    await act(async () => {
      container
        .querySelector("form")
        ?.dispatchEvent(
          new Event("submit", { bubbles: true, cancelable: true }),
        );
    });

    expect(onSave).toHaveBeenCalledTimes(1);
    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({
        name: "Research",
        executionMachineId: 7,
        claudeProfileId: 12,
        codexProfileId: 13,
        checkDirtyCheckouts: false,
        grillDefaults: {
          agent: "claude",
          model: "claude-opus-5",
          effort: "medium",
        },
        implementDefaults: {
          agent: "codex",
          model: "gpt-6-sol",
          effort: "high",
        },
        attentionDefaults: expect.arrayContaining([
          expect.objectContaining({
            object_kind: "issue",
            policy: { title: true, state: false, metadata: true },
          }),
          expect.objectContaining({
            object_kind: "pull_request",
            policy: { title: true, state: false, metadata: true },
          }),
          expect.objectContaining({
            object_kind: "generic",
            policy: { title: true, state: false, metadata: true },
          }),
        ]),
      }),
    );
  });
});
