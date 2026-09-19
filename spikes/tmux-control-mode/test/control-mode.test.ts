import { execFileSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import assert from "node:assert/strict";
import { after, before, test } from "node:test";

import { TmuxControlPane } from "../src/control-mode.js";
import { createEmbeddedTerminalServer } from "../src/server.js";

const tmuxPath = process.env.TMUX_PATH ?? "tmux";
const socketName = `mission-spike-${process.pid}`;
const sessionName = "outside-app";
const paneProgram = String.raw`trap 'printf "INTERRUPTED\n"' INT; sleep 0.3; printf 'READY\n'; while IFS= read -r line; do printf 'ECHO:%s\n' "$line"; done`;

function tmux(args: string[]): string {
  return execFileSync(tmuxPath, ["-f", "/dev/null", "-L", socketName, ...args], {
    encoding: "utf8",
  });
}

function waitForOutput(pane: TmuxControlPane, text: string): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    let output = Buffer.alloc(0);
    const timeout = setTimeout(() => {
      pane.off("output", onOutput);
      reject(new Error(`Timed out waiting for terminal output ${text}`));
    }, 3_000);

    const onOutput = (chunk: Buffer) => {
      output = Buffer.concat([output, chunk]);
      if (output.toString("utf8").includes(text)) {
        clearTimeout(timeout);
        pane.off("output", onOutput);
        resolve(output);
      }
    };

    pane.on("output", onOutput);
  });
}

function paneSnapshot(): { paneId: string; pid: number; width: number; height: number } {
  const line = tmux([
    "list-panes",
    "-t",
    sessionName,
    "-F",
    "#{pane_id} #{pane_pid} #{pane_width} #{pane_height}",
  ]).trim();
  const [paneId, pid, width, height] = line.split(" ");
  assert.ok(paneId && pid && width && height, `unexpected pane snapshot: ${line}`);
  return {
    paneId,
    pid: Number(pid),
    width: Number(width),
    height: Number(height),
  };
}

function listen(server: ReturnType<typeof createEmbeddedTerminalServer>): Promise<string> {
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (!address || typeof address === "string") {
        reject(new Error("server did not expose a TCP address"));
        return;
      }
      resolve(`http://127.0.0.1:${address.port}`);
    });
  });
}

function closeServer(server: ReturnType<typeof createEmbeddedTerminalServer>): Promise<void> {
  return new Promise((resolve, reject) => {
    server.close((error) => (error ? reject(error) : resolve()));
  });
}

before(() => {
  tmux(["new-session", "-d", "-s", sessionName, "sh", "-c", paneProgram]);
});

after(() => {
  try {
    tmux(["kill-server"]);
  } catch {
    // The test may already have stopped the isolated server.
  }
});

test("attaches to an existing pane, streams output, sends input, and resizes it", async () => {
  const beforeAttach = paneSnapshot();
  const pane = new TmuxControlPane({
    tmuxPath,
    socketName,
    sessionName,
    paneId: beforeAttach.paneId,
  });

  const ready = waitForOutput(pane, "READY");
  await pane.connect();
  await ready;

  assert.match((await pane.snapshot()).toString("utf8"), /READY/);

  const spikeRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
  const server = createEmbeddedTerminalServer(pane, {
    publicRoot: resolve(spikeRoot, "public"),
    vendorRoot: resolve(spikeRoot, "node_modules"),
  });
  const baseUrl = await listen(server);
  const events = await fetch(`${baseUrl}/events`);
  try {
    const page = await fetch(baseUrl);
    assert.equal(page.status, 200);
    assert.match(await page.text(), /Existing tmux Pane/);

    const xterm = await fetch(`${baseUrl}/vendor/xterm.js`);
    assert.equal(xterm.status, 200);
    assert.match(await xterm.text(), /Terminal/);

    const input = await fetch(`${baseUrl}/input`, {
      body: JSON.stringify({ base64: Buffer.from("bridge\r").toString("base64") }),
      headers: { "content-type": "application/json" },
      method: "POST",
    });
    assert.equal(input.status, 204);

    const resize = await fetch(`${baseUrl}/resize`, {
      body: JSON.stringify({ columns: 90, rows: 28 }),
      headers: { "content-type": "application/json" },
      method: "POST",
    });
    assert.equal(resize.status, 204);
    assert.deepEqual(
      (await pane.listPanes()).find((candidate) => candidate.paneId === beforeAttach.paneId),
      { paneId: beforeAttach.paneId, pid: beforeAttach.pid, columns: 90, rows: 28 },
    );
  } finally {
    await events.body?.cancel();
    await closeServer(server);
  }

  const echoed = waitForOutput(pane, "ECHO:hello");
  await pane.sendInput(Buffer.from("hello\r"));
  await echoed;

  const interrupted = waitForOutput(pane, "INTERRUPTED");
  await pane.sendInput(Buffer.from([0x03]));
  await interrupted;

  await pane.resize({ columns: 100, rows: 30 });
  const afterResize = (await pane.listPanes()).find(
    (candidate) => candidate.paneId === beforeAttach.paneId,
  );
  assert.deepEqual(afterResize, {
    paneId: beforeAttach.paneId,
    pid: beforeAttach.pid,
    columns: 100,
    rows: 30,
  });

  await pane.close();
  const afterDetach = paneSnapshot();
  assert.equal(afterDetach.paneId, beforeAttach.paneId);
  assert.equal(afterDetach.pid, beforeAttach.pid);
  assert.equal(tmux(["list-panes", "-t", sessionName]).trim().split("\n").length, 1);
});

test("reports a missing pane instead of attaching to a different pane", async () => {
  const pane = new TmuxControlPane({
    tmuxPath,
    socketName,
    sessionName,
    paneId: "%9999",
  });

  await assert.rejects(pane.connect(), /pane %9999 was not found/);
  await pane.close();
  assert.equal(tmux(["list-panes", "-t", sessionName]).trim().split("\n").length, 1);
});
