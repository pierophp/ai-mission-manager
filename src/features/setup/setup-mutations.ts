import { useMutation, useQueryClient } from "@tanstack/react-query";

import { setupAdapter } from "../../runtime/adapters";
import { invalidateSetupQueries } from "../../runtime/query-invalidation";
import { healthStatusQueryOptions, setupKeys } from "./setup-queries";
import type { ProviderChoice } from "../../runtime/types";

export function useCompleteSetupMutation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: ({ contextName, provider }: { contextName: string; provider: ProviderChoice }) =>
      setupAdapter.complete(contextName, provider),
    onSuccess: async (completed) => {
      queryClient.setQueryData(setupKeys.state(), completed);
      await invalidateSetupQueries(queryClient);
    },
  });
}

export function useHealthCheckMutation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (provider: ProviderChoice | null) => setupAdapter.getHealth(provider),
    onSuccess: (health, provider) => {
      queryClient.setQueryData(healthStatusQueryOptions(provider).queryKey, health);
    },
  });
}
