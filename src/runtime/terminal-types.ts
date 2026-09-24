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
  generation: number;
  sessionName: string;
  paneId: string;
  snapshot: number[];
  panes: PaneTab[];
};

export type TerminalOutputEvent = {
  terminalId: string;
  generation: number;
  paneId: string;
  data: number[];
};

export type TerminalExitEvent = {
  terminalId: string;
  generation: number;
  paneId: string;
  code: number | null;
};
