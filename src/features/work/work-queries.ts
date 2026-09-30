import { queryOptions, useQuery } from "@tanstack/react-query";

import { workCommands } from "./work-commands";
import { currentMinute } from "../../runtime/time";
import type { ItemView } from "../../runtime/types";

export const workKeys = {
  all: ["work"] as const,
  home: (contextId: number | undefined) =>
    [...workKeys.all, "home", { contextId: contextId ?? null }] as const,
  search: (query: string, contextId: number | undefined) =>
    [
      ...workKeys.all,
      "search",
      { query, contextId: contextId ?? null },
    ] as const,
  runSuggestions: () => [...workKeys.all, "runSuggestions"] as const,
};

export const homeQueryOptions = (contextId: number | undefined) =>
  queryOptions({
    queryKey: workKeys.home(contextId),
    queryFn: () => workCommands.getHome(contextId, currentMinute()),
    refetchInterval: 3_000,
    staleTime: 2_000,
  });

export const searchQueryOptions = (
  query: string,
  contextId: number | undefined,
) =>
  queryOptions({
    queryKey: workKeys.search(query, contextId),
    queryFn: () => workCommands.searchItems(query, contextId),
    enabled: Boolean(query.trim()),
    staleTime: 30_000,
    select: (items: ItemView[]) => items,
  });

export const runSuggestionsQueryOptions = () =>
  queryOptions({
    queryKey: workKeys.runSuggestions(),
    queryFn: workCommands.listRunSuggestions,
    refetchInterval: 10_000,
    staleTime: 5_000,
  });

/**
 * Specs are read from their provider when the Spec tab opens. They live outside
 * `workKeys` so saving an Item does not refetch them.
 */
export const issueDocumentQueryOptions = (externalObjectId: number) =>
  queryOptions({
    queryKey: ["issueDocument", externalObjectId] as const,
    queryFn: () => workCommands.fetchIssueDocument(externalObjectId),
    staleTime: 0,
    refetchOnMount: "always" as const,
    retry: false,
  });

export const externalDocumentQueryOptions = (
  externalObjectId: number,
  enabled = true,
) =>
  queryOptions({
    queryKey: ["externalDocument", externalObjectId] as const,
    queryFn: () => workCommands.fetchExternalDocument(externalObjectId),
    staleTime: 0,
    refetchOnMount: "always" as const,
    retry: false,
    enabled,
  });

export const externalCommentsQueryOptions = (
  externalObjectId: number,
  enabled = true,
) =>
  queryOptions({
    queryKey: ["externalComments", externalObjectId] as const,
    queryFn: () => workCommands.fetchExternalComments(externalObjectId),
    staleTime: 0,
    refetchOnMount: "always" as const,
    retry: false,
    enabled,
  });

export function useIssueDocumentQuery(externalObjectId: number) {
  return useQuery(issueDocumentQueryOptions(externalObjectId));
}

export function useExternalDocumentQuery(
  externalObjectId: number,
  enabled = true,
) {
  return useQuery(externalDocumentQueryOptions(externalObjectId, enabled));
}

export function useExternalCommentsQuery(
  externalObjectId: number,
  enabled = true,
) {
  return useQuery(externalCommentsQueryOptions(externalObjectId, enabled));
}

export const activityKeys = {
  all: ["activity"] as const,
  tab: () => [...activityKeys.all, "tab"] as const,
};

export const activityQueryOptions = () =>
  queryOptions({
    queryKey: activityKeys.tab(),
    queryFn: workCommands.getActivityTab,
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
