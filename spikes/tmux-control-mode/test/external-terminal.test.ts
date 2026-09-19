import { execFileSync, spawn, type ChildProcess } from "node:child_process";
import assert from "node:assert/strict";
import { after, before, test } from "node:test";

import {
  buildPaneAttachCommand,
  buildTerminalAppleScript,
} from "../src/external-terminal.js";

const tmuxPath = process.env.TMUX_PATH ?? "tmux";
const socketName = `mission-terminal-${process.pid}`;
const sessionName = "external-proof";

function tmux(args: string[]): string {
  return execFileSync(tmuxPath, ["-f", "/dev/null", "-L", socketName, ...args], {
    encoding: "utf8",
  });
}

function waitForClient(paneId: string): Promise<{ tty: string; paneId: string }> {
  return new Promise((resolve, reject) => {
    const timeout = setTimeout(() => {
      clearInterval(interval);
      reject(new Error(`timed out waiting for an external client focused on ${paneId}`));
    }, 3_000);
    const interval = setInterval(() => {
      try {
        const line = tmux([
          "list-clients",
          "-F",
          "#{client_tty}|#{client_control_mode}|#{pane_id}",
        ]).trim();
        const [tty, controlMode, currentPaneId] = line.split("|");
        if (tty && controlMode === "0" && currentPaneId === paneId) {
          clearTimeout(timeout);
          clearInterval(interval);
          resolve({ tty, paneId: currentPaneId });
        }
      } catch {
        // The isolated tmux server may not be ready for a query yet.
      }
    }, 25);
  });
}

function startTerminal(command: string): ChildProcess {
  return spawn("script", ["-q", "/dev/null", "sh", "-c", command], {
    env: { ...process.env, TERM: "xterm-256color" },
    stdio: ["ignore", "pipe", "pipe"],
  });
}

function waitForExit(child: ChildProcess): Promise<{ code: number | null; output: string }> {
  let output = "";
  child.stdout?.on("data", (chunk: Buffer) => {
    output += chunk.toString("utf8");
  });
  child.stderr?.on("data", (chunk: Buffer) => {
    output += chunk.toString("utf8");
  });
  return new Promise((resolve) => {
    child.once("close", (code) => resolve({ code, output }));
  });
}

before(() => {
  tmux(["new-session", "-d", "-s", sessionName, "sh"]);
  tmux(["split-window", "-h", "-t", sessionName]);
});

after(() => {
  try {
    tmux(["kill-server"]);
  } catch {
    // The test may already have stopped the isolated server.
  }
});

test("builds a guarded exact-pane command for a local Machine", () => {
  const command = buildPaneAttachCommand({
    tmuxPath,
    socketName,
    sessionName,
    paneId: "%1",
  });

  assert.match(command, /display-message/);
  assert.match(command, /attach-session/);
  assert.match(command, /%1/);
  assert.match(command, /external-proof/);
  assert.doesNotMatch(command, /ssh/);
});

test("wraps the exact-pane command for macOS Terminal.app", () => {
  const script = buildTerminalAppleScript(
    buildPaneAttachCommand({
      tmuxPath,
      socketName,
      sessionName,
      paneId: "%1",
    }),
  );

  assert.match(script, /^tell application "Terminal"/);
  assert.match(script, /activate/);
  assert.match(script, /do script/);
  assert.match(script, /display-message/);
  assert.match(script, /attach-session/);
});

test("opens a real terminal client focused on the stored local Pane", async () => {
  const panes = tmux(["list-panes", "-t", sessionName, "-F", "#{pane_id}"])
    .trim()
    .split("\n");
  const targetPane = panes[1];
  assert.ok(targetPane);

  const child = startTerminal(
    buildPaneAttachCommand({ tmuxPath, socketName, sessionName, paneId: targetPane }),
  );
  const client = await waitForClient(targetPane);

  assert.equal(client.paneId, targetPane);
  assert.equal(
    tmux(["display-message", "-p", "-t", client.paneId, "#{session_name}"]).trim(),
    sessionName,
  );

  tmux(["detach-client", "-t", client.tty]);
  child.kill("SIGTERM");
  await waitForExit(child);
});

test("reports a missing Pane without attaching to another Pane", async () => {
  const child = startTerminal(
    buildPaneAttachCommand({ tmuxPath, socketName, sessionName, paneId: "%9999" }),
  );
  const result = await waitForExit(child);

  assert.notEqual(result.code, 0);
  assert.match(result.output, /Pane %9999 in session external-proof was not found/);
  assert.equal(tmux(["list-clients"]).trim(), "");
});
