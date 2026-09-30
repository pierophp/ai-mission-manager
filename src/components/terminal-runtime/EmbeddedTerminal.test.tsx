import { Window } from "happy-dom";
import { StrictMode, act, createElement } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

const terminalAdapter = vi.hoisted(() => ({
  open: vi.fn(async (_runId: number, terminalId: string) => ({
    terminalId,
    generation: 1,
    sessionName: "session",
    paneId: "%1",
    snapshot: [],
    panes: [
      {
        sessionName: "session",
        paneId: "%1",
        runId: 1,
        label: "Agent",
        paneIndex: 0,
        pid: 1,
        columns: 80,
        rows: 24,
        title: "Agent",
        currentCommand: "codex",
        currentPath: "/repo",
        available: true,
      },
    ],
  })),
  listen: vi.fn(async () => () => {}),
  close: vi.fn(async () => {}),
  input: vi.fn(async () => {}),
  resize: vi.fn(async () => {}),
}));

vi.mock("../../runtime/adapters", () => ({
  terminalRuntimeAdapter: terminalAdapter,
}));

vi.mock("@xterm/xterm", () => ({
  Terminal: class {
    cols = 80;
    rows = 24;
    loadAddon() {}
    open() {}
    onData() {
      return { dispose() {} };
    }
    reset() {}
    write() {}
    dispose() {}
  },
}));

vi.mock("@xterm/addon-fit", () => ({
  FitAddon: class {
    fit() {}
  },
}));

const dom = new Window();
Object.assign(globalThis, {
  window: dom,
  document: dom.document,
  getComputedStyle: dom.getComputedStyle.bind(dom),
  ResizeObserver: class {
    observe() {}
    disconnect() {}
  },
});
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });

const { EmbeddedTerminal } = await import("./EmbeddedTerminal");

afterEach(() => {
  terminalAdapter.open.mockClear();
  terminalAdapter.listen.mockClear();
  terminalAdapter.close.mockClear();
});

describe("EmbeddedTerminal", () => {
  it("uses a separate connection identity for each Strict Mode attachment", async () => {
    const container = document.createElement("div");
    document.body.append(container);
    const root = createRoot(container);

    await act(async () => {
      root.render(
        createElement(
          StrictMode,
          null,
          createElement(EmbeddedTerminal, {
            runId: 1,
            initialPane: {
              sessionName: "session",
              paneId: "%1",
              runId: 1,
              label: "Agent",
              paneIndex: 0,
              pid: 1,
              columns: 80,
              rows: 24,
              title: "Agent",
              currentCommand: "codex",
              currentPath: "/repo",
              available: true,
            },
            onClose() {},
          }),
        ),
      );
      await Promise.resolve();
      await Promise.resolve();
      await Promise.resolve();
    });

    const openedIds = terminalAdapter.open.mock.calls.map((call) => call[1]);
    expect(openedIds.length).toBeGreaterThan(1);
    expect(new Set(openedIds).size).toBe(openedIds.length);

    await act(async () => root.unmount());
    container.remove();
  });
});
