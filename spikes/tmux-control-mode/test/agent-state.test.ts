import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import assert from "node:assert/strict";
import { afterEach, test } from "node:test";

import {
  AGENT_STATE_OPTION,
  buildClaudeHookSettings,
  buildCodexHookSettings,
  reportAgentState,
  stateForHookEvent,
  type AgentStateEnvironment,
} from "../src/agent-state.js";

const temporaryDirectories: string[] = [];

afterEach(() => {
  for (const directory of temporaryDirectories.splice(0)) {
    rmSync(directory, { force: true, recursive: true });
  }
});

test("maps Claude lifecycle hooks to working, blocked, and finished", () => {
  assert.equal(stateForHookEvent("claude", { hook_event_name: "SessionStart" }), "working");
  assert.equal(stateForHookEvent("claude", { hook_event_name: "UserPromptSubmit" }), "working");
  assert.equal(
    stateForHookEvent("claude", {
      hook_event_name: "Notification",
      notification_type: "permission_prompt",
    }),
    "blocked",
  );
  assert.equal(stateForHookEvent("claude", { hook_event_name: "Stop" }), "finished");
  assert.equal(stateForHookEvent("claude", { hook_event_name: "SessionEnd" }), "finished");
});

test("maps Codex lifecycle hooks and documents its narrower blocked signal", () => {
  assert.equal(stateForHookEvent("codex", { hook_event_name: "SessionStart" }), "working");
  assert.equal(stateForHookEvent("codex", { hook_event_name: "UserPromptSubmit" }), "working");
  assert.equal(stateForHookEvent("codex", { hook_event_name: "PermissionRequest" }), "blocked");
  assert.equal(stateForHookEvent("codex", { hook_event_name: "Stop" }), "finished");
  assert.equal(stateForHookEvent("codex", { hook_event_name: "SessionEnd" }), "finished");
  assert.equal(stateForHookEvent("codex", { hook_event_name: "Notification" }), undefined);
});

test("writes the Run state before best-effort tmux notification", () => {
  const directory = mkdtempSync(join(tmpdir(), "mission-agent-state-"));
  temporaryDirectories.push(directory);
  const stateFile = join(directory, "state.json");
  const environment: AgentStateEnvironment = {
    runId: "run-123",
    stateFile,
    tmuxPath: "/usr/bin/tmux",
    socketName: "mission-test",
    paneId: "%7",
  };
  const notifications: Array<{ path: string; args: string[] }> = [];

  const record = reportAgentState("claude", "blocked", environment, {
    now: () => new Date("2026-09-19T12:34:56.000Z"),
    runTmux: (path, args) => notifications.push({ path, args }),
  });

  assert.deepEqual(JSON.parse(readFileSync(stateFile, "utf8")), record);
  assert.equal(record.runId, "run-123");
  assert.deepEqual(notifications, [
    {
      path: "/usr/bin/tmux",
      args: [
        "-f",
        "/dev/null",
        "-L",
        "mission-test",
        "set-option",
        "-p",
        "-t",
        "%7",
        AGENT_STATE_OPTION,
        JSON.stringify(record),
      ],
    },
  ]);
});

test("does not turn a lost notification path into an agent failure", () => {
  const directory = mkdtempSync(join(tmpdir(), "mission-agent-state-"));
  temporaryDirectories.push(directory);
  const stateFile = join(directory, "state.json");

  assert.doesNotThrow(() =>
    reportAgentState(
      "codex",
      "finished",
      { runId: "run-456", stateFile, tmuxPath: "tmux", socketName: "mission-test", paneId: "%8" },
      { runTmux: () => { throw new Error("connection down"); } },
    ),
  );
  assert.equal(JSON.parse(readFileSync(stateFile, "utf8")).runId, "run-456");
});

test("builds lifecycle hook configuration for Claude and Codex", () => {
  const claude = buildClaudeHookSettings("node /opt/mission/agent-state.js");
  const codex = buildCodexHookSettings("node /opt/mission/agent-state.js");

  assert.equal(claude.hooks.SessionStart[0]?.hooks[0]?.command, "node /opt/mission/agent-state.js claude");
  assert.equal(claude.hooks.Notification[0]?.matcher, "permission_prompt|elicitation_dialog|elicitation_url_dialog");
  assert.equal(claude.hooks.Stop[0]?.hooks[0]?.command, "node /opt/mission/agent-state.js claude");
  assert.equal(claude.hooks.SessionEnd[0]?.hooks[0]?.command, "node /opt/mission/agent-state.js claude");

  assert.equal(codex.hooks.SessionStart[0]?.hooks[0]?.command, "node /opt/mission/agent-state.js codex");
  assert.equal(codex.hooks.PermissionRequest[0]?.hooks[0]?.command, "node /opt/mission/agent-state.js codex");
  assert.equal(codex.hooks.Stop[0]?.hooks[0]?.command, "node /opt/mission/agent-state.js codex");
  assert.equal(codex.hooks.SessionEnd[0]?.hooks[0]?.command, "node /opt/mission/agent-state.js codex");
});
