import { describe, expect, it } from "vitest";

import type { PlanUsageSnapshot } from "../../runtime/types";
import {
  formatResetCountdown,
  formatSnapshotAge,
  hasBlindProfile,
  usageTone,
  worstUsedPercent,
} from "./plan-usage-format";

const snapshot = (
  profiles: PlanUsageSnapshot["profiles"],
): PlanUsageSnapshot => ({
  profiles,
  fetchedAt: 1790731000,
  status: "ready",
});

const profile = (
  overrides: Partial<PlanUsageSnapshot["profiles"][number]>,
): PlanUsageSnapshot["profiles"][number] => ({
  profileId: 1,
  profileName: "Personal",
  provider: "claude",
  machineId: 1,
  machineName: "This Mac",
  state: "ready",
  detail: null,
  plan: null,
  observedAt: null,
  windows: [],
  ...overrides,
});

describe("worstUsedPercent", () => {
  it("takes the closest window to a wall across every profile", () => {
    expect(
      worstUsedPercent(
        snapshot([
          profile({
            windows: [
              { id: "five_hour", label: "5h", usedPercent: 12, resetsAt: null },
            ],
          }),
          profile({
            profileId: 2,
            provider: "codex",
            windows: [
              { id: "primary", label: "5h", usedPercent: 3, resetsAt: null },
              {
                id: "secondary",
                label: "Weekly",
                usedPercent: 88,
                resetsAt: null,
              },
            ],
          }),
        ]),
      ),
    ).toBe(88);
  });

  it("reports nothing rather than zero when no window was read", () => {
    expect(
      worstUsedPercent(snapshot([profile({ state: "signedOut" })])),
    ).toBeNull();
    expect(worstUsedPercent(undefined)).toBeNull();
  });
});

describe("hasBlindProfile", () => {
  it("flags a profile whose usage could not be read", () => {
    expect(
      hasBlindProfile(
        snapshot([
          profile({}),
          profile({ profileId: 2, state: "machineUnreachable" }),
        ]),
      ),
    ).toBe(true);
  });

  it("stays quiet when every profile reported", () => {
    expect(hasBlindProfile(snapshot([profile({})]))).toBe(false);
  });
});

describe("usageTone", () => {
  it("escalates at the thresholds the button and the bars share", () => {
    expect(usageTone(74)).toBe("normal");
    expect(usageTone(75)).toBe("warning");
    expect(usageTone(89)).toBe("warning");
    expect(usageTone(90)).toBe("critical");
  });
});

describe("formatResetCountdown", () => {
  const now = 1_790_731_000_000;

  it("counts down in the largest unit that still says something", () => {
    expect(formatResetCountdown(1_790_731_000 + 40 * 60, now)).toBe("in 40m");
    expect(formatResetCountdown(1_790_731_000 + 2 * 3600 + 14 * 60, now)).toBe(
      "in 2h 14m",
    );
    expect(formatResetCountdown(1_790_731_000 + 50 * 3600, now)).toBe(
      "in 2d 2h",
    );
  });

  it("does not run backwards once the window has turned over", () => {
    expect(formatResetCountdown(1_790_731_000 - 60, now)).toBe("resetting now");
  });
});

describe("formatSnapshotAge", () => {
  const now = 1_790_731_000_000;

  it("says the read never happened rather than dating it to the epoch", () => {
    expect(formatSnapshotAge(null, now)).toBe("never read");
  });

  it("ages the read in the unit that matches how stale it is", () => {
    expect(formatSnapshotAge(1_790_731_000 - 10, now)).toBe("just now");
    expect(formatSnapshotAge(1_790_731_000 - 3 * 60, now)).toBe("3 min ago");
    expect(formatSnapshotAge(1_790_731_000 - 5 * 3600, now)).toBe("5 h ago");
    expect(formatSnapshotAge(1_790_731_000 - 3 * 86400, now)).toBe("3 d ago");
  });
});
