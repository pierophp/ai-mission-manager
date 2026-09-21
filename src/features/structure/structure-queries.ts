import { queryOptions, useQuery } from "@tanstack/react-query";

import { structureAdapter } from "../../runtime/adapters";
import type {
  Context,
  ContextAttentionDefault,
  Machine,
  Project,
  Repository,
  RepositoryLocation,
} from "../../runtime/types";

export type StructureData = {
  contexts: Context[];
  projects: Project[];
  repositories: Repository[];
  repositoryLocations: RepositoryLocation[];
  machines: Machine[];
  attentionDefaults: ContextAttentionDefault[];
};

export const structureKeys = {
  all: ["structure"] as const,
  contexts: () => [...structureKeys.all, "contexts"] as const,
  projects: () => [...structureKeys.all, "projects"] as const,
  repositories: () => [...structureKeys.all, "repositories"] as const,
  repositoryLocations: () => [...structureKeys.all, "repositoryLocations"] as const,
  machines: () => [...structureKeys.all, "machines"] as const,
  attentionDefaults: () => [...structureKeys.all, "attentionDefaults"] as const,
};

const structureStaleTime = 5 * 60 * 1000;

export const structureQueryOptions = {
  contexts: () =>
    queryOptions({
      queryKey: structureKeys.contexts(),
      queryFn: structureAdapter.listContexts,
      staleTime: structureStaleTime,
    }),
  projects: () =>
    queryOptions({
      queryKey: structureKeys.projects(),
      queryFn: structureAdapter.listProjects,
      staleTime: structureStaleTime,
    }),
  repositories: () =>
    queryOptions({
      queryKey: structureKeys.repositories(),
      queryFn: structureAdapter.listRepositories,
      staleTime: structureStaleTime,
    }),
  repositoryLocations: () =>
    queryOptions({
      queryKey: structureKeys.repositoryLocations(),
      queryFn: structureAdapter.listRepositoryLocations,
      staleTime: structureStaleTime,
    }),
  machines: () =>
    queryOptions({
      queryKey: structureKeys.machines(),
      queryFn: structureAdapter.listMachines,
      staleTime: structureStaleTime,
    }),
  attentionDefaults: () =>
    queryOptions({
      queryKey: structureKeys.attentionDefaults(),
      queryFn: structureAdapter.listAttentionDefaults,
      staleTime: structureStaleTime,
    }),
};

export function useStructureData() {
  const contexts = useQuery(structureQueryOptions.contexts());
  const projects = useQuery(structureQueryOptions.projects());
  const repositories = useQuery(structureQueryOptions.repositories());
  const repositoryLocations = useQuery(structureQueryOptions.repositoryLocations());
  const machines = useQuery(structureQueryOptions.machines());
  const attentionDefaults = useQuery(structureQueryOptions.attentionDefaults());

  return {
    data: {
      contexts: contexts.data ?? [],
      projects: projects.data ?? [],
      repositories: repositories.data ?? [],
      repositoryLocations: repositoryLocations.data ?? [],
      machines: machines.data ?? [],
      attentionDefaults: attentionDefaults.data ?? [],
    } satisfies StructureData,
    isPending: [contexts, projects, repositories, repositoryLocations, machines, attentionDefaults].some(
      (query) => query.isPending,
    ),
    error:
      contexts.error ??
      projects.error ??
      repositories.error ??
      repositoryLocations.error ??
      machines.error ??
      attentionDefaults.error,
  };
}
