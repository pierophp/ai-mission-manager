import { listen } from "./tauri";
import { commands } from "../bindings";

export const terminalRuntimeAdapter = {
  open: (
    runId: number,
    terminalId: string,
    sessionName: string,
    paneId: string,
  ) =>
    commands.openTerminal(runId, terminalId, sessionName, paneId),
  input: (terminalId: string, input: number[]) =>
    commands.terminalInput(terminalId, input),
  resize: (terminalId: string, columns: number, rows: number) =>
    commands.terminalResize(terminalId, columns, rows),
  close: (terminalId: string) => commands.closeTerminal(terminalId),
  listen,
};
