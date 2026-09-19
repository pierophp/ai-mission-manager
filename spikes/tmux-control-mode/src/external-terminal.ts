import { execFile } from "node:child_process";

import {
  shellQuote,
  validatePaneId,
  validateSshTransport,
  validateTmuxTarget,
  type TmuxControlPaneOptions,
} from "./control-mode.js";

export interface MacTerminalOptions {
  osascriptPath?: string;
  terminalApp?: string;
}

function tmuxCommand(
  identity: TmuxControlPaneOptions,
  ...args: string[]
): string {
  return [identity.tmuxPath, "-f", "/dev/null", "-L", identity.socketName, ...args]
    .map(shellQuote)
    .join(" ");
}

function paneSessionCheck(identity: TmuxControlPaneOptions): string {
  validatePaneId(identity.paneId);
  validateTmuxTarget(identity.sessionName);

  const lookup = tmuxCommand(
    identity,
    "display-message",
    "-p",
    "-t",
    identity.paneId,
    "#{session_name}",
  );
  const attach = tmuxCommand(identity, "attach-session", "-t", identity.paneId);
  const expectedSession = shellQuote(identity.sessionName);
  const missingMessage = shellQuote(
    "Pane " + identity.paneId + " in session " + identity.sessionName + " was not found",
  );
  const wrongSessionMessage = shellQuote(
    "Pane " + identity.paneId + " is not in stored session " + identity.sessionName,
  );

  return [
    "actual_session=\"$(" + lookup + " 2>/dev/null)\"",
    "if [ -z \"$actual_session\" ]; then printf '%s\\n' " + missingMessage + "; exit 1; fi",
    "if [ \"$actual_session\" != " + expectedSession + " ]; then printf '%s\\n' " + wrongSessionMessage + "; exit 1; fi",
    "exec " + attach,
  ].join("; ");
}

function sshCommand(identity: TmuxControlPaneOptions, remoteCommand: string): string {
  const transport = identity.transport;
  if (!transport || transport.kind === "local") {
    return remoteCommand;
  }
  validateSshTransport(transport);

  const target = transport.user ? transport.user + "@" + transport.host : transport.host;
  const args = [transport.sshPath ?? "ssh", "-tt", "-o", "BatchMode=yes"];
  if (transport.port !== undefined) {
    args.push("-p", String(transport.port));
  }
  if (transport.identityFile) {
    args.push("-i", transport.identityFile);
  }
  if (transport.knownHostsFile) {
    args.push("-o", "UserKnownHostsFile=" + transport.knownHostsFile);
  }
  if (transport.strictHostKeyChecking) {
    args.push("-o", "StrictHostKeyChecking=" + transport.strictHostKeyChecking);
  }
  args.push(target, remoteCommand);
  return args.map(shellQuote).join(" ");
}

/**
 * Builds the command a real terminal should execute for a stored Pane.
 *
 * The session lookup happens before attach so a missing or stale identity
 * cannot fall through to the session's current/default Pane.
 */
export function buildPaneAttachCommand(identity: TmuxControlPaneOptions): string {
  return sshCommand(identity, paneSessionCheck(identity));
}

function appleScriptString(value: string): string {
  return "\"" +
    value
      .replaceAll("\\", "\\\\")
      .replaceAll("\"", "\\\"")
      .replaceAll("\n", "\\n") +
    "\"";
}

export function buildTerminalAppleScript(
  command: string,
  terminalApp = "Terminal",
): string {
  return [
    "tell application " + appleScriptString(terminalApp),
    "activate",
    "do script " + appleScriptString(command),
    "end tell",
  ].join("\n");
}

/** Opens the stored Pane in a new macOS Terminal window. */
export function openPaneInTerminal(
  identity: TmuxControlPaneOptions,
  options: MacTerminalOptions = {},
): Promise<void> {
  const osascriptPath = options.osascriptPath ?? "osascript";
  const script = buildTerminalAppleScript(
    buildPaneAttachCommand(identity),
    options.terminalApp ?? "Terminal",
  );

  return new Promise((resolve, reject) => {
    execFile(osascriptPath, ["-e", script], (error, _stdout, stderr) => {
      if (error) {
        const detail = String(stderr).trim();
        reject(new Error(detail ? error.message + ": " + detail : error.message));
        return;
      }
      resolve();
    });
  });
}

export type StoredPaneIdentity = TmuxControlPaneOptions;
