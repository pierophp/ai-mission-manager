import { useMutation, useQueryClient } from "@tanstack/react-query";

import { command } from "../../runtime/command";
import { invalidateSetupQueries } from "../../runtime/query-invalidation";
import { healthStatusQueryOptions, setupKeys } from "./setup-queries";
import type { ProviderChoice } from "../../runtime/types";

export function useCompleteSetupMutation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ contextName, provider }: { contextName: string; provider: ProviderChoice }) =>
      command("completeSetup", contextName, provider),
    onSuccess: async (completed) => {
      queryClient.setQueryData(setupKeys.state(), completed);
      await invalidateSetupQueries(queryClient);
    },
  });
}

export function useHealthCheckMutation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (provider: ProviderChoice | null) => command("getHealthStatus", provider),
    onSuccess: (health, provider) => {
      queryClient.setQueryData(healthStatusQueryOptions(provider).queryKey, health);
    },
  });
}
