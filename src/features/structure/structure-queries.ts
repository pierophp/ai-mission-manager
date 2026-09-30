import { queryOptions, useQuery } from "@tanstack/react-query";

import { structureAdapter } from "../../runtime/adapters";
import type {
  Context,
  CliProfileSettingsView,
  ContextAttentionDefault,
  GrillAgentCatalog,
  GrillModelCatalogSnapshot,
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
  cliConfigurationProfiles: CliProfileSettingsView[];
  attentionDefaults: ContextAttentionDefault[];
  grillModelCatalog: GrillAgentCatalog[];
  grillModelCatalogStatus: GrillModelCatalogSnapshot["codexStatus"];
  grillModelCatalogError: string | null;
};

export const structureKeys = {
  all: ["structure"] as const,
  contexts: () => [...structureKeys.all, "contexts"] as const,
  projects: () => [...structureKeys.all, "projects"] as const,
  repositories: () => [...structureKeys.all, "repositories"] as const,
  repositoryLocations: () => [...structureKeys.all, "repositoryLocations"] as const,
  machines: () => [...structureKeys.all, "machines"] as const,
  cliConfigurationProfiles: () => [...structureKeys.all, "cliConfigurationProfiles"] as const,
  attentionDefaults: () => [...structureKeys.all, "attentionDefaults"] as const,
  grillModelCatalog: () => [...structureKeys.all, "grillModelCatalog"] as const,
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
  cliConfigurationProfiles: () =>
    queryOptions({ queryKey: structureKeys.cliConfigurationProfiles(), queryFn: structureAdapter.listCliConfigurationProfiles, staleTime: structureStaleTime }),
  attentionDefaults: () =>
    queryOptions({
      queryKey: structureKeys.attentionDefaults(),
      queryFn: structureAdapter.listAttentionDefaults,
      staleTime: structureStaleTime,
    }),
  grillModelCatalog: () =>
    queryOptions({
      queryKey: structureKeys.grillModelCatalog(),
      queryFn: structureAdapter.listGrillModelCatalog,
      staleTime: structureStaleTime,
      refetchInterval: (query) =>
        query.state.data?.codexStatus === "refreshing" ? 2_000 : 60 * 60 * 1000,
    }),
};

export function useStructureData() {
  const contexts = useQuery(structureQueryOptions.contexts());
  const projects = useQuery(structureQueryOptions.projects());
  const repositories = useQuery(structureQueryOptions.repositories());
  const repositoryLocations = useQuery(structureQueryOptions.repositoryLocations());
  const machines = useQuery(structureQueryOptions.machines());
  const cliConfigurationProfiles = useQuery(structureQueryOptions.cliConfigurationProfiles());
  const attentionDefaults = useQuery(structureQueryOptions.attentionDefaults());
  const grillModelCatalog = useQuery(structureQueryOptions.grillModelCatalog());

  return {
    data: {
      contexts: contexts.data ?? [],
      projects: projects.data ?? [],
      repositories: repositories.data ?? [],
      repositoryLocations: repositoryLocations.data ?? [],
      machines: machines.data ?? [],
      cliConfigurationProfiles: cliConfigurationProfiles.data ?? [],
      attentionDefaults: attentionDefaults.data ?? [],
      grillModelCatalog: grillModelCatalog.data?.catalogs ?? [],
      grillModelCatalogStatus: grillModelCatalog.data?.codexStatus ?? "refreshing",
      grillModelCatalogError: grillModelCatalog.data?.codexError ?? null,
    } satisfies StructureData,
    isPending: [contexts, projects, repositories, repositoryLocations, machines, cliConfigurationProfiles, attentionDefaults, grillModelCatalog].some(
      (query) => query.isPending,
    ),
    error:
      contexts.error ??
      projects.error ??
      repositories.error ??
      repositoryLocations.error ??
      machines.error ??
      cliConfigurationProfiles.error ??
      attentionDefaults.error ??
      grillModelCatalog.error,
    retryGrillModelCatalog: async () => {
      await structureAdapter.refreshGrillModelCatalog();
      await grillModelCatalog.refetch();
    },
  };
}
