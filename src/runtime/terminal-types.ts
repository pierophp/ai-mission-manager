export type PaneTab = {
  paneId: string;
  sessionName: string;
  runId: number;
  label: string;
  available: boolean;
  paneIndex: number;
  pid: number;
  columns: number;
  rows: number;
  title: string;
  currentCommand: string;
  currentPath: string;
};

export type TerminalAttachment = {
  terminalId: string;
  sessionName: string;
  paneId: string;
  snapshot: number[];
  panes: PaneTab[];
};

export type TerminalOutputEvent = {
  terminalId: string;
  paneId: string;
  data: number[];
};

export type TerminalExitEvent = {
  terminalId: string;
  paneId: string;
  code: number | null;
};
