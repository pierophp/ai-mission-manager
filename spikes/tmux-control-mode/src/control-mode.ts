import { spawn, type ChildProcessWithoutNullStreams } from "node:child_process";
import { EventEmitter } from "node:events";

import {
  AGENT_STATE_OPTION,
  type AgentStateRecord,
} from "./agent-state.js";

export interface PaneSize {
  columns: number;
  rows: number;
}

export interface PaneInfo extends PaneSize {
  paneId: string;
  pid: number;
}

export interface TmuxControlPaneOptions {
  tmuxPath: string;
  socketName: string;
  sessionName: string;
  paneId: string;
  transport?: TmuxTransport;
}

export interface SshTransportOptions {
  kind: "ssh";
  host: string;
  user?: string;
  port?: number;
  identityFile?: string;
  knownHostsFile?: string;
  strictHostKeyChecking?: "yes" | "accept-new" | "no";
  sshPath?: string;
}

export type TmuxTransport = { kind: "local" } | SshTransportOptions;

interface PendingCommand {
  command: string;
  resolve: (lines: string[]) => void;
  reject: (error: Error) => void;
  lines: string[];
}

const paneIdPattern = /^%\d+$/;
const sessionTargetPattern = /^[\w$@%+.,:=~-]+$/;
const agentStateSubscription = "mission-manager-agent-state";

export function validatePaneId(value: string): string {
  if (!paneIdPattern.test(value)) {
    throw new Error("invalid tmux pane ID: " + value);
  }
  return value;
}

function assertPositiveInteger(value: number, label: string): void {
  if (!Number.isInteger(value) || value < 1) {
    throw new RangeError(`${label} must be a positive integer`);
  }
}

export function validateSshTransport(transport: SshTransportOptions): void {
  if (transport.port !== undefined) {
    assertPositiveInteger(transport.port, "SSH port");
  }
  if (!/^[\w.@:-]+$/.test(transport.host)) {
    throw new Error("SSH host contains unsupported characters: " + transport.host);
  }
  if (transport.user && !/^[\w.-]+$/.test(transport.user)) {
    throw new Error("SSH user contains unsupported characters: " + transport.user);
  }
}

function decodePaneOutput(encoded: Buffer): Buffer {
  const decoded: number[] = [];

  for (let index = 0; index < encoded.length; index += 1) {
    const byte = encoded[index];
    const firstOctal = encoded[index + 1];
    const secondOctal = encoded[index + 2];
    const thirdOctal = encoded[index + 3];
    if (
      byte === 0x5c &&
      firstOctal !== undefined &&
      secondOctal !== undefined &&
      thirdOctal !== undefined &&
      firstOctal >= 0x30 &&
      firstOctal <= 0x37 &&
      secondOctal >= 0x30 &&
      secondOctal <= 0x37 &&
      thirdOctal >= 0x30 &&
      thirdOctal <= 0x37
    ) {
      decoded.push(
        (firstOctal - 0x30) * 64 +
          (secondOctal - 0x30) * 8 +
          (thirdOctal - 0x30),
      );
      index += 3;
      continue;
    }

    if (byte !== undefined) {
      decoded.push(byte);
    }
  }

  return Buffer.from(decoded);
}

export function validateTmuxTarget(value: string): string {
  if (!sessionTargetPattern.test(value)) {
    throw new Error(`tmux target contains unsupported characters: ${value}`);
  }
  return value;
}

export function shellQuote(value: string): string {
  return `'${value.replaceAll("'", `'"'"'`)}'`;
}

function parsePaneInfo(line: string): PaneInfo | undefined {
  const [paneId, pid, columns, rows] = line.split("|");
  if (!paneId || !pid || !columns || !rows || !paneIdPattern.test(paneId)) {
    return undefined;
  }

  const parsed = {
    paneId,
    pid: Number(pid),
    columns: Number(columns),
    rows: Number(rows),
  } satisfies PaneInfo;

  if (
    !Number.isInteger(parsed.pid) ||
    !Number.isInteger(parsed.columns) ||
    !Number.isInteger(parsed.rows)
  ) {
    return undefined;
  }

  return parsed;
}

function parseAgentState(value: string): AgentStateRecord | undefined {
  if (!value) {
    return undefined;
  }

  try {
    const parsed: unknown = JSON.parse(value);
    if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
      return undefined;
    }
    const record = parsed as Record<string, unknown>;
    if (
      (record.agent !== "claude" && record.agent !== "codex") ||
      typeof record.runId !== "string" ||
      !record.runId ||
      (record.state !== "working" && record.state !== "blocked" && record.state !== "finished") ||
      typeof record.updatedAt !== "string"
    ) {
      return undefined;
    }
    return {
      agent: record.agent,
      runId: record.runId,
      state: record.state,
      updatedAt: record.updatedAt,
    };
  } catch {
    return undefined;
  }
}

/**
 * The smallest useful runtime adapter proved by this spike.
 *
 * The adapter owns the tmux control-mode process and hides its line protocol.
 * Callers see pane bytes, input bytes, pane size, and stable pane identity.
 */
export class TmuxControlPane extends EventEmitter {
  private readonly options: TmuxControlPaneOptions;
  private child: ChildProcessWithoutNullStreams | undefined;
  private receiveBuffer = Buffer.alloc(0);
  private stderr = "";
  private currentCommand: PendingCommand | undefined;
  private readonly queuedCommands: PendingCommand[] = [];
  private initialCommandLines: string[] | undefined;
  private sessionReady: Promise<void> | undefined;
  private resolveSessionReady: (() => void) | undefined;
  private rejectSessionReady: ((error: Error) => void) | undefined;

  public constructor(options: TmuxControlPaneOptions) {
    super();
    if (!paneIdPattern.test(options.paneId)) {
      throw new Error(`invalid tmux pane ID: ${options.paneId}`);
    }
    this.options = options;
  }

  public get paneId(): string {
    return this.options.paneId;
  }

  public get sessionName(): string {
    return this.options.sessionName;
  }

  public override on(event: "output", listener: (chunk: Buffer) => void): this;
  public override on(event: "exit", listener: (code: number | null) => void): this;
  public override on(event: "agent-state", listener: (record: AgentStateRecord) => void): this;
  public override on(event: string, listener: (...args: any[]) => void): this {
    return super.on(event, listener);
  }

  public async connect(): Promise<void> {
    if (this.child) {
      return;
    }

    const controlProcess = this.controlProcess();
    const child = spawn(controlProcess.file, controlProcess.args, {
      stdio: ["pipe", "pipe", "pipe"],
    });
    this.child = child;
    this.sessionReady = new Promise<void>((resolve, reject) => {
      this.resolveSessionReady = resolve;
      this.rejectSessionReady = reject;
    });
    child.stdout.on("data", (chunk: Buffer) => this.consume(chunk));
    child.stderr.on("data", (chunk: Buffer) => {
      this.stderr += chunk.toString("utf8");
    });
    child.on("close", (code) => this.handleClose(code));
    child.on("error", (error) => this.handleClose(null, error));

    try {
      await new Promise<void>((resolve, reject) => {
        child.once("spawn", () => resolve());
        child.once("error", reject);
      });

      await this.waitForSessionReady();
      const panes = await this.listPanes();
      if (!panes.some((pane) => pane.paneId === this.options.paneId)) {
        throw new Error(`pane ${this.options.paneId} was not found`);
      }
      await this.request(
        `refresh-client -B ${shellQuote(`${agentStateSubscription}:${this.options.paneId}:#{${AGENT_STATE_OPTION}}`)}`,
      );
      const currentState = await this.agentState();
      if (currentState) {
        this.emit("agent-state", currentState);
      }
    } catch (error) {
      await this.close();
      if (error instanceof Error) {
        throw error;
      }
      throw new Error(String(error));
    }
  }

  public async sendInput(input: Uint8Array): Promise<void> {
    if (input.byteLength === 0) {
      return;
    }
    const hexBytes = Array.from(input, (byte) => `0x${byte.toString(16).padStart(2, "0")}`);
    await this.request(
      `send-keys -t ${this.options.paneId} -H ${hexBytes.join(" ")}`,
    );
  }

  public async resize(size: PaneSize): Promise<void> {
    assertPositiveInteger(size.columns, "columns");
    assertPositiveInteger(size.rows, "rows");
    await this.request(`refresh-client -C ${size.columns},${size.rows}`);
  }

  public async listPanes(): Promise<PaneInfo[]> {
    const lines = await this.request(
      `list-panes -t ${validateTmuxTarget(this.options.sessionName)} -F '#{pane_id}|#{pane_pid}|#{pane_width}|#{pane_height}'`,
    );
    return lines.flatMap((line) => {
      const pane = parsePaneInfo(line);
      return pane ? [pane] : [];
    });
  }

  public async snapshot(): Promise<Buffer> {
    const lines = await this.request(`capture-pane -p -e -t ${this.options.paneId}`);
    return Buffer.from(`${lines.join("\n")}\n`, "utf8");
  }

  public async agentState(): Promise<AgentStateRecord | undefined> {
    const lines = await this.request(
      `display-message -p -t ${this.options.paneId} ${shellQuote(`#{${AGENT_STATE_OPTION}}`)}`,
    );
    return parseAgentState(lines.join("\n").trim());
  }

  public async close(): Promise<void> {
    const child = this.child;
    if (!child) {
      return;
    }

    child.stdin.write("\n");
    await new Promise<void>((resolve) => {
      const timeout = setTimeout(() => {
        child.kill();
        resolve();
      }, 1_000);
      child.once("close", () => {
        clearTimeout(timeout);
        resolve();
      });
    });
  }

  private request(command: string): Promise<string[]> {
    if (!this.child) {
      return Promise.reject(new Error("tmux control mode is not connected"));
    }

    return new Promise((resolve, reject) => {
      this.queuedCommands.push({ command, resolve, reject, lines: [] });
      this.flushCommandQueue();
    });
  }

  private controlProcess(): { file: string; args: string[] } {
    const tmuxArgs = [
      "-C",
      "-f",
      "/dev/null",
      "-L",
      this.options.socketName,
      "attach-session",
      "-t",
      validateTmuxTarget(this.options.sessionName),
    ];
    const transport = this.options.transport ?? { kind: "local" as const };
    if (transport.kind === "local") {
      return { file: this.options.tmuxPath, args: tmuxArgs };
    }

    validateSshTransport(transport);

    const target = transport.user ? `${transport.user}@${transport.host}` : transport.host;
    const remoteCommand = [this.options.tmuxPath, ...tmuxArgs].map(shellQuote).join(" ");
    const args = ["-T", "-o", "BatchMode=yes"];
    if (transport.port !== undefined) {
      args.push("-p", String(transport.port));
    }
    if (transport.identityFile) {
      args.push("-i", transport.identityFile);
    }
    if (transport.knownHostsFile) {
      args.push("-o", `UserKnownHostsFile=${transport.knownHostsFile}`);
    }
    if (transport.strictHostKeyChecking) {
      args.push("-o", `StrictHostKeyChecking=${transport.strictHostKeyChecking}`);
    }
    args.push(target, remoteCommand);
    return { file: transport.sshPath ?? "ssh", args };
  }

  private async waitForSessionReady(): Promise<void> {
    if (!this.sessionReady) {
      throw new Error("tmux control mode did not start");
    }

    let timeout: NodeJS.Timeout | undefined;
    try {
      await Promise.race([
        this.sessionReady,
        new Promise<never>((_, reject) => {
          timeout = setTimeout(() => reject(new Error("timed out attaching to tmux session")), 3_000);
        }),
      ]);
    } finally {
      if (timeout) {
        clearTimeout(timeout);
      }
    }
  }

  private flushCommandQueue(): void {
    if (this.currentCommand || !this.child) {
      return;
    }

    const next = this.queuedCommands.shift();
    if (!next) {
      return;
    }

    this.currentCommand = next;
    this.child.stdin.write(`${next.command}\n`);
  }

  private consume(chunk: Buffer): void {
    this.receiveBuffer = Buffer.concat([this.receiveBuffer, chunk]);
    let newline = this.receiveBuffer.indexOf(0x0a);
    while (newline !== -1) {
      const line = this.receiveBuffer.subarray(0, newline);
      this.receiveBuffer = this.receiveBuffer.subarray(newline + 1);
      this.handleLine(line[line.length - 1] === 0x0d ? line.subarray(0, -1) : line);
      newline = this.receiveBuffer.indexOf(0x0a);
    }
  }

  private handleLine(line: Buffer): void {
    const outputPrefix = Buffer.from("%output ");
    if (line.subarray(0, outputPrefix.length).equals(outputPrefix)) {
      const paneSeparator = line.indexOf(0x20, outputPrefix.length);
      if (paneSeparator === -1) {
        return;
      }
      const paneId = line.subarray(outputPrefix.length, paneSeparator).toString("ascii");
      if (paneId === this.options.paneId) {
        this.emit("output", decodePaneOutput(line.subarray(paneSeparator + 1)));
      }
      return;
    }

    const text = line.toString("utf8");
    if (text.startsWith("%subscription-changed ")) {
      const valueSeparator = text.indexOf(" : ");
      if (valueSeparator !== -1) {
        const header = text.slice(0, valueSeparator).split(" ");
        const subscriptionName = header[1];
        const paneId = header[5];
        if (subscriptionName === agentStateSubscription && paneId === this.options.paneId) {
          const state = parseAgentState(text.slice(valueSeparator + 3));
          if (state) {
            this.emit("agent-state", state);
          }
        }
      }
      return;
    }
    if (text.startsWith("%begin ")) {
      if (!this.currentCommand) {
        this.initialCommandLines = [];
        return;
      }
      this.currentCommand.lines = [];
      return;
    }

    if (text.startsWith("%end ") || text.startsWith("%error ")) {
      const command = this.currentCommand;
      if (!command && this.initialCommandLines) {
        const initialLines = this.initialCommandLines;
        this.initialCommandLines = undefined;
        if (text.startsWith("%error ")) {
          this.rejectSessionReady?.(
            new Error(initialLines.join("\n") || this.stderr || "tmux attach failed"),
          );
          this.resolveSessionReady = undefined;
          this.rejectSessionReady = undefined;
        }
        return;
      }
      if (!command) {
        return;
      }
      this.currentCommand = undefined;
      if (text.startsWith("%error ")) {
        command.reject(new Error(command.lines.join("\n") || this.stderr || "tmux command failed"));
      } else {
        command.resolve(command.lines);
      }
      this.flushCommandQueue();
      return;
    }

    if (text === "%exit") {
      this.emit("exit", null);
      return;
    }

    if (text.startsWith("%session-changed ")) {
      this.resolveSessionReady?.();
      this.resolveSessionReady = undefined;
      this.rejectSessionReady = undefined;
      return;
    }

    if (this.currentCommand) {
      this.currentCommand.lines.push(text);
    } else if (this.initialCommandLines) {
      this.initialCommandLines.push(text);
    }
  }

  private handleClose(code: number | null, error?: Error): void {
    const reason = error ?? new Error(this.stderr || `tmux control mode exited with code ${code ?? "unknown"}`);
    this.rejectSessionReady?.(reason);
    this.resolveSessionReady = undefined;
    this.rejectSessionReady = undefined;
    if (this.currentCommand) {
      this.currentCommand.reject(reason);
      this.currentCommand = undefined;
    }
    for (const command of this.queuedCommands.splice(0)) {
      command.reject(reason);
    }
    this.child = undefined;
    this.emit("exit", code);
  }
}
