import { execFileSync, spawn, type ChildProcess } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir, userInfo } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import assert from "node:assert/strict";
import { after, before, test } from "node:test";

import { TmuxControlPane } from "../src/control-mode.js";
import { createEmbeddedTerminalServer } from "../src/server.js";

const remoteTmuxPath =
  process.env.TMUX_REMOTE_PATH ??
  process.env.TMUX_PATH ??
  execFileSync("which", ["tmux"], { encoding: "utf8" }).trim();
const sshPath = process.env.SSH_PATH ?? "ssh";
const sshdPath = process.env.SSHD_PATH ?? "/usr/sbin/sshd";
const sshdAvailable = existsSync(sshdPath);
const socketName = `mission-ssh-spike-${process.pid}`;
const sessionName = "remote-proof";
const username = userInfo().username;

function delay(milliseconds: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

function shellQuote(value: string): string {
  return `'${value.replaceAll("'", `'"'"'`)}'`;
}

function listen(server: ReturnType<typeof createEmbeddedTerminalServer>): Promise<string> {
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      if (!address || typeof address === "string") {
        reject(new Error("terminal server did not expose a TCP address"));
        return;
      }
      resolve("http://127.0.0.1:" + address.port);
    });
  });
}

function closeServer(server: ReturnType<typeof createEmbeddedTerminalServer>): Promise<void> {
  return new Promise((resolve, reject) => {
    server.close((error) => (error ? reject(error) : resolve()));
  });
}

async function waitForSseOutput(
  reader: ReadableStreamDefaultReader<Uint8Array>,
  text: string,
): Promise<void> {
  const decoder = new TextDecoder();
  let buffer = "";
  let output = "";
  const timeout = setTimeout(() => void reader.cancel(), 5_000);
  try {
    while (true) {
      const result = await reader.read();
      if (result.done) {
        throw new Error("Terminal event stream closed before " + text);
      }
      buffer += decoder.decode(result.value, { stream: true });
      const events = buffer.split("\n\n");
      buffer = events.pop() ?? "";
      for (const event of events) {
        if (!event.split("\n").includes("event: output")) {
          continue;
        }
        const data = event.split("\n").find((line) => line.startsWith("data: "));
        if (data) {
          output += Buffer.from(JSON.parse(data.slice("data: ".length)), "base64").toString("utf8");
          if (output.includes(text)) {
            return;
          }
        }
      }
    }
  } finally {
    clearTimeout(timeout);
  }
}

async function freePort(): Promise<number> {
  const server = createServer();
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => resolve());
  });
  const address = server.address();
  if (!address || typeof address === "string") {
    server.close();
    throw new Error("could not allocate a loopback port");
  }
  const port = address.port;
  await new Promise<void>((resolve, reject) => server.close((error) => (error ? reject(error) : resolve())));
  return port;
}

class LoopbackSshd {
  private constructor(
    private readonly child: ChildProcess,
    private readonly root: string,
    public readonly port: number,
    public readonly clientKeyPath: string,
    private readonly remoteTmuxPath: string,
  ) {}

  public static async start(): Promise<LoopbackSshd> {
    const root = mkdtempSync(join(tmpdir(), "mission-ssh-spike-"));
    const clientKeyPath = join(root, "client");
    const hostKeyPath = join(root, "host");
    const authorizedKeysPath = join(root, "authorized_keys");
    const configPath = join(root, "sshd_config");
    const port = await freePort();

    execFileSync("ssh-keygen", ["-q", "-t", "ed25519", "-N", "", "-f", clientKeyPath]);
    execFileSync("ssh-keygen", ["-q", "-t", "ed25519", "-N", "", "-f", hostKeyPath]);
    writeFileSync(authorizedKeysPath, readFileSync(`${clientKeyPath}.pub`));
    writeFileSync(
      configPath,
      [
        `Port ${port}`,
        "ListenAddress 127.0.0.1",
        `HostKey ${hostKeyPath}`,
        `PidFile ${join(root, "sshd.pid")}`,
        `AuthorizedKeysFile ${authorizedKeysPath}`,
        "PasswordAuthentication no",
        "KbdInteractiveAuthentication no",
        "PubkeyAuthentication yes",
        "PermitRootLogin no",
        "UsePAM no",
        "StrictModes no",
        "LogLevel ERROR",
        "",
      ].join("\n"),
    );

    const child = spawn(sshdPath, ["-D", "-e", "-f", configPath], {
      detached: true,
      stdio: ["ignore", "ignore", "ignore"],
    });
    const instance = new LoopbackSshd(child, root, port, clientKeyPath, remoteTmuxPath);
    for (let attempt = 0; attempt < 40; attempt += 1) {
      try {
        instance.runSsh(["true"]);
        return instance;
      } catch {
        await delay(50);
      }
    }

    await instance.stop();
    throw new Error("loopback sshd did not become ready");
  }

  public runSsh(remoteCommand: string[]): string {
    const command = remoteCommand.map(shellQuote).join(" ");
    return execFileSync(
      sshPath,
      [
        "-T",
        "-o",
        "BatchMode=yes",
        "-o",
        "StrictHostKeyChecking=no",
        "-o",
        "UserKnownHostsFile=/dev/null",
        "-o",
        "LogLevel=ERROR",
        "-p",
        String(this.port),
        "-i",
        this.clientKeyPath,
        `${username}@127.0.0.1`,
        command,
      ],
      { encoding: "utf8" },
    );
  }

  public runTmux(args: string[]): string {
    return this.runSsh([this.remoteTmuxPath, "-f", "/dev/null", "-L", socketName, ...args]);
  }

  public sendBytes(paneId: string, bytes: Uint8Array): void {
    const hexBytes = Array.from(bytes, (byte) => `0x${byte.toString(16).padStart(2, "0")}`);
    this.runTmux(["send-keys", "-t", paneId, "-H", ...hexBytes]);
  }

  public killControlClient(): void {
    try {
      this.runSsh([
        "pkill",
        "-KILL",
        "-f",
        `tmux -C -f /dev/null -L ${socketName} attach-session -t ${sessionName}`,
      ]);
    } catch {
      // pkill returns non-zero after it terminates the matching control client.
    }
  }

  public async stop(): Promise<void> {
    if (this.child.exitCode === null && this.child.signalCode === null) {
      if (this.child.pid) {
        process.kill(-this.child.pid, "SIGKILL");
      } else {
        this.child.kill("SIGKILL");
      }
      await new Promise<void>((resolve) => this.child.once("close", () => resolve()));
    }
    rmSync(this.root, { force: true, recursive: true });
  }
}

function waitForOutput(pane: TmuxControlPane, text: string): Promise<Buffer> {
  return new Promise((resolve, reject) => {
    let output = Buffer.alloc(0);
    const timeout = setTimeout(() => {
      pane.off("output", onOutput);
      reject(
        new Error(
          `Timed out waiting for remote terminal output ${text}; received ${JSON.stringify(output.toString("utf8"))}`,
        ),
      );
    }, 5_000);

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

function remotePaneSnapshot(machine: LoopbackSshd): { paneId: string; pid: number; columns: number; rows: number } {
  const line = machine
    .runTmux(["list-panes", "-t", sessionName, "-F", "#{pane_id}|#{pane_pid}|#{pane_width}|#{pane_height}"])
    .trim();
  const [paneId, pid, columns, rows] = line.split("|");
  assert.ok(paneId && pid && columns && rows, `unexpected remote pane snapshot: ${line}`);
  return { paneId, pid: Number(pid), columns: Number(columns), rows: Number(rows) };
}

let machine: LoopbackSshd | undefined;
let remotePaneId: string | undefined;

before(async () => {
  if (!sshdAvailable) {
    return;
  }
  machine = await LoopbackSshd.start();
  machine.runTmux(["new-session", "-d", "-s", sessionName]);
  remotePaneId = remotePaneSnapshot(machine).paneId;
  machine.sendBytes(
    remotePaneId,
    Buffer.from(
      String.raw`trap 'printf "REMOTE_INTERRUPTED\n"' INT; sleep 0.4; printf 'REMOTE_READY\n'; while IFS= read -r line; do printf 'REMOTE_ECHO:%s\n' "$line"; done` +
        String.fromCharCode(0x0d),
    ),
  );
});

after(async () => {
  if (!machine) {
    return;
  }
  try {
    machine.runTmux(["kill-server"]);
  } catch {
    // The SSH server may already be stopped after a failed setup.
  }
  await machine.stop();
});

test(
  "drives an existing remote pane over SSH and reconnects after transport loss",
  { skip: !sshdAvailable },
  async () => {
    assert.ok(machine);
    assert.ok(remotePaneId);
    const beforeAttach = remotePaneSnapshot(machine);
    const options = {
      tmuxPath: remoteTmuxPath,
      socketName,
      sessionName,
      paneId: remotePaneId,
      transport: {
        kind: "ssh" as const,
        host: "127.0.0.1",
        port: machine.port,
        user: username,
        identityFile: machine.clientKeyPath,
        strictHostKeyChecking: "no" as const,
        knownHostsFile: "/dev/null",
        sshPath,
      },
    };
    const pane = new TmuxControlPane(options);
    const spikeRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
    const server = createEmbeddedTerminalServer(pane, {
      publicRoot: resolve(spikeRoot, "public"),
      vendorRoot: resolve(spikeRoot, "node_modules"),
    });
    const baseUrl = await listen(server);
    let reader: ReadableStreamDefaultReader<Uint8Array> | undefined;
    let reconnectedPane: TmuxControlPane | undefined;
    try {
      await pane.connect();
      const page = await fetch(baseUrl + "/");
      assert.equal(page.status, 200);
      assert.match(await page.text(), /Existing tmux Pane/);
      const events = await fetch(baseUrl + "/events");
      assert.ok(events.body);
      reader = events.body.getReader();

      const ready = waitForSseOutput(reader, "REMOTE_READY");
      await ready;

      const echoed = waitForSseOutput(reader, "REMOTE_ECHO:hello");
      const input = await fetch(baseUrl + "/input", {
        body: JSON.stringify({ base64: Buffer.from("hello\r").toString("base64") }),
        headers: { "content-type": "application/json" },
        method: "POST",
      });
      assert.equal(input.status, 204);
      await echoed;

      const interrupted = waitForSseOutput(reader, "REMOTE_INTERRUPTED");
      const interrupt = await fetch(baseUrl + "/input", {
        body: JSON.stringify({ base64: Buffer.from([0x03]).toString("base64") }),
        headers: { "content-type": "application/json" },
        method: "POST",
      });
      assert.equal(interrupt.status, 204);
      await interrupted;

      const resize = await fetch(baseUrl + "/resize", {
        body: JSON.stringify({ columns: 90, rows: 28 }),
        headers: { "content-type": "application/json" },
        method: "POST",
      });
      assert.equal(resize.status, 204);
      assert.deepEqual(remotePaneSnapshot(machine), {
        paneId: beforeAttach.paneId,
        pid: beforeAttach.pid,
        columns: 90,
        rows: 28,
      });
      assert.equal(machine.runTmux(["list-sessions", "-F", "#{session_name}"]).trim(), sessionName);

      const disconnected = new Promise<void>((resolve, reject) => {
        const timeout = setTimeout(() => reject(new Error("SSH control client did not disconnect")), 5_000);
        pane.once("exit", () => {
          clearTimeout(timeout);
          resolve();
        });
      });
      machine.killControlClient();
      await disconnected;
      assert.deepEqual(remotePaneSnapshot(machine), {
        paneId: beforeAttach.paneId,
        pid: beforeAttach.pid,
        columns: 90,
        rows: 28,
      });

      reconnectedPane = new TmuxControlPane({
        ...options,
        transport: {
          ...options.transport,
          port: machine.port,
          identityFile: machine.clientKeyPath,
        },
      });
      const recovered = waitForOutput(reconnectedPane, "REMOTE_ECHO:recovered");
      await reconnectedPane.connect();
      await reconnectedPane.sendInput(Buffer.from("recovered\r"));
      await recovered;
      assert.equal(machine.runTmux(["list-sessions", "-F", "#{session_name}"]).trim(), sessionName);
    } finally {
      await reader?.cancel();
      await closeServer(server);
      await pane.close();
      await reconnectedPane?.close();
    }
  },
);
