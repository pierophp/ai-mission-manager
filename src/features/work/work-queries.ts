import { queryOptions, useQuery } from "@tanstack/react-query";

import { activityAdapter, workAdapter } from "../../runtime/adapters";
import { currentMinute } from "../../runtime/time";
import type { ItemView } from "../../runtime/types";

export const workKeys = {
  all: ["work"] as const,
  home: (contextId: number | undefined) =>
    [...workKeys.all, "home", { contextId: contextId ?? null }] as const,
  search: (query: string, contextId: number | undefined) =>
    [...workKeys.all, "search", { query, contextId: contextId ?? null }] as const,
  runSuggestions: () => [...workKeys.all, "runSuggestions"] as const,
};

export const homeQueryOptions = (contextId: number | undefined) =>
  queryOptions({
    queryKey: workKeys.home(contextId),
    queryFn: () => workAdapter.getHome(contextId, currentMinute()),
    refetchInterval: 3_000,
    staleTime: 2_000,
  });

export const searchQueryOptions = (
  query: string,
  contextId: number | undefined,
) =>
  queryOptions({
    queryKey: workKeys.search(query, contextId),
    queryFn: () => workAdapter.searchItems(query, contextId),
    enabled: Boolean(query.trim()),
    staleTime: 30_000,
    select: (items: ItemView[]) => items,
  });

export const runSuggestionsQueryOptions = () =>
  queryOptions({
    queryKey: workKeys.runSuggestions(),
    queryFn: workAdapter.listRunSuggestions,
    refetchInterval: 10_000,
    staleTime: 5_000,
  });

export const activityKeys = {
  all: ["activity"] as const,
  tab: () => [...activityKeys.all, "tab"] as const,
};

export const activityQueryOptions = () =>
  queryOptions({
    queryKey: activityKeys.tab(),
    queryFn: activityAdapter.getTab,
    staleTime: 30_000,
  });

export function useHomeQuery(contextId: number | undefined) {
  return useQuery(homeQueryOptions(contextId));
}

export function useSearchQuery(query: string, contextId: number | undefined) {
  return useQuery(searchQueryOptions(query, contextId));
}

export function useRunSuggestionsQuery() {
  return useQuery(runSuggestionsQueryOptions());
}

export function useActivityQuery() {
  return useQuery(activityQueryOptions());
}
