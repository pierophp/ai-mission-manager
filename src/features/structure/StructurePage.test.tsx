// @vitest-environment happy-dom
import { act, createElement } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { StructurePage } from "./StructurePage";

const mocks = vi.hoisted(() => ({
  data: {
    contexts: [] as Array<{ id: number; name: string }>,
    projects: [] as unknown[],
    repositories: [] as unknown[],
    repositoryLocations: [] as unknown[],
    machines: [] as unknown[],
    cliConfigurationProfiles: [] as unknown[],
    attentionDefaults: [] as unknown[],
    grillModelCatalog: [
      {
        agent: "claude",
        models: [
          {
            id: "claude-sonnet-4-5",
            label: "Sonnet 4.5",
            efforts: [{ id: "high", label: "High" }],
          },
        ],
      },
    ],
  },
  createAction: vi.fn(),
  execute: vi.fn(),
  shouldBlock: undefined as (() => boolean) | undefined,
  confirm: vi.fn(),
}));

vi.mock("@tanstack/react-query", () => ({
  useQueryClient: () => ({
    refetchQueries: vi.fn(async () => {}),
    invalidateQueries: vi.fn(async () => {}),
    fetchQuery: vi.fn(async () => undefined),
  }),
}));

vi.mock("@tanstack/react-router", () => ({
  Link: ({ children }: { children: React.ReactNode }) => children,
  useBlocker: (options: { shouldBlockFn: () => boolean }) => {
    mocks.shouldBlock = options.shouldBlockFn;
  },
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("../../components/app-shell", () => ({
  useAppShell: () => ({
    closeTerminal: vi.fn(),
    themePreference: "system",
    setThemePreference: vi.fn(),
  }),
}));
vi.mock("./structure-queries", () => ({
  structureKeys: { all: ["structure"] },
  structureQueryOptions: {},
  useStructureData: () => ({ data: mocks.data }),
}));
vi.mock("../work/work-queries", () => ({
  activityQueryOptions: vi.fn(),
  homeQueryOptions: vi.fn(),
  runSuggestionsQueryOptions: vi.fn(),
  useHomeQuery: () => ({ data: undefined }),
}));
vi.mock("../setup/setup-queries", () => ({
  healthStatusQueryOptions: vi.fn(),
  setupStateQueryOptions: vi.fn(),
}));
vi.mock("../../runtime/query-invalidation", () => ({
  invalidateStructureQueries: vi.fn(async () => {}),
}));
vi.mock("./structure-mutations", () => ({
  structureActions: new Proxy(
    {},
    {
      get: (_target, property) => {
        if (property === "createContextConfiguration") {
          return (configuration: unknown) => {
            mocks.createAction(configuration);
            return async () => {
              const context = { id: 1, name: "Research" };
              mocks.data.contexts.push(context);
              return context;
            };
          };
        }
        return () => async () => undefined;
      },
    },
  ),
  useStructureCommand: () => ({
    execute: (action: () => Promise<unknown>) => action(),
  }),
}));

describe("StructurePage Context creation", () => {
  let container: HTMLDivElement;
  let root: ReturnType<typeof createRoot>;

  beforeEach(() => {
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    Object.defineProperty(window, "confirm", {
      configurable: true,
      value: mocks.confirm,
    });
    mocks.data.contexts = [];
    mocks.createAction.mockClear();
    mocks.confirm.mockReset();
    mocks.shouldBlock = undefined;
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
  });

  afterEach(() => {
    if (root) act(() => root.unmount());
    container.remove();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    delete (window as unknown as { confirm?: typeof window.confirm }).confirm;
  });

  it("opens the shared editor inline and selects the created Context", async () => {
    act(() => root.render(createElement(StructurePage, { section: "contexts" })));
    const addContext = Array.from(container.querySelectorAll("button")).find(
      (button) => button.textContent === "Add Context",
    );
    act(() => addContext?.dispatchEvent(new MouseEvent("click", { bubbles: true })));

    expect(container.textContent).toContain("Primary settings");
    expect(container.textContent).toContain("Needs Attention");
    expect(container.querySelector('[role="dialog"]')).toBeNull();

    const nameInput = Array.from(container.querySelectorAll("label"))
      .find((label) => label.textContent?.includes("Context name"))
      ?.querySelector<HTMLInputElement>("input");
    expect(nameInput).toBeDefined();
    Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
      "value",
    )?.set?.call(nameInput, "Research");
    act(() => nameInput?.dispatchEvent(new Event("input", { bubbles: true })));
    expect(nameInput?.value).toBe("Research");
    await act(async () => {
      container
        .querySelector("form")
        ?.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    });

    expect(mocks.createAction).toHaveBeenCalledWith(
      expect.objectContaining({ name: "Research" }),
    );
    const selectedContext = Array.from(
      container.querySelectorAll('[data-selected="true"]'),
    ).find((row) => row.textContent?.includes("Research"));
    expect(selectedContext).toBeDefined();
    expect(container.textContent).not.toContain("Create a Context with its settings together.");
  });

  it("asks before discarding a dirty create draft when leaving Settings", () => {
    act(() => root.render(createElement(StructurePage, { section: "contexts" })));
    const addContext = Array.from(container.querySelectorAll("button")).find(
      (button) => button.textContent === "Add Context",
    );
    act(() => addContext?.dispatchEvent(new MouseEvent("click", { bubbles: true })));
    const nameInput = Array.from(container.querySelectorAll("label"))
      .find((label) => label.textContent?.includes("Context name"))
      ?.querySelector<HTMLInputElement>("input");
    Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
      "value",
    )?.set?.call(nameInput, "Draft");
    act(() => nameInput?.dispatchEvent(new Event("input", { bubbles: true })));

    mocks.confirm.mockReturnValue(false);
    let shouldBlock = false;
    act(() => {
      shouldBlock = mocks.shouldBlock?.() ?? false;
    });

    expect(shouldBlock).toBe(true);
    expect(mocks.confirm).toHaveBeenCalledWith("Discard unsaved Context changes?");
    expect(container.textContent).toContain("Create Context");
  });
});
