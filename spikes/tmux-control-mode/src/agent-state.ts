import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { randomUUID } from "node:crypto";

export const AGENT_STATE_OPTION = "@ai_mission_manager_run_state";

export type AgentProvider = "claude" | "codex";
export type AgentState = "working" | "blocked" | "finished";

export interface AgentStateRecord {
  agent: AgentProvider;
  runId: string;
  state: AgentState;
  updatedAt: string;
}

export interface AgentStateEnvironment {
  runId?: string;
  stateFile?: string;
  tmuxPath?: string;
  socketName?: string;
  paneId?: string;
}

export function agentStateEnvironmentFromProcess(
  environment: NodeJS.ProcessEnv = process.env,
): AgentStateEnvironment {
  const result: AgentStateEnvironment = {};
  if (environment.AI_MISSION_MANAGER_RUN_ID) {
    result.runId = environment.AI_MISSION_MANAGER_RUN_ID;
  }
  if (environment.AI_MISSION_MANAGER_STATE_FILE) {
    result.stateFile = environment.AI_MISSION_MANAGER_STATE_FILE;
  }
  if (environment.AI_MISSION_MANAGER_TMUX_PATH) {
    result.tmuxPath = environment.AI_MISSION_MANAGER_TMUX_PATH;
  }
  if (environment.AI_MISSION_MANAGER_TMUX_SOCKET) {
    result.socketName = environment.AI_MISSION_MANAGER_TMUX_SOCKET;
  }
  if (environment.AI_MISSION_MANAGER_PANE_ID) {
    result.paneId = environment.AI_MISSION_MANAGER_PANE_ID;
  }
  return result;
}

export interface AgentStateReporterDependencies {
  now?: () => Date;
  runTmux?: (path: string, args: string[]) => void;
}

interface HookCommand {
  type: "command";
  command: string;
}

interface HookGroup {
  matcher?: string;
  hooks: HookCommand[];
}

interface HookSettings {
  hooks: Record<string, HookGroup[]>;
}

export interface ClaudeHookSettings extends HookSettings {
  hooks: {
    SessionStart: HookGroup[];
    UserPromptSubmit: HookGroup[];
    Notification: HookGroup[];
    Stop: HookGroup[];
    SessionEnd: HookGroup[];
  };
}

export interface CodexHookSettings extends HookSettings {
  hooks: {
    SessionStart: HookGroup[];
    UserPromptSubmit: HookGroup[];
    PermissionRequest: HookGroup[];
    Stop: HookGroup[];
    SessionEnd: HookGroup[];
  };
}

function commandGroup(command: string, matcher?: string): HookGroup {
  return matcher === undefined
    ? { hooks: [{ type: "command", command }] }
    : { matcher, hooks: [{ type: "command", command }] };
}

function hookCommand(command: string, provider: AgentProvider): string {
  return `${command} ${provider}`;
}

export function buildClaudeHookSettings(command: string): ClaudeHookSettings {
  const hook = hookCommand(command, "claude");
  return {
    hooks: {
      SessionStart: [commandGroup(hook)],
      UserPromptSubmit: [commandGroup(hook)],
      Notification: [
        commandGroup(
          hook,
          "permission_prompt|elicitation_dialog|elicitation_url_dialog",
        ),
      ],
      Stop: [commandGroup(hook)],
      SessionEnd: [commandGroup(hook)],
    },
  };
}

export function buildCodexHookSettings(command: string): CodexHookSettings {
  const hook = hookCommand(command, "codex");
  return {
    hooks: {
      SessionStart: [commandGroup(hook)],
      UserPromptSubmit: [commandGroup(hook)],
      PermissionRequest: [commandGroup(hook)],
      Stop: [commandGroup(hook)],
      SessionEnd: [commandGroup(hook)],
    },
  };
}

export function stateForHookEvent(
  provider: AgentProvider,
  event: Record<string, unknown>,
): AgentState | undefined {
  const eventName = event.hook_event_name;
  if (eventName === "SessionStart" || eventName === "UserPromptSubmit") {
    return "working";
  }
  if (eventName === "Stop" || eventName === "SessionEnd") {
    return "finished";
  }
  if (provider === "claude" && eventName === "Notification") {
    const notificationType = event.notification_type;
    if (
      notificationType === "permission_prompt" ||
      notificationType === "elicitation_dialog" ||
      notificationType === "elicitation_url_dialog"
    ) {
      return "blocked";
    }
  }
  if (provider === "codex" && eventName === "PermissionRequest") {
    return "blocked";
  }
  return undefined;
}

function writeStateFile(path: string, record: AgentStateRecord): void {
  mkdirSync(dirname(path), { mode: 0o700, recursive: true });
  const temporaryPath = `${path}.${process.pid}.${randomUUID()}.tmp`;
  writeFileSync(temporaryPath, JSON.stringify(record) + "\n", {
    encoding: "utf8",
    mode: 0o600,
  });
  renameSync(temporaryPath, path);
}

function notifyTmux(
  record: AgentStateRecord,
  environment: AgentStateEnvironment,
  runTmux: (path: string, args: string[]) => void,
): void {
  if (!environment.tmuxPath || !environment.socketName || !environment.paneId) {
    return;
  }
  runTmux(environment.tmuxPath, [
    "-f",
    "/dev/null",
    "-L",
    environment.socketName,
    "set-option",
    "-p",
    "-t",
    environment.paneId,
    AGENT_STATE_OPTION,
    JSON.stringify(record),
  ]);
}

export function reportAgentState(
  provider: AgentProvider,
  state: AgentState,
  environment: AgentStateEnvironment,
  dependencies: AgentStateReporterDependencies = {},
): AgentStateRecord {
  if (!environment.runId) {
    throw new Error("AI_MISSION_MANAGER_RUN_ID is required");
  }
  if (!environment.stateFile) {
    throw new Error("AI_MISSION_MANAGER_STATE_FILE is required");
  }

  const record: AgentStateRecord = {
    agent: provider,
    runId: environment.runId,
    state,
    updatedAt: (dependencies.now ?? (() => new Date()))().toISOString(),
  };
  writeStateFile(environment.stateFile, record);

  try {
    notifyTmux(
      record,
      environment,
      dependencies.runTmux ?? ((path, args) => {
        execFileSync(path, args, { stdio: "ignore" });
      }),
    );
  } catch {
    // The state file is the truth. The tmux notification only reduces latency.
  }

  return record;
}

export function runAgentStateHook(
  provider: AgentProvider,
  event: Record<string, unknown>,
  environment: AgentStateEnvironment,
  dependencies: AgentStateReporterDependencies = {},
): AgentStateRecord | undefined {
  const state = stateForHookEvent(provider, event);
  if (!state || !environment.runId || !environment.stateFile) {
    return undefined;
  }
  return reportAgentState(provider, state, environment, dependencies);
}

async function readStdin(): Promise<string> {
  const chunks: Buffer[] = [];
  for await (const chunk of process.stdin) {
    chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
  }
  return Buffer.concat(chunks).toString("utf8");
}

export async function main(
  args = process.argv.slice(2),
  environment: AgentStateEnvironment = agentStateEnvironmentFromProcess(),
): Promise<void> {
  const provider = args[0];
  if (provider !== "claude" && provider !== "codex") {
    throw new Error("usage: agent-state.js <claude|codex>");
  }
  const input = (await readStdin()).trim();
  if (!input) {
    return;
  }
  const event = JSON.parse(input) as Record<string, unknown>;
  runAgentStateHook(provider, event, environment);
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  void main().catch((error: unknown) => {
    console.error(error);
    process.exitCode = 1;
  });
}

export function readAgentStateFile(path: string): AgentStateRecord {
  return JSON.parse(readFileSync(path, "utf8")) as AgentStateRecord;
}
