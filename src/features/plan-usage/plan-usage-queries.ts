import {
  queryOptions,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";

import { command } from "../../runtime/command";

export const planUsageKeys = {
  all: ["plan-usage"] as const,
  snapshot: () => [...planUsageKeys.all, "snapshot"] as const,
};

export const planUsageQueryOptions = () =>
  queryOptions({
    queryKey: planUsageKeys.snapshot(),
    queryFn: () => command("listPlanUsage"),
    // The backend refreshes in the background, so polling only decides how
    // soon a finished refresh becomes visible.
    refetchInterval: (query) =>
      query.state.data?.status === "refreshing" ? 2_000 : 60_000,
  });

export function usePlanUsageQuery() {
  return useQuery(planUsageQueryOptions());
}

export function useRefreshPlanUsageMutation() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => command("refreshPlanUsage"),
    onSuccess: () =>
      queryClient.invalidateQueries({ queryKey: planUsageKeys.all }),
  });
}
