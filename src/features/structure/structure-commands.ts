import { useMutation, useQueryClient } from "@tanstack/react-query";

import type {
  ContextConfiguration,
  ContextAttentionDefault,
  MachineTransport,
  ItemStatus,
  ExecutionMode,
  GrillConfiguration,
} from "../../runtime/types";
import type { AgentKind } from "../../runtime/execution-types";
import { command } from "../../runtime/command";
import {
  normalizeContextConfiguration,
  normalizeMachine,
  normalizeMachines,
  toGeneratedMachineTransport,
} from "../../runtime/normalize-generated";

import { invalidateStructureQueries } from "../../runtime/query-invalidation";

export const structureCommands = {
  listContexts: () => command("listContexts"),
  listProjects: () => command("listProjects"),
  listRepositories: () => command("listRepositories"),
  listRepositoryLocations: () => command("listRepositoryLocations"),
  listMachines: async () => normalizeMachines(await command("listMachines")),
  listCliConfigurationProfiles: () => command("listCliConfigurationProfiles"),
  listAttentionDefaults: () =>
    command("listContextAttentionDefaults"),
  newContextConfiguration: async () =>
    normalizeContextConfiguration(await command("newContextConfiguration")),
  listGrillModelCatalog: () =>
    command("listGrillModelCatalog"),
  refreshGrillModelCatalog: () => command("refreshGrillModelCatalog"),
  createContext: (name: string) => command("createContext", name),
  createContextConfiguration: (configuration: ContextConfiguration) =>
    command("createContextConfiguration", configuration),
  updateContext: (contextId: number, name: string) =>
    command("updateContext", contextId, name),
  updateContextConfiguration: (contextId: number, configuration: ContextConfiguration) =>
    command("updateContextConfiguration", contextId, configuration),
  setContextExecutionMachine: (contextId: number, machineId: number | null) =>
    command("setContextExecutionMachine", contextId, machineId),
  createCliConfigurationProfile: (input: { machineId: number; provider: AgentKind; name: string; appManaged: boolean; existingDirectory: string | null }) =>
    command("createCliConfigurationProfile", input.machineId, input.provider, input.name, input.appManaged, input.existingDirectory),
  setContextCliConfigurationProfile: (contextId: number, provider: AgentKind, profileId: number | null) =>
    command("setContextCliConfigurationProfile", contextId, provider, profileId),
  deleteCliConfigurationProfile: (profileId: number) =>
    command("deleteCliConfigurationProfile", profileId),
  createProject: (
    name: string,
    contextId: number,
    defaultItemStatus: ItemStatus,
    executionMode: ExecutionMode = "worktree",
  ) =>
    command("createProject", name, contextId, defaultItemStatus, executionMode),
  updateProject: (
    projectId: number,
    name: string,
    defaultItemStatus: ItemStatus,
    executionMode: ExecutionMode = "worktree",
  ) =>
    command("updateProject", projectId, name, defaultItemStatus, executionMode),
  prepareProjectDeletion: (projectId: number) =>
    command("prepareProjectDeletion", projectId),
  prepareContextDeletion: (contextId: number) =>
    command("prepareContextDeletion", contextId),
  prepareReset: () => command("prepareResetLocalData"),
  reset: (confirmation: string) =>
    command("resetAllLocalData", confirmation),
  deleteProject: (
    projectId: number,
    itemIds: number[],
    repositoryIds: number[],
    workspaceIds: number[],
  ) =>
    command("deleteProject", projectId, itemIds, repositoryIds, workspaceIds, true),
  deleteContext: ({
    contextId,
    projectIds,
    itemIds,
    repositoryIds,
    workspaceIds,
    machineIds,
  }: {
    contextId: number;
    projectIds: number[];
    itemIds: number[];
    repositoryIds: number[];
    workspaceIds: number[];
    machineIds: number[];
  }) =>
    command("deleteContext", contextId, projectIds, itemIds, repositoryIds, workspaceIds, machineIds, true),
  registerRepository: (projectId: number, name: string, remoteUrl: string) =>
    command("registerRepository", projectId, name, remoteUrl),
  updateRepository: (
    repositoryId: number,
    name: string,
    remoteUrl: string,
    baseBranch: string,
  ) =>
    command("updateRepository", repositoryId, name, remoteUrl, baseBranch),
  registerRepositoryAtLocation: (input: {
    projectId: number;
    name: string;
    remoteUrl: string | null;
    baseBranch: string;
    machineId: number;
    checkoutPath: string;
    worktreeRoot: string | null;
    cloneIntoDestination: boolean;
  }) => command("registerRepositoryAtLocation", input.projectId, input.name, input.remoteUrl, input.baseBranch, input.machineId, input.checkoutPath, input.worktreeRoot, input.cloneIntoDestination),
  updateRepositoryLocation: (input: {
    repositoryId: number;
    previousMachineId: number | null;
    machineId: number;
    checkoutPath: string;
    worktreeRoot: string;
  }) => command("updateRepositoryLocation", input.repositoryId, input.previousMachineId, input.machineId, input.checkoutPath, input.worktreeRoot),
  prepareRepositoryDeletion: (repositoryId: number) =>
    command("prepareRepositoryDeletion", repositoryId),
  deleteRepository: (
    repositoryId: number,
    workspaceIds: number[],
  ) =>
    command("deleteRepository", repositoryId, workspaceIds, true),
  prepareMachineDeletion: (machineId: number) =>
    command("prepareMachineDeletion", machineId),
  deleteMachine: (
    machineId: number,
    runIds: number[],
    worktreeIds: number[],
    repositoryLocationRepositoryIds: number[],
  ) =>
    command("deleteMachine", machineId, runIds, worktreeIds, repositoryLocationRepositoryIds, true),
  deleteRun: (runId: number) =>
    command("deleteRun", runId, true),
  registerMachine: (
    contextId: number,
    name: string,
    socketName: string,
    transport: MachineTransport,
  ) =>
    command("registerMachine", contextId, name, socketName, toGeneratedMachineTransport(transport)).then(normalizeMachine),
  updateMachine: (
    machineId: number,
    name: string,
    socketName: string,
    transport: MachineTransport,
  ) =>
    command("updateMachine", machineId, name, socketName, toGeneratedMachineTransport(transport)).then(normalizeMachine),
  checkMachine: (machineId: number) =>
    command("checkMachine", machineId).then(normalizeMachine),
  setAttentionDefault: (
    contextId: number,
    objectKind: ContextAttentionDefault["object_kind"],
    policy: ContextAttentionDefault["policy"],
  ) =>
    command("setContextAttentionDefault", contextId, objectKind, policy),
  setContextGrillDefaults: (contextId: number, defaults: GrillConfiguration) =>
    command("setContextGrillDefaults", contextId, defaults),
  setContextImplementDefaults: (contextId: number, defaults: GrillConfiguration) =>
    command("setContextImplementDefaults", contextId, defaults),
  setContextDirtyCheckoutCheck: (contextId: number, enabled: boolean) =>
    command("setContextDirtyCheckoutCheck", contextId, enabled),
  createItem: (
    title: string,
    contextId: number,
    projectId: number,
    notes: string,
  ) => command("createItem", title, contextId, projectId, notes),
};


export type StructureAction<TData> = () => Promise<TData>;

export const structureActions = {
  createContext: (name: string) => () => structureCommands.createContext(name),
  createContextConfiguration: (
    configuration: Parameters<typeof structureCommands.createContextConfiguration>[0],
  ) => () => structureCommands.createContextConfiguration(configuration),
  updateContext: (contextId: number, name: string) =>
    () => structureCommands.updateContext(contextId, name),
  updateContextConfiguration: (
    contextId: number,
    configuration: Parameters<typeof structureCommands.updateContextConfiguration>[1],
  ) => () => structureCommands.updateContextConfiguration(contextId, configuration),
  setContextExecutionMachine: (contextId: number, machineId: number | null) =>
    () => structureCommands.setContextExecutionMachine(contextId, machineId),
  createCliConfigurationProfile: (input: Parameters<typeof structureCommands.createCliConfigurationProfile>[0]) =>
    () => structureCommands.createCliConfigurationProfile(input),
  setContextCliConfigurationProfile: (contextId: number, provider: Parameters<typeof structureCommands.setContextCliConfigurationProfile>[1], profileId: number | null) =>
    () => structureCommands.setContextCliConfigurationProfile(contextId, provider, profileId),
  deleteCliConfigurationProfile: (profileId: number) =>
    () => structureCommands.deleteCliConfigurationProfile(profileId),
  createProject: (
    name: string,
    contextId: number,
    defaultItemStatus: Parameters<typeof structureCommands.createProject>[2],
    executionMode: Parameters<typeof structureCommands.createProject>[3] = "worktree",
  ) => () => structureCommands.createProject(name, contextId, defaultItemStatus, executionMode),
  updateProject: (
    projectId: number,
    name: string,
    defaultItemStatus: Parameters<typeof structureCommands.updateProject>[2],
    executionMode: Parameters<typeof structureCommands.updateProject>[3] = "worktree",
  ) => () => structureCommands.updateProject(projectId, name, defaultItemStatus, executionMode),
  prepareProjectDeletion: (projectId: number) =>
    () => structureCommands.prepareProjectDeletion(projectId),
  prepareContextDeletion: (contextId: number) =>
    () => structureCommands.prepareContextDeletion(contextId),
  deleteProject: (
    projectId: number,
    itemIds: number[],
    repositoryIds: number[],
    workspaceIds: number[],
  ) =>
    () =>
      structureCommands.deleteProject(
        projectId,
        itemIds,
        repositoryIds,
        workspaceIds,
      ),
  deleteContext: (input: Parameters<typeof structureCommands.deleteContext>[0]) =>
    () => structureCommands.deleteContext(input),
  registerRepository: (projectId: number, name: string, remoteUrl: string) =>
    () => structureCommands.registerRepository(projectId, name, remoteUrl),
  registerRepositoryAtLocation: (input: Parameters<typeof structureCommands.registerRepositoryAtLocation>[0]) =>
    () => structureCommands.registerRepositoryAtLocation(input),
  updateRepositoryLocation: (input: Parameters<typeof structureCommands.updateRepositoryLocation>[0]) =>
    () => structureCommands.updateRepositoryLocation(input),
  updateRepository: (
    repositoryId: number,
    name: string,
    remoteUrl: string,
    baseBranch: string,
  ) => () => structureCommands.updateRepository(repositoryId, name, remoteUrl, baseBranch),
  prepareRepositoryDeletion: (repositoryId: number) =>
    () => structureCommands.prepareRepositoryDeletion(repositoryId),
  deleteRepository: (
    repositoryId: number,
    workspaceIds: number[],
  ) =>
    () =>
      structureCommands.deleteRepository(
        repositoryId,
        workspaceIds,
      ),
  registerMachine: (
    contextId: number,
    name: string,
    socketName: string,
    transport: Parameters<typeof structureCommands.registerMachine>[3],
  ) => () => structureCommands.registerMachine(contextId, name, socketName, transport),
  updateMachine: (
    machineId: number,
    name: string,
    socketName: string,
    transport: Parameters<typeof structureCommands.updateMachine>[3],
  ) => () => structureCommands.updateMachine(machineId, name, socketName, transport),
  checkMachine: (machineId: number) => () => structureCommands.checkMachine(machineId),
  prepareMachineDeletion: (machineId: number) =>
    () => structureCommands.prepareMachineDeletion(machineId),
  deleteMachine: (
    machineId: number,
    runIds: number[],
    worktreeIds: number[],
    repositoryLocationRepositoryIds: number[],
  ) =>
    () =>
      structureCommands.deleteMachine(
        machineId,
        runIds,
        worktreeIds,
        repositoryLocationRepositoryIds,
      ),
  deleteRun: (runId: number) => () => structureCommands.deleteRun(runId),
  setAttentionDefault: (
    contextId: number,
    objectKind: Parameters<typeof structureCommands.setAttentionDefault>[1],
    policy: Parameters<typeof structureCommands.setAttentionDefault>[2],
  ) => () => structureCommands.setAttentionDefault(contextId, objectKind, policy),
  setContextGrillDefaults: (
    contextId: number,
    defaults: Parameters<typeof structureCommands.setContextGrillDefaults>[1],
  ) => () => structureCommands.setContextGrillDefaults(contextId, defaults),
  setContextImplementDefaults: (contextId: number, defaults: Parameters<typeof structureCommands.setContextImplementDefaults>[1]) =>
    () => structureCommands.setContextImplementDefaults(contextId, defaults),
  setContextDirtyCheckoutCheck: (contextId: number, enabled: boolean) =>
    () => structureCommands.setContextDirtyCheckoutCheck(contextId, enabled),
  createItem: (
    title: string,
    contextId: number,
    projectId: number,
    notes: string,
  ) => () => structureCommands.createItem(title, contextId, projectId, notes),
  prepareReset: () => () => structureCommands.prepareReset(),
  reset: (confirmation: string) => () => structureCommands.reset(confirmation),
};

export function useStructureCommand() {
  const queryClient = useQueryClient();
  const mutation = useMutation({
    mutationFn: ({ action }: { action: StructureAction<unknown>; invalidate: boolean }) =>
      action(),
    onSuccess: async (_, variables) => {
      if (variables.invalidate) await invalidateStructureQueries(queryClient);
    },
  });

  return {
    isPending: mutation.isPending,
    execute<TData>(action: StructureAction<TData>, invalidate = true) {
      return mutation.mutateAsync({ action, invalidate }) as Promise<TData>;
    },
  };
}
