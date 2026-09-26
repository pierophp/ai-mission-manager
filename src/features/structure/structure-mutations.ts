import { useMutation, useQueryClient } from "@tanstack/react-query";

import { structureAdapter } from "../../runtime/adapters";
import { invalidateStructureQueries } from "../../runtime/query-invalidation";

export type StructureAction<TData> = () => Promise<TData>;

export const structureActions = {
  createContext: (name: string) => () => structureAdapter.createContext(name),
  updateContext: (contextId: number, name: string) =>
    () => structureAdapter.updateContext(contextId, name),
  setContextExecutionMachine: (contextId: number, machineId: number | null) =>
    () => structureAdapter.setContextExecutionMachine(contextId, machineId),
  createProject: (
    name: string,
    contextId: number,
    defaultItemStatus: Parameters<typeof structureAdapter.createProject>[2],
    executionMode: Parameters<typeof structureAdapter.createProject>[3] = "worktree",
  ) => () => structureAdapter.createProject(name, contextId, defaultItemStatus, executionMode),
  updateProject: (
    projectId: number,
    name: string,
    defaultItemStatus: Parameters<typeof structureAdapter.updateProject>[2],
    executionMode: Parameters<typeof structureAdapter.updateProject>[3] = "worktree",
  ) => () => structureAdapter.updateProject(projectId, name, defaultItemStatus, executionMode),
  prepareProjectDeletion: (projectId: number) =>
    () => structureAdapter.prepareProjectDeletion(projectId),
  prepareContextDeletion: (contextId: number) =>
    () => structureAdapter.prepareContextDeletion(contextId),
  deleteProject: (
    projectId: number,
    itemIds: number[],
    repositoryIds: number[],
    workspaceIds: number[],
  ) =>
    () =>
      structureAdapter.deleteProject(
        projectId,
        itemIds,
        repositoryIds,
        workspaceIds,
      ),
  deleteContext: (input: Parameters<typeof structureAdapter.deleteContext>[0]) =>
    () => structureAdapter.deleteContext(input),
  registerRepository: (projectId: number, name: string, remoteUrl: string) =>
    () => structureAdapter.registerRepository(projectId, name, remoteUrl),
  registerRepositoryAtLocation: (input: Parameters<typeof structureAdapter.registerRepositoryAtLocation>[0]) =>
    () => structureAdapter.registerRepositoryAtLocation(input),
  updateRepositoryLocation: (input: Parameters<typeof structureAdapter.updateRepositoryLocation>[0]) =>
    () => structureAdapter.updateRepositoryLocation(input),
  updateRepository: (
    repositoryId: number,
    name: string,
    remoteUrl: string,
    baseBranch: string,
  ) => () => structureAdapter.updateRepository(repositoryId, name, remoteUrl, baseBranch),
  prepareRepositoryDeletion: (repositoryId: number) =>
    () => structureAdapter.prepareRepositoryDeletion(repositoryId),
  deleteRepository: (
    repositoryId: number,
    workspaceIds: number[],
  ) =>
    () =>
      structureAdapter.deleteRepository(
        repositoryId,
        workspaceIds,
      ),
  registerMachine: (
    contextId: number,
    name: string,
    socketName: string,
    transport: Parameters<typeof structureAdapter.registerMachine>[3],
  ) => () => structureAdapter.registerMachine(contextId, name, socketName, transport),
  updateMachine: (
    machineId: number,
    name: string,
    socketName: string,
    transport: Parameters<typeof structureAdapter.updateMachine>[3],
  ) => () => structureAdapter.updateMachine(machineId, name, socketName, transport),
  checkMachine: (machineId: number) => () => structureAdapter.checkMachine(machineId),
  prepareMachineDeletion: (machineId: number) =>
    () => structureAdapter.prepareMachineDeletion(machineId),
  deleteMachine: (
    machineId: number,
    runIds: number[],
    worktreeIds: number[],
    repositoryLocationRepositoryIds: number[],
  ) =>
    () =>
      structureAdapter.deleteMachine(
        machineId,
        runIds,
        worktreeIds,
        repositoryLocationRepositoryIds,
      ),
  deleteRun: (runId: number) => () => structureAdapter.deleteRun(runId),
  setAttentionDefault: (
    contextId: number,
    objectKind: Parameters<typeof structureAdapter.setAttentionDefault>[1],
    policy: Parameters<typeof structureAdapter.setAttentionDefault>[2],
  ) => () => structureAdapter.setAttentionDefault(contextId, objectKind, policy),
  setContextGrillDefaults: (
    contextId: number,
    defaults: Parameters<typeof structureAdapter.setContextGrillDefaults>[1],
  ) => () => structureAdapter.setContextGrillDefaults(contextId, defaults),
  setContextImplementDefaults: (contextId: number, defaults: Parameters<typeof structureAdapter.setContextImplementDefaults>[1]) =>
    () => structureAdapter.setContextImplementDefaults(contextId, defaults),
  createItem: (title: string, contextId: number, projectId: number) =>
    () => structureAdapter.createItem(title, contextId, projectId),
  prepareReset: () => () => structureAdapter.prepareReset(),
  reset: (confirmation: string) => () => structureAdapter.reset(confirmation),
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
