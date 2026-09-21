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
    <section className="embedded-terminal" aria-labelledby="embedded-terminal-heading">
      <div className="embedded-terminal-heading">
        <div>
          <p className="eyebrow">Embedded terminal</p>
          <h2 id="embedded-terminal-heading">{activePane.label}</h2>
          <p className="embedded-terminal-status">{status}</p>
        </div>
        <Button type="button" variant="secondary" onClick={onClose}>
          Close view
        </Button>
      </div>
      <div className="terminal-tabs" role="tablist" aria-label="Panes in this Workset">
        {panes.map((pane) => (
          <button
            type="button"
            role="tab"
            aria-selected={pane.paneId === activePane.paneId}
            className={
              pane.paneId === activePane.paneId
                ? "terminal-tab active"
                : "terminal-tab"
            }
            key={`${pane.sessionName}-${pane.paneId}`}
            disabled={!pane.available}
            onClick={() => void attachPaneRef.current?.(pane)}
          >
            {pane.label}
            <small>{pane.currentCommand || pane.currentPath || "unavailable"}</small>
          </button>
        ))}
      </div>
      <div className="terminal-surface" ref={terminalContainerRef} />
      {terminalError && (
        <Alert variant="destructive">
          <AlertDescription>{terminalError}</AlertDescription>
        </Alert>
      )}
    </section>
  );
}
