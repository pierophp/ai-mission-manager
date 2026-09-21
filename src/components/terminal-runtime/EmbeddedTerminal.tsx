import { useEffect, useRef, useState } from "react";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";

import { Alert, AlertDescription } from "../ui/alert";
import { Button } from "../ui/button";
import { errorMessage } from "../../runtime/errors";
import { terminalRuntimeAdapter } from "../../runtime/adapters";
import type {
  PaneTab,
  TerminalExitEvent,
  TerminalOutputEvent,
} from "../../runtime/terminal-types";

type EmbeddedTerminalProps = {
  worksetId: number;
  initialPane: PaneTab;
  onClose: () => void;
};

export function EmbeddedTerminal({
  worksetId,
  initialPane,
  onClose,
}: EmbeddedTerminalProps) {
  const terminalContainerRef = useRef<HTMLDivElement>(null);
  const attachPaneRef = useRef<((pane: PaneTab) => Promise<void>) | undefined>(
    undefined,
  );
  const activePaneRef = useRef(initialPane);
  const attachRequestRef = useRef(0);
  const attachedRef = useRef(false);
  const terminalId = `workset-${worksetId}`;
  const [activePane, setActivePane] = useState(initialPane);
  const [panes, setPanes] = useState<PaneTab[]>([initialPane]);
  const [status, setStatus] = useState("Attaching…");
  const [terminalError, setTerminalError] = useState<string>();

  useEffect(() => {
    const container = terminalContainerRef.current;
    if (!container) return;

    let disposed = false;
    const terminal = new Terminal({
      cursorBlink: true,
      fontFamily: "SFMono-Regular, Menlo, Monaco, Consolas, monospace",
      fontSize: 13,
      theme: {
        background: "#201c19",
        foreground: "#f7f0e6",
        cursor: "#efb28e",
      },
    });
    const fitAddon = new FitAddon();
    terminal.loadAddon(fitAddon);
    terminal.open(container);
    fitAddon.fit();

    const inputDisposable = terminal.onData((data) => {
      if (!attachedRef.current) return;
      void terminalRuntimeAdapter
        .input(terminalId, Array.from(new TextEncoder().encode(data)))
        .catch((inputError) => {
          if (!disposed) setTerminalError(errorMessage(inputError));
        });
    });

    const resizeTerminal = () => {
      fitAddon.fit();
      if (!attachedRef.current || terminal.cols < 1 || terminal.rows < 1) return;
      void terminalRuntimeAdapter
        .resize(terminalId, terminal.cols, terminal.rows)
        .catch((resizeError) => {
          if (!disposed) setTerminalError(errorMessage(resizeError));
        });
    };

    const resizeObserver = new ResizeObserver(resizeTerminal);
    resizeObserver.observe(container);
    const unlisteners: (() => void)[] = [];

    const attachPane = async (pane: PaneTab) => {
      const requestId = ++attachRequestRef.current;
      activePaneRef.current = pane;
      setActivePane(pane);
      attachedRef.current = false;
      setStatus(`Attaching ${pane.label}…`);
      setTerminalError(undefined);

      try {
        const attachment = await terminalRuntimeAdapter.open(
          worksetId,
          terminalId,
          pane.sessionName,
          pane.paneId,
        );
        if (disposed || requestId !== attachRequestRef.current) return;

        setPanes(attachment.panes);
        const attachedPane =
          attachment.panes.find(
            (candidate) => candidate.paneId === attachment.paneId,
          ) ?? pane;
        activePaneRef.current = attachedPane;
        setActivePane(attachedPane);
        terminal.reset();
        terminal.write(Uint8Array.from(attachment.snapshot));
        attachedRef.current = true;
        setStatus("Connected · closing this view leaves the Run running");
        resizeTerminal();
      } catch (attachError) {
        if (disposed || requestId !== attachRequestRef.current) return;
        attachedRef.current = false;
        setStatus("Could not attach");
        setTerminalError(errorMessage(attachError));
      }
    };
    attachPaneRef.current = attachPane;

    const start = async () => {
      unlisteners.push(
        await terminalRuntimeAdapter.listen<TerminalOutputEvent>(
          "terminal-output",
          (event) => {
            const payload = event.payload;
            if (
              payload.terminalId === terminalId &&
              payload.paneId === activePaneRef.current.paneId
            ) {
              terminal.write(Uint8Array.from(payload.data));
            }
          },
        ),
        await terminalRuntimeAdapter.listen<TerminalExitEvent>(
          "terminal-exit",
          (event) => {
            if (
              event.payload.terminalId === terminalId &&
              event.payload.paneId === activePaneRef.current.paneId &&
              !disposed
            ) {
              attachedRef.current = false;
              setStatus("Pane connection closed; the Run was left untouched");
            }
          },
        ),
      );
      await attachPane(initialPane);
    };
    void start().catch((startError) => {
      if (!disposed) {
        attachedRef.current = false;
        setStatus("Could not attach");
        setTerminalError(errorMessage(startError));
      }
    });

    return () => {
      disposed = true;
      attachedRef.current = false;
      attachRequestRef.current += 1;
      attachPaneRef.current = undefined;
      inputDisposable.dispose();
      resizeObserver.disconnect();
      terminal.dispose();
      unlisteners.forEach((unlisten) => unlisten());
      void terminalRuntimeAdapter.close(terminalId).catch(() => undefined);
    };
  }, [initialPane, terminalId, worksetId]);

  return (
    <section
      className="mt-7 grid gap-3.5 rounded-2xl border border-[var(--terminal-border)] bg-[var(--terminal-bg)] p-[18px] shadow-[0_18px_50px_rgb(var(--terminal-shadow-rgb)/0.16)]"
      aria-labelledby="embedded-terminal-heading"
    >
      <div className="flex items-start justify-between gap-4 max-[560px]:flex-col">
        <div>
          <p className="mb-2 text-xs font-bold uppercase tracking-[0.14em] text-[var(--terminal-muted)]">
            Embedded terminal
          </p>
          <h2 id="embedded-terminal-heading" className="font-heading text-xl font-medium text-[var(--terminal-heading)]">
            {activePane.label}
          </h2>
          <p className="mt-1.5 text-xs text-[var(--terminal-muted)]">{status}</p>
        </div>
        <Button type="button" variant="secondary" onClick={onClose}>
          Close view
        </Button>
      </div>
      <div className="flex gap-1.5 overflow-x-auto pb-0.5" role="tablist" aria-label="Panes in this Workset">
        {panes.map((pane) => (
          <button
            type="button"
            role="tab"
            aria-selected={pane.paneId === activePane.paneId}
            className={`grid gap-0.5 rounded-lg border px-2.5 py-2 text-left text-[var(--terminal-tab-text)] disabled:cursor-not-allowed disabled:opacity-50 ${
              pane.paneId === activePane.paneId
                ? "border-[var(--terminal-active-border)] bg-[var(--terminal-active)] text-[var(--terminal-heading)]"
                : "border-[var(--terminal-tab-border)] bg-[var(--terminal-tab)] hover:border-[var(--terminal-active-border)] hover:bg-[var(--terminal-active)] hover:text-[var(--terminal-heading)]"
            }`}
            key={`${pane.sessionName}-${pane.paneId}`}
            disabled={!pane.available}
            onClick={() => void attachPaneRef.current?.(pane)}
          >
            {pane.label}
            <small className="max-w-[190px] overflow-hidden text-ellipsis whitespace-nowrap text-[0.63rem] font-medium text-[var(--terminal-tab-muted)]">
              {pane.currentCommand || pane.currentPath || "unavailable"}
            </small>
          </button>
        ))}
      </div>
      <div
        className="terminal-surface min-h-[420px] overflow-hidden rounded-[10px] border border-[var(--terminal-surface-border)] bg-[var(--terminal-bg)] p-3"
        ref={terminalContainerRef}
      />
      {terminalError && (
        <Alert variant="destructive">
          <AlertDescription>{terminalError}</AlertDescription>
        </Alert>
      )}
    </section>
  );
}
