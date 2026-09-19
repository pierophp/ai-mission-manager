const terminal = new Terminal({
  convertEol: false,
  cursorBlink: true,
  fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace",
  fontSize: 13,
  scrollback: 5000,
  theme: {
    background: "#0b0d11",
    foreground: "#e4e7ec",
  },
});
const fit = new FitAddon.FitAddon();
terminal.loadAddon(fit);
terminal.open(document.getElementById("terminal"));

const status = document.getElementById("status");

function toBase64(bytes) {
  let binary = "";
  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }
  return btoa(binary);
}

function fromBase64(value) {
  const binary = atob(value);
  return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}

async function post(path, body) {
  const response = await fetch(path, {
    body: JSON.stringify(body),
    headers: { "content-type": "application/json" },
    method: "POST",
  });
  if (!response.ok) {
    throw new Error(await response.text());
  }
}

let resizeTimer;
function resize() {
  fit.fit();
  clearTimeout(resizeTimer);
  resizeTimer = setTimeout(() => {
    void post("/resize", { columns: terminal.cols, rows: terminal.rows }).catch((error) => {
      status.textContent = error.message;
      status.style.color = "#f38ba8";
    });
  }, 80);
}

terminal.onData((data) => {
  void post("/input", { base64: toBase64(new TextEncoder().encode(data)) }).catch((error) => {
    status.textContent = error.message;
    status.style.color = "#f38ba8";
  });
});

const events = new EventSource("/events");
events.addEventListener("connected", () => {
  status.textContent = "attached · live";
  resize();
});
events.addEventListener("output", (event) => {
  terminal.write(fromBase64(JSON.parse(event.data)));
});
events.addEventListener("exit", (event) => {
  status.textContent = `detached · tmux exited (${JSON.parse(event.data).code ?? "unknown"})`;
  status.style.color = "#f9c74f";
});
events.addEventListener("error", () => {
  status.textContent = "connection lost";
  status.style.color = "#f38ba8";
});

window.addEventListener("resize", resize);
resize();
