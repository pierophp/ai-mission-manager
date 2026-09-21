import type { TerminalAttachment } from "../terminal-types";
import { command, listen } from "./tauri";

export const terminalRuntimeAdapter = {
  open: (
    worksetId: number,
    terminalId: string,
    sessionName: string,
    paneId: string,
  ) =>
    command<TerminalAttachment>("open_terminal", {
      worksetId,
      terminalId,
      sessionName,
      paneId,
    }),
  input: (terminalId: string, input: number[]) =>
    command<void>("terminal_input", { terminalId, input }),
  resize: (terminalId: string, columns: number, rows: number) =>
    command<void>("terminal_resize", { terminalId, columns, rows }),
  close: (terminalId: string) => command<void>("close_terminal", { terminalId }),
  listen,
};
