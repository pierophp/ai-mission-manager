import { queryOptions, useQuery } from "@tanstack/react-query";

import { command } from "../../runtime/command";
import type { ProviderChoice } from "../../runtime/types";

export const setupKeys = {
  all: ["setup"] as const,
  state: () => [...setupKeys.all, "state"] as const,
  health: (provider: ProviderChoice | null) =>
    [...setupKeys.all, "health", provider ?? "auto"] as const,
};

export const setupStateQueryOptions = () =>
  queryOptions({
    queryKey: setupKeys.state(),
    queryFn: () => command("getSetupState"),
    staleTime: 5 * 60 * 1000,
  });

export const healthStatusQueryOptions = (provider: ProviderChoice | null) =>
  queryOptions({
    queryKey: setupKeys.health(provider),
    queryFn: () => command("getHealthStatus", provider),
    staleTime: 60 * 1000,
  });

export function useSetupStateQuery() {
  return useQuery(setupStateQueryOptions());
}

export function useHealthStatusQuery(provider: ProviderChoice | null) {
  return useQuery(healthStatusQueryOptions(provider));
}
