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
  command: vi.fn(),
  shouldBlock: undefined as (() => boolean) | undefined,
  confirm: vi.fn(),
}));

vi.mock("@tanstack/react-query", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tanstack/react-query")>();
  const React = await import("react");
  return {
    ...actual,
    useQueryClient: () => ({
      refetchQueries: vi.fn(async () => {}),
      invalidateQueries: vi.fn(async () => {}),
      fetchQuery: vi.fn(async (options: { queryFn: () => unknown }) => options.queryFn()),
    }),
    useMutation: (options: { mutationFn: (value: unknown) => Promise<unknown>; onSuccess?: (data: unknown, value: unknown) => Promise<void> }) => ({
      isPending: false,
      mutateAsync: async (value: unknown) => {
        const data = await options.mutationFn(value);
        await options.onSuccess?.(data, value);
        return data;
      },
    }),
    useQuery: (options: { queryKey: readonly unknown[]; queryFn: () => unknown }) => {
      const [data, setData] = React.useState<unknown>();
      const key = JSON.stringify(options.queryKey);
      React.useEffect(() => {
        let active = true;
        Promise.resolve(options.queryFn()).then((value) => {
          if (active) setData(value);
        });
        return () => { active = false; };
      }, [key]);
      return { data, isPending: data === undefined, isFetching: false, error: null };
    },
  };
});

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
vi.mock("../../runtime/query-invalidation", () => ({
  invalidateStructureQueries: vi.fn(async () => {}),
}));
vi.mock("../../runtime/command", () => ({ command: mocks.command }));

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
    mocks.command.mockReset().mockImplementation(async (name: string) => {
      if (name === "listContexts") return mocks.data.contexts;
      if (name === "listProjects") return mocks.data.projects;
      if (name === "listRepositories") return mocks.data.repositories;
      if (name === "listRepositoryLocations") return mocks.data.repositoryLocations;
      if (name === "listMachines") return mocks.data.machines;
      if (name === "listCliConfigurationProfiles") return mocks.data.cliConfigurationProfiles;
      if (name === "listContextAttentionDefaults") return mocks.data.attentionDefaults;
      if (name === "listGrillModelCatalog") return { catalogs: mocks.data.grillModelCatalog, codexStatus: "available", codexError: null };
      if (name === "getHome") return { attention_entries: [], needs_attention: [], running: [], waiting: [], due: [], completed: [] };
      if (name === "listRunSuggestions") return [];
      if (name === "getSetupState") return { completed: false, provider: "none" };
      if (name === "getHealthStatus") return {
        runtime: { key: "runtime", label: "Runtime", state: "available", executablePath: null, message: "", action: null },
        provider: { key: "provider", label: "Provider", state: "available", executablePath: null, message: "", action: null },
        agents: [],
        checkedAt: 0,
      };
      if (name === "newContextConfiguration") {
        const defaults = {
          agent: "claude",
          model: "claude-sonnet-4-5",
          effort: "high",
        };
        return {
          name: "",
          executionMachineId: null,
          claudeProfileId: null,
          codexProfileId: null,
          checkDirtyCheckouts: true,
          grillDefaults: defaults,
          implementDefaults: defaults,
          defaultWorkflow: "matt-pocock",
          pstackDefaults: defaults,
          pstackRoles: [],
          ghExecutablePath: null,
          twgExecutablePath: null,
          azExecutablePath: null,
          atlassianSite: null,
          azureDevopsOrganization: null,
          bitbucketWorkspace: null,
          attentionDefaults: [],
        };
      }
      if (name === "createContextConfiguration") {
        const context = { id: 1, name: "Research" };
        mocks.data.contexts.push(context);
        return context;
      }
      return undefined;
    });
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
    await act(async () => {});

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

    expect(mocks.command).toHaveBeenCalledWith(
      "createContextConfiguration",
      expect.objectContaining({ name: "Research" }),
    );
    const selectedContext = Array.from(
      container.querySelectorAll('[data-selected="true"]'),
    ).find((row) => row.textContent?.includes("Research"));
    expect(selectedContext).toBeDefined();
    expect(container.textContent).not.toContain("Create a Context with its settings together.");
  });

  it("asks before discarding a dirty create draft when leaving Settings", async () => {
    act(() => root.render(createElement(StructurePage, { section: "contexts" })));
    const addContext = Array.from(container.querySelectorAll("button")).find(
      (button) => button.textContent === "Add Context",
    );
    act(() => addContext?.dispatchEvent(new MouseEvent("click", { bubbles: true })));
    await act(async () => {});
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
