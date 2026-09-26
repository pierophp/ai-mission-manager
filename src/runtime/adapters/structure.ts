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
  GrillAgentCatalog,
  GrillConfiguration,
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
  listGrillModelCatalog: () =>
    command<GrillAgentCatalog[]>("list_grill_model_catalog"),
  createContext: (name: string) => command<Context>("create_context", { name }),
  updateContext: (contextId: number, name: string) =>
    command<Context>("update_context", { contextId, name }),
  setContextExecutionMachine: (contextId: number, machineId: number | null) =>
    command<Context>("set_context_execution_machine", { contextId, machineId }),
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
  updateProject: (
    projectId: number,
    name: string,
    defaultItemStatus: ItemStatus,
    executionMode: ExecutionMode = "worktree",
  ) =>
    command<ProjectRecord>("update_project", {
      projectId,
      name,
      defaultItemStatus,
      executionMode,
    }),
  prepareProjectDeletion: (projectId: number) =>
    command<ParentDeletionPreview>("prepare_project_deletion", { projectId }),
  prepareContextDeletion: (contextId: number) =>
    command<ParentDeletionPreview>("prepare_context_deletion", { contextId }),
  prepareReset: () => command<ResetLocalDataPreview>("prepare_reset_local_data"),
  reset: (confirmation: string) =>
    command<ResetLocalDataResult>("reset_all_local_data", { confirmation }),
  deleteProject: (
    projectId: number,
    itemIds: number[],
    repositoryIds: number[],
    workspaceIds: number[],
  ) =>
    command<ParentDeletionResult>("delete_project", {
      projectId,
      itemIds,
      repositoryIds,
      workspaceIds,
      confirmed: true,
    }),
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
    command<ParentDeletionResult>("delete_context", {
      contextId,
      projectIds,
      itemIds,
      repositoryIds,
      workspaceIds,
      machineIds,
      confirmed: true,
    }),
  registerRepository: (projectId: number, name: string, remoteUrl: string) =>
    command<Repository>("register_repository", { projectId, name, remoteUrl }),
  updateRepository: (
    repositoryId: number,
    name: string,
    remoteUrl: string,
    baseBranch: string,
  ) =>
    command<Repository>("update_repository", {
      repositoryId,
      name,
      remoteUrl,
      baseBranch,
    }),
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
  updateRepositoryLocation: (input: {
    repositoryId: number;
    previousMachineId: number | null;
    machineId: number;
    checkoutPath: string;
    worktreeRoot: string;
  }) => command<RepositoryLocation>("update_repository_location", input),
  prepareRepositoryDeletion: (repositoryId: number) =>
    command<RepositoryDeletionPreview>("prepare_repository_deletion", {
      repositoryId,
    }),
  deleteRepository: (
    repositoryId: number,
    workspaceIds: number[],
  ) =>
    command<RepositoryDeletionResult>("delete_repository", {
      repositoryId,
      workspaceIds,
      confirmed: true,
    }),
  prepareMachineDeletion: (machineId: number) =>
    command<MachineDeletionPreview>("prepare_machine_deletion", { machineId }),
  deleteMachine: (
    machineId: number,
    runIds: number[],
    worktreeIds: number[],
    repositoryLocationRepositoryIds: number[],
  ) =>
    command<MachineDeletionResult>("delete_machine", {
      machineId,
      runIds,
      worktreeIds,
      repositoryLocationRepositoryIds,
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
  updateMachine: (
    machineId: number,
    name: string,
    socketName: string,
    transport: MachineTransport,
  ) =>
    command<Machine>("update_machine", {
      machineId,
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
  setContextGrillDefaults: (contextId: number, defaults: GrillConfiguration) =>
    command<Context>("set_context_grill_defaults", { contextId, defaults }),
  setContextImplementDefaults: (contextId: number, defaults: GrillConfiguration) =>
    command<Context>("set_context_implement_defaults", { contextId, defaults }),
  createItem: (title: string, contextId: number, projectId: number) =>
    command<Item>("create_item", { title, contextId, projectId }),
};
