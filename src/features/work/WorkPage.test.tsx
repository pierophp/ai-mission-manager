// @vitest-environment happy-dom
import { act, createElement } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { WorkPage } from "./WorkPage";

const mocks = vi.hoisted(() => ({
  search: {} as Record<string, unknown>,
  navigate: vi.fn(),
  createItem: vi.fn(),
}));

vi.mock("@tanstack/react-router", () => ({
  useSearch: () => mocks.search,
  useNavigate: () => mocks.navigate,
}));
vi.mock("@tanstack/react-query", () => ({
  useQueryClient: () => ({ invalidateQueries: vi.fn(async () => {}) }),
}));
vi.mock("../../components/app-shell", () => ({
  useAppShell: () => ({ openTerminal: vi.fn() }),
}));
vi.mock("../../runtime/RuntimeEventsBridge", () => ({
  usePollExternalObjects: () => ({ mutateAsync: vi.fn(), isPending: false }),
}));
vi.mock("../structure/structure-queries", () => ({
  useStructureData: () => ({
    data: {
      contexts: [{ id: 1, name: "Product" }],
      projects: [{ id: 2, context_id: 1, name: "App" }],
      repositories: [],
      machines: [],
      grillModelCatalog: [],
    },
  }),
}));
vi.mock("./work-queries", () => ({
  useHomeQuery: () => ({ data: emptyHome, isPending: false, isFetching: false }),
  useRunSuggestionsQuery: () => ({ data: [] }),
  useSearchQuery: () => ({ data: [], isFetching: false }),
}));
vi.mock("../structure/structure-mutations", () => ({
  structureActions: {
    createItem: (...args: unknown[]) => {
      mocks.createItem(...args);
      return async () => ({
        id: 42,
        human_identifier: "APP-42",
        title: "Investigate invoice import",
        project_id: 2,
        status: "Active",
        notes: "",
        reminders: [],
      });
    },
  },
  useStructureCommand: () => ({
    isPending: false,
    execute: (action: () => Promise<unknown>) => action(),
  }),
}));
vi.mock("./work-mutations", () => ({
  useWorkCommand: () => ({ isPending: false, execute: vi.fn() }),
  workActions: {},
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
    mocks.createItem.mockReset();
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

    expect(mocks.createItem).toHaveBeenCalledWith(
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
