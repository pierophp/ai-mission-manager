import type { QueryClient } from "@tanstack/react-query";

import { activityKeys, workKeys } from "../features/work/work-queries";
import { setupKeys } from "../features/setup/setup-queries";
import { structureKeys } from "../features/structure/structure-queries";

export async function invalidateWorkQueries(queryClient: QueryClient): Promise<void> {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: workKeys.all }),
    queryClient.invalidateQueries({ queryKey: activityKeys.all }),
  ]);
}

export async function invalidateRunQueries(queryClient: QueryClient): Promise<void> {
  await Promise.all([
    invalidateWorkQueries(queryClient),
    queryClient.invalidateQueries({ queryKey: workKeys.runSuggestions() }),
  ]);
}

export async function invalidateStructureQueries(queryClient: QueryClient): Promise<void> {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: structureKeys.all }),
    invalidateWorkQueries(queryClient),
  ]);
}

export async function invalidateSetupQueries(queryClient: QueryClient): Promise<void> {
  await Promise.all([
    queryClient.invalidateQueries({ queryKey: setupKeys.all }),
    invalidateStructureQueries(queryClient),
  ]);
}
