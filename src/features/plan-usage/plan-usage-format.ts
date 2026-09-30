import type { PlanUsageSnapshot, ProfileUsageState } from "../../runtime/types";

export type UsageTone = "normal" | "warning" | "critical";

// The thresholds the button colour and every bar agree on.
export function usageTone(usedPercent: number): UsageTone {
  if (usedPercent >= 90) return "critical";
  if (usedPercent >= 75) return "warning";
  return "normal";
}

// The button collapses every window into the one number that can change a
// decision: the closest any profile is to a wall.
export function worstUsedPercent(snapshot: PlanUsageSnapshot | undefined) {
  const percentages = (snapshot?.profiles ?? []).flatMap((profile) =>
    profile.windows.flatMap((window) =>
      window.usedPercent === null ? [] : [window.usedPercent],
    ),
  );
  return percentages.length === 0 ? null : Math.max(...percentages);
}

export function hasBlindProfile(snapshot: PlanUsageSnapshot | undefined) {
  return (snapshot?.profiles ?? []).some(
    (profile) => profile.state !== "ready",
  );
}

export function profileStateLabel(state: ProfileUsageState) {
  switch (state) {
    case "ready":
      return "Ready";
    case "signedOut":
      return "Not signed in";
    case "machineUnreachable":
      return "Machine unreachable";
    case "notReported":
      return "No usage reported";
  }
}

// "in 2h 14m" — the answer to "do I wait or not".
export function formatResetCountdown(resetsAt: number, now: number) {
  const seconds = resetsAt - Math.floor(now / 1000);
  if (seconds <= 0) return "resetting now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `in ${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `in ${hours}h ${minutes % 60}m`;
  const days = Math.floor(hours / 24);
  return `in ${days}d ${hours % 24}h`;
}

// The wall-clock time the countdown refers to, which stays right even when
// the snapshot it was read from has gone stale.
export function formatResetClock(resetsAt: number, now: number) {
  const reset = new Date(resetsAt * 1000);
  const withinADay = resetsAt * 1000 - now < 24 * 60 * 60 * 1000;
  return reset.toLocaleString(undefined, {
    weekday: withinADay ? undefined : "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}

export function formatSnapshotAge(fetchedAt: number | null, now: number) {
  if (fetchedAt === null) return "never read";
  const seconds = Math.max(0, Math.floor(now / 1000) - fetchedAt);
  if (seconds < 60) return "just now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} h ago`;
  return `${Math.floor(hours / 24)} d ago`;
}

export function providerLabel(provider: "claude" | "codex") {
  return provider === "claude" ? "Claude" : "Codex";
}
