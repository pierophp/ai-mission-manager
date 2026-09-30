import { useEffect, useState } from "react";
import { GaugeIcon, RefreshCwIcon } from "lucide-react";

import { Badge } from "../../components/ui/badge";
import { Button } from "../../components/ui/button";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "../../components/ui/popover";
import { Progress } from "../../components/ui/progress";
import type {
  PlanUsageSnapshot,
  ProfilePlanUsage,
  UsageWindow,
} from "../../runtime/types";
import {
  usePlanUsageQuery,
  useRefreshPlanUsageMutation,
} from "./plan-usage-queries";
import {
  formatResetClock,
  formatResetCountdown,
  formatSnapshotAge,
  hasBlindProfile,
  profileStateLabel,
  providerLabel,
  usageTone,
  worstUsedPercent,
  type UsageTone,
} from "./plan-usage-format";

const toneTextClass: Record<UsageTone, string> = {
  normal: "text-muted-foreground",
  warning: "text-amber-600 dark:text-amber-400",
  critical: "text-destructive",
};

const toneBarClass: Record<UsageTone, string> = {
  normal: "[&_[data-slot=progress-indicator]]:bg-primary",
  warning: "[&_[data-slot=progress-indicator]]:bg-amber-500",
  critical: "[&_[data-slot=progress-indicator]]:bg-destructive",
};

export function PlanUsageButton() {
  const usageQuery = usePlanUsageQuery();
  const refreshMutation = useRefreshPlanUsageMutation();
  const snapshot = usageQuery.data;
  const now = useCurrentTime();
  const worst = worstUsedPercent(snapshot);
  const tone = worst === null ? "normal" : usageTone(worst);

  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="h-auto min-h-8 shrink-0 gap-2 px-2 text-xs text-muted-foreground hover:bg-secondary hover:text-primary"
          aria-label={
            worst === null
              ? "Plan usage. No usage reported yet"
              : `Plan usage. Highest window at ${Math.round(worst)} percent`
          }
        >
          <GaugeIcon aria-hidden="true" className="size-3.5 shrink-0" />
          <span className={toneTextClass[tone]}>
            {worst === null ? "—" : `${Math.round(worst)}%`}
          </span>
          {hasBlindProfile(snapshot) ? (
            <span
              aria-hidden="true"
              className="size-1.5 shrink-0 rounded-full bg-muted-foreground/60"
            />
          ) : null}
        </Button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-96 gap-3">
        <PlanUsagePanel
          snapshot={snapshot}
          now={now}
          error={usageQuery.error}
          isRefreshing={
            snapshot?.status === "refreshing" || refreshMutation.isPending
          }
          onRefresh={() => refreshMutation.mutate()}
        />
      </PopoverContent>
    </Popover>
  );
}

function PlanUsagePanel({
  snapshot,
  now,
  error,
  isRefreshing,
  onRefresh,
}: {
  snapshot: PlanUsageSnapshot | undefined;
  now: number;
  error: unknown;
  isRefreshing: boolean;
  onRefresh: () => void;
}) {
  return (
    <>
      <div className="flex items-baseline justify-between gap-2">
        <span className="font-medium">Plan usage</span>
        <span className="text-xs text-muted-foreground">
          {isRefreshing
            ? "Reading…"
            : `Read ${formatSnapshotAge(snapshot?.fetchedAt ?? null, now)}`}
        </span>
      </div>

      {error ? (
        <p className="text-xs text-destructive">
          Plan usage could not be read.
        </p>
      ) : null}

      {snapshot && snapshot.profiles.length === 0 && !isRefreshing ? (
        <p className="text-xs text-muted-foreground">
          No Agent CLI Configuration Profile is set up yet. Add one in Settings
          to see its plan limits here.
        </p>
      ) : null}

      <div className="flex flex-col gap-3">
        {groupByMachine(snapshot?.profiles ?? []).map(
          ([machineName, profiles]) => (
            <div key={machineName} className="flex flex-col gap-2">
              <span className="text-xs font-medium text-muted-foreground">
                {machineName}
              </span>
              {profiles.map((profile) => (
                <ProfileRow
                  key={profile.profileId}
                  profile={profile}
                  now={now}
                />
              ))}
            </div>
          ),
        )}
      </div>

      <div className="flex justify-end">
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="h-7 gap-1.5 px-2 text-xs"
          disabled={isRefreshing}
          onClick={onRefresh}
        >
          <RefreshCwIcon aria-hidden="true" className="size-3" />
          Refresh
        </Button>
      </div>
    </>
  );
}

function ProfileRow({
  profile,
  now,
}: {
  profile: ProfilePlanUsage;
  now: number;
}) {
  return (
    <div className="flex flex-col gap-1.5 rounded-md bg-muted/40 p-2">
      <div className="flex items-baseline justify-between gap-2">
        <span className="truncate text-xs">
          {providerLabel(profile.provider)}
          <span className="text-muted-foreground">
            {" "}
            · {profile.profileName}
          </span>
        </span>
        {profile.plan ? (
          <Badge variant="secondary" className="shrink-0 text-[10px] uppercase">
            {profile.plan}
          </Badge>
        ) : null}
      </div>

      {profile.state === "ready" ? (
        profile.windows.map((window) => (
          <WindowBar key={window.id} window={window} now={now} />
        ))
      ) : (
        <p className="text-xs text-muted-foreground">
          {profileStateLabel(profile.state)}
          {profile.detail ? ` — ${profile.detail}` : ""}
        </p>
      )}

      {/* Claude reports numbers a past session cached, so its own age is not
          the age of our read and has to be stated separately. */}
      {profile.state === "ready" && profile.observedAt !== null ? (
        <span className="text-[10px] text-muted-foreground">
          Reported by the CLI {formatSnapshotAge(profile.observedAt, now)}
        </span>
      ) : null}
    </div>
  );
}

function WindowBar({ window, now }: { window: UsageWindow; now: number }) {
  const tone = usageTone(window.usedPercent);
  return (
    <div className="flex flex-col gap-1">
      <div className="flex items-baseline justify-between gap-2 text-xs">
        <span className="truncate text-muted-foreground">{window.label}</span>
        <span className={toneTextClass[tone]}>
          {Math.round(window.usedPercent)}%
        </span>
      </div>
      <Progress value={window.usedPercent} className={toneBarClass[tone]} />
      {window.resetsAt === null ? null : (
        <span className="text-[10px] text-muted-foreground">
          Resets {formatResetCountdown(window.resetsAt, now)}
          <span className="text-muted-foreground/60">
            {" "}
            · {formatResetClock(window.resetsAt, now)}
          </span>
        </span>
      )}
    </div>
  );
}

function groupByMachine(profiles: ProfilePlanUsage[]) {
  const grouped = new Map<string, ProfilePlanUsage[]>();
  for (const profile of profiles) {
    const existing = grouped.get(profile.machineName);
    if (existing) existing.push(profile);
    else grouped.set(profile.machineName, [profile]);
  }
  return [...grouped];
}

// Countdowns have to keep counting down while the popover stays open.
function useCurrentTime() {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 30_000);
    return () => clearInterval(timer);
  }, []);
  return now;
}
