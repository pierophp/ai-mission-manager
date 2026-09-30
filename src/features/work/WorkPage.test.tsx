// @vitest-environment happy-dom
import { act, createElement } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { WorkPage } from "./WorkPage";

const mocks = vi.hoisted(() => ({
  search: {} as Record<string, unknown>,
  navigate: vi.fn(),
  command: vi.fn(),
}));

vi.mock("@tanstack/react-router", () => ({
  useSearch: () => mocks.search,
  useNavigate: () => mocks.navigate,
}));
vi.mock("@tanstack/react-query", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@tanstack/react-query")>();
  const React = await import("react");
  return {
    ...actual,
    useQueryClient: () => ({ invalidateQueries: vi.fn(async () => {}) }),
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
vi.mock("../../components/app-shell", () => ({
  useAppShell: () => ({ openTerminal: vi.fn() }),
}));
vi.mock("../../runtime/RuntimeEventsBridge", () => ({
  usePollExternalObjects: () => ({ mutateAsync: vi.fn(), isPending: false }),
}));
vi.mock("../../runtime/command", () => ({ command: mocks.command }));
vi.mock("../../runtime/query-invalidation", () => ({
  invalidateStructureQueries: vi.fn(async () => {}),
  invalidateWorkQueries: vi.fn(async () => {}),
}));

const emptyHome = {
  attention_entries: [],
  needs_attention: [],
  running: [],
  waiting: [],
  due: [],
  completed: [],
};

describe("WorkPage item creation", () => {
  let container: HTMLDivElement;
  let root: ReturnType<typeof createRoot>;

  beforeEach(() => {
    vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
    mocks.search = {};
    mocks.navigate.mockReset();
    mocks.command.mockReset().mockImplementation(async (name: string) => {
      if (name === "getHome") return emptyHome;
      if (name === "searchItemsCommand") return [];
      if (name === "listRunSuggestions") return [];
      if (name === "listContexts") return [{ id: 1, name: "Product" }];
      if (name === "listProjects") return [{ id: 2, context_id: 1, name: "App" }];
      if (name === "listRepositories" || name === "listRepositoryLocations" || name === "listMachines" || name === "listCliConfigurationProfiles" || name === "listContextAttentionDefaults") return [];
      if (name === "listGrillModelCatalog") return { catalogs: [], codexStatus: "available", codexError: null };
      if (name === "createItem") {
        return {
          id: 42,
          human_identifier: "APP-42",
          title: "Investigate invoice import",
          project_id: 2,
          status: "Active",
          notes: "",
          reminders: [],
        };
      }
      return undefined;
    });
    container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
    act(() => root.render(createElement(WorkPage)));
  });

  afterEach(() => {
    act(() => root.unmount());
    container.remove();
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("opens the newly created Item", async () => {
    act(() => {
      Array.from(container.querySelectorAll("button"))
        .find((button) => button.textContent?.includes("Add Item"))
        ?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });

    const titleInput = Array.from(document.querySelectorAll("label"))
      .find((label) => label.textContent?.includes("Title"))
      ?.querySelector<HTMLInputElement>("input");
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")
      ?.set?.call(titleInput, "Investigate invoice import");
    act(() => titleInput?.dispatchEvent(new Event("input", { bubbles: true })));

    await act(async () => {
      document
        .querySelector("form")
        ?.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true }));
    });

    expect(mocks.command).toHaveBeenCalledWith(
      "createItem",
      "Investigate invoice import",
      1,
      2,
      "",
    );
    const selectedItemNavigation = mocks.navigate.mock.calls
      .map(([options]) => options as { search?: unknown })
      .find((options) =>
        typeof options.search === "function" &&
        (options.search as (current: Record<string, unknown>) => Record<string, unknown>)(
          {},
        ).item === 42,
      );
    expect(selectedItemNavigation).toBeDefined();
  });
});
