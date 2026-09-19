import { createReadStream, existsSync } from "node:fs";
import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import { fileURLToPath } from "node:url";
import { dirname, extname, resolve } from "node:path";

import { TmuxControlPane, type PaneSize } from "./control-mode.js";

export interface EmbeddedTerminalServerOptions {
  publicRoot: string;
  vendorRoot: string;
}

const contentTypes: Record<string, string> = {
  ".css": "text/css; charset=utf-8",
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
};

function sendJson(response: ServerResponse, status: number, body: unknown): void {
  const payload = JSON.stringify(body);
  response.writeHead(status, {
    "content-type": "application/json; charset=utf-8",
    "content-length": Buffer.byteLength(payload),
  });
  response.end(payload);
}

function sendError(response: ServerResponse, error: unknown): void {
  const message = error instanceof Error ? error.message : String(error);
  sendJson(response, 400, { error: message });
}

async function readJson(request: IncomingMessage): Promise<Record<string, unknown>> {
  const chunks: Buffer[] = [];
  let length = 0;
  for await (const chunk of request) {
    const buffer = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
    length += buffer.length;
    if (length > 1024 * 1024) {
      throw new Error("request body is too large");
    }
    chunks.push(buffer);
  }

  const parsed: unknown = JSON.parse(Buffer.concat(chunks).toString("utf8"));
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
    throw new Error("request body must be a JSON object");
  }
  return parsed as Record<string, unknown>;
}

function serveFile(response: ServerResponse, path: string): void {
  if (!existsSync(path)) {
    response.writeHead(404);
    response.end("not found");
    return;
  }
  response.writeHead(200, { "content-type": contentTypes[extname(path)] ?? "application/octet-stream" });
  createReadStream(path).pipe(response);
}

function sendEvent(response: ServerResponse, event: string, data: unknown): void {
  response.write(`event: ${event}\ndata: ${JSON.stringify(data)}\n\n`);
}

export function createEmbeddedTerminalServer(
  pane: TmuxControlPane,
  options: EmbeddedTerminalServerOptions,
): Server {
  const clients = new Set<ServerResponse>();
  const onOutput = (chunk: Buffer) => {
    for (const client of clients) {
      sendEvent(client, "output", chunk.toString("base64"));
    }
  };
  const onExit = (code: number | null) => {
    for (const client of clients) {
      sendEvent(client, "exit", { code });
    }
  };

  pane.on("output", onOutput);
  pane.on("exit", onExit);

  const server = createServer(async (request, response) => {
    const url = new URL(request.url ?? "/", "http://localhost");

    if (request.method === "GET" && url.pathname === "/") {
      serveFile(response, resolve(options.publicRoot, "index.html"));
      return;
    }
    if (request.method === "GET" && url.pathname === "/app.js") {
      serveFile(response, resolve(options.publicRoot, "app.js"));
      return;
    }
    if (request.method === "GET" && url.pathname === "/app.css") {
      serveFile(response, resolve(options.publicRoot, "app.css"));
      return;
    }
    if (request.method === "GET" && url.pathname === "/vendor/xterm.js") {
      serveFile(response, resolve(options.vendorRoot, "@xterm/xterm/lib/xterm.js"));
      return;
    }
    if (request.method === "GET" && url.pathname === "/vendor/xterm.css") {
      serveFile(response, resolve(options.vendorRoot, "@xterm/xterm/css/xterm.css"));
      return;
    }
    if (request.method === "GET" && url.pathname === "/vendor/addon-fit.js") {
      serveFile(response, resolve(options.vendorRoot, "@xterm/addon-fit/lib/addon-fit.js"));
      return;
    }
    if (request.method === "GET" && url.pathname === "/events") {
      response.writeHead(200, {
        "cache-control": "no-cache",
        connection: "keep-alive",
        "content-type": "text/event-stream; charset=utf-8",
      });
      clients.add(response);
      sendEvent(response, "connected", {});
      void pane
        .snapshot()
        .then((snapshot) => {
          if (clients.has(response)) {
            sendEvent(response, "output", snapshot.toString("base64"));
          }
        })
        .catch((error: unknown) => {
          if (clients.has(response)) {
            sendEvent(response, "error", { message: error instanceof Error ? error.message : String(error) });
          }
        });
      response.on("close", () => {
        clients.delete(response);
      });
      return;
    }
    if (request.method === "GET" && url.pathname === "/status") {
      sendJson(response, 200, { paneId: pane.paneId, sessionName: pane.sessionName });
      return;
    }
    if (request.method === "POST" && url.pathname === "/input") {
      try {
        const body = await readJson(request);
        if (typeof body.base64 !== "string") {
          throw new Error("input.base64 must be a string");
        }
        await pane.sendInput(Buffer.from(body.base64, "base64"));
        response.writeHead(204);
        response.end();
      } catch (error) {
        sendError(response, error);
      }
      return;
    }
    if (request.method === "POST" && url.pathname === "/resize") {
      try {
        const body = await readJson(request);
        const size: PaneSize = {
          columns: Number(body.columns),
          rows: Number(body.rows),
        };
        await pane.resize(size);
        response.writeHead(204);
        response.end();
      } catch (error) {
        sendError(response, error);
      }
      return;
    }

    response.writeHead(404);
    response.end("not found");
  });

  server.once("close", () => {
    pane.off("output", onOutput);
    pane.off("exit", onExit);
    for (const client of clients) {
      client.end();
    }
    clients.clear();
  });

  return server;
}

function flag(args: string[], name: string, fallback?: string): string {
  const index = args.indexOf(name);
  const value = index === -1 ? undefined : args[index + 1];
  if (value) {
    return value;
  }
  if (fallback !== undefined) {
    return fallback;
  }
  throw new Error(`missing ${name}`);
}

async function main(): Promise<void> {
  const args = process.argv.slice(2);
  const pane = new TmuxControlPane({
    tmuxPath: flag(args, "--tmux", "tmux"),
    socketName: flag(args, "--socket"),
    sessionName: flag(args, "--session"),
    paneId: flag(args, "--pane"),
  });
  await pane.connect();

  const currentDirectory = dirname(fileURLToPath(import.meta.url));
  const server = createEmbeddedTerminalServer(pane, {
    publicRoot: resolve(currentDirectory, "../public"),
    vendorRoot: resolve(currentDirectory, "../../node_modules"),
  });
  const port = Number(flag(args, "--port", "4173"));
  server.listen(port, "127.0.0.1", () => {
    console.log(`tmux control-mode spike at http://127.0.0.1:${port}`);
  });

  const stop = async () => {
    await pane.close();
    server.close();
  };
  process.once("SIGINT", () => void stop());
  process.once("SIGTERM", () => void stop());
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  void main().catch((error: unknown) => {
    console.error(error);
    process.exitCode = 1;
  });
}
