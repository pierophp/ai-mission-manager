import type {
  Context,
  ContextAttentionDefault,
  Machine,
  MachineDeletionPreview,
  MachineDeletionResult,
  MachineTransport,
  ParentDeletionPreview,
  ParentDeletionResult,
  Project,
  Project as ProjectRecord,
  Repository,
  RepositoryLocation,
  RepositoryDeletionPreview,
  RepositoryDeletionResult,
  ResetLocalDataPreview,
  ResetLocalDataResult,
  RunDeletionResult,
  Item,
  ItemStatus,
  ExecutionMode,
} from "../types";
import { command } from "./tauri";

export const structureAdapter = {
  listContexts: () => command<Context[]>("list_contexts"),
  listProjects: () => command<Project[]>("list_projects"),
  listRepositories: () => command<Repository[]>("list_repositories"),
  listRepositoryLocations: () => command<RepositoryLocation[]>("list_repository_locations"),
  listMachines: () => command<Machine[]>("list_machines"),
  listAttentionDefaults: () =>
    command<ContextAttentionDefault[]>("list_context_attention_defaults"),
  createContext: (name: string) => command<Context>("create_context", { name }),
  createProject: (
    name: string,
    contextId: number,
    defaultItemStatus: ItemStatus,
    executionMode: ExecutionMode = "worktree",
  ) =>
    command<ProjectRecord>("create_project", {
      name,
      contextId,
      defaultItemStatus,
      executionMode,
    }),
  prepareProjectDeletion: (projectId: number) =>
    command<ParentDeletionPreview>("prepare_project_deletion", { projectId }),
  prepareContextDeletion: (contextId: number) =>
    command<ParentDeletionPreview>("prepare_context_deletion", { contextId }),
  prepareReset: () => command<ResetLocalDataPreview>("prepare_reset_local_data"),
  reset: (confirmation: string, deleteWorksetDirectories: boolean) =>
    command<ResetLocalDataResult>("reset_all_local_data", {
      confirmation,
      deleteWorksetDirectories,
    }),
  deleteProject: (
    projectId: number,
    itemIds: number[],
    repositoryIds: number[],
    worksetIds: number[],
    deleteWorksetDirectories: boolean,
  ) =>
    command<ParentDeletionResult>("delete_project", {
      projectId,
      itemIds,
      repositoryIds,
      worksetIds,
      confirmed: true,
      deleteWorksetDirectories,
    }),
  deleteContext: ({
    contextId,
    projectIds,
    itemIds,
    repositoryIds,
    worksetIds,
    machineIds,
    deleteWorksetDirectories,
  }: {
    contextId: number;
    projectIds: number[];
    itemIds: number[];
    repositoryIds: number[];
    worksetIds: number[];
    machineIds: number[];
    deleteWorksetDirectories: boolean;
  }) =>
    command<ParentDeletionResult>("delete_context", {
      contextId,
      projectIds,
      itemIds,
      repositoryIds,
      worksetIds,
      machineIds,
      confirmed: true,
      deleteWorksetDirectories,
    }),
  registerRepository: (projectId: number, name: string, remoteUrl: string) =>
    command<Repository>("register_repository", { projectId, name, remoteUrl }),
  registerRepositoryAtLocation: (input: {
    projectId: number;
    name: string;
    remoteUrl: string | null;
    baseBranch: string;
    machineId: number;
    checkoutPath: string;
    worktreeRoot: string | null;
    cloneIntoDestination: boolean;
  }) => command<Repository>("register_repository_at_location", input),
  prepareRepositoryDeletion: (repositoryId: number) =>
    command<RepositoryDeletionPreview>("prepare_repository_deletion", {
      repositoryId,
    }),
  deleteRepository: (
    repositoryId: number,
    worksetIds: number[],
    deleteWorksetDirectories: boolean,
  ) =>
    command<RepositoryDeletionResult>("delete_repository", {
      repositoryId,
      worksetIds,
      confirmed: true,
      deleteWorksetDirectories,
    }),
  prepareMachineDeletion: (machineId: number) =>
    command<MachineDeletionPreview>("prepare_machine_deletion", { machineId }),
  deleteMachine: (machineId: number, runIds: number[]) =>
    command<MachineDeletionResult>("delete_machine", {
      machineId,
      runIds,
      confirmed: true,
    }),
  deleteRun: (runId: number) =>
    command<RunDeletionResult>("delete_run", { runId, confirmed: true }),
  registerMachine: (
    contextId: number,
    name: string,
    socketName: string,
    transport: MachineTransport,
  ) =>
    command<Machine>("register_machine", {
      contextId,
      name,
      socketName,
      transport,
    }),
  checkMachine: (machineId: number) =>
    command<Machine>("check_machine", { machineId }),
  setAttentionDefault: (
    contextId: number,
    objectKind: ContextAttentionDefault["object_kind"],
    policy: ContextAttentionDefault["policy"],
  ) =>
    command<ContextAttentionDefault>("set_context_attention_default", {
      contextId,
      objectKind,
      policy,
    }),
  createItem: (title: string, contextId: number, projectId: number) =>
    command<Item>("create_item", { title, contextId, projectId }),
};
