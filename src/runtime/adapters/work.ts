import type {
  ExternalLinkAction,
  ExternalLinkDeletionResult,
  ExternalLinkView,
  ExternalObjectDeletionPreview,
  ExternalObjectDeletionResult,
  ExternalSnapshot,
  DirectRunPreview,
  HomeView,
  Item,
  ItemDeletionPreview,
  ItemDeletionResult,
  ItemRelation,
  ItemRelationKind,
  ItemView,
  PollResult,
  Run,
  RunCheckout,
  RunPromptSelection,
  RunSuggestion,
  Workset,
  WorksetRemovalReport,
  WorksetRemovalResult,
  WorksetRepositoryInput,
  Workspace,
  WorkspaceRepositoryInput,
  Worktree,
} from "../types";
import { command } from "./tauri";

export const workAdapter = {
  getHome: (contextId: number | undefined, now: string) =>
    command<HomeView>("get_home", { contextId: contextId ?? null, now }),
  searchItems: (query: string, contextId: number | undefined) =>
    command<ItemView[]>("search_items_command", {
      query,
      contextId: contextId ?? null,
    }),
  reconcileRuns: () => command<void>("reconcile_runs"),
  listRunSuggestions: () => command<RunSuggestion[]>("list_run_suggestions"),
  attachRun: (suggestion: RunSuggestion) =>
    command<Run>("attach_run", { suggestion }),
  pollExternalObjects: () => command<PollResult>("poll_external_objects"),
  setItemStatus: (itemId: number, status: Item["status"]) =>
    command<Item>("set_item_status", { itemId, status }),
  setItemTitle: (itemId: number, title: string) =>
    command<Item>("set_item_title", { itemId, title }),
  setItemNotes: (itemId: number, notes: string) =>
    command<Item>("set_item_notes", { itemId, notes }),
  addReminder: (itemId: number, remindAt: string) =>
    command<Item>("add_item_reminder", { itemId, remindAt }),
  removeReminder: (itemId: number, reminderId: number) =>
    command<Item>("remove_item_reminder", { itemId, reminderId }),
  setRelation: (fromItemId: number, toItemId: number, kind: ItemRelationKind) =>
    command<ItemRelation>("set_item_relation", { fromItemId, toItemId, kind }),
  linkExternalObject: (itemId: number, url: string) =>
    command<ExternalLinkAction>("link_external_object", { itemId, url }),
  createWorkset: (
    itemId: number,
    rootDirectory: string,
    branch: string,
    repositories: WorksetRepositoryInput[],
  ) =>
    command<Workset>("create_workset", {
      itemId,
      rootDirectory,
      branch,
      repositories,
    }),
  createWorkspace: (itemId: number, repositories: WorkspaceRepositoryInput[]) =>
    command<Workspace>("create_workspace", { itemId, repositories }),
  prepareDirectRun: (
    itemId: number,
    workspaceId: number,
    machineId: number | null,
  ) =>
    command<DirectRunPreview>("prepare_direct_run", {
      itemId,
      workspaceId,
      machineId,
    }),
  createWorktree: (
    workspaceId: number,
    repositoryId: number,
    machineId: number,
    path: string,
    branch: string,
    baseBranch: string,
  ) =>
    command<Worktree>("create_worktree", {
      workspaceId,
      repositoryId,
      machineId,
      path,
      branch,
      baseBranch,
    }),
  attachWorkset: (itemId: number, rootDirectory: string) =>
    command<Workset>("attach_workset", { itemId, rootDirectory }),
  addRepositoryToWorkset: (
    worksetId: number,
    repositoryId: number,
    branchOverride: string | null,
    baseBranchOverride: string | null,
  ) =>
    command<Workset>("add_repository_to_workset", {
      worksetId,
      repositoryId,
      branchOverride,
      baseBranchOverride,
    }),
  setWorksetArchived: (worksetId: number, archived: boolean) =>
    command<Workset>("set_workset_archived", { worksetId, archived }),
  stopRun: (runId: number) => command<Run>("stop_run", { runId }),
  deleteRun: (runId: number) =>
    command<{ runId: number }>("delete_run", { runId, confirmed: true }),
  prepareWorksetRemoval: (worksetId: number) =>
    command<WorksetRemovalReport>("prepare_workset_removal", { worksetId }),
  removeWorkset: (worksetId: number) =>
    command<WorksetRemovalResult>("remove_workset", { worksetId, confirmed: true }),
  prepareItemDeletion: (itemId: number) =>
    command<ItemDeletionPreview>("prepare_item_deletion", { itemId }),
  deleteItem: (itemId: number, deleteWorksetDirectories: boolean) =>
    command<ItemDeletionResult>("delete_item", {
      itemId,
      confirmed: true,
      deleteWorksetDirectories,
    }),
  unlinkExternalLink: (linkId: number) =>
    command<ExternalLinkDeletionResult>("unlink_external_link", {
      linkId,
      confirmed: true,
    }),
  prepareExternalObjectDeletion: (externalObjectId: number) =>
    command<ExternalObjectDeletionPreview>("prepare_external_object_deletion", {
      externalObjectId,
    }),
  deleteExternalObject: (externalObjectId: number) =>
    command<ExternalObjectDeletionResult>("delete_external_object", {
      externalObjectId,
      confirmed: true,
    }),
  composeRunPrompt: (
    itemId: number,
    executionProfile: Run["execution_profile"],
    selection: RunPromptSelection,
    customPrompt: string | null,
  ) =>
    command<string>("compose_run_prompt", {
      itemId,
      executionProfile,
      promptSelection: selection,
      customPrompt,
    }),
  startRun: ({
    itemId,
    worksetId,
    machineId,
    agent,
    executionProfile,
    prompt,
    promptSelection,
  }: {
    itemId: number;
    worksetId: number;
    machineId: number | null;
    agent: Run["agent"];
    executionProfile: Run["execution_profile"];
    prompt: string;
    promptSelection: RunPromptSelection;
  }) =>
    command<Run>("start_run", {
      itemId,
      worksetId,
      machineId,
      agent,
      executionProfile,
      prompt,
      promptSelection,
    }),
  startDirectRun: ({
    itemId,
    workspaceId,
    machineId,
    agent,
    executionProfile,
    prompt,
    promptSelection,
    expectedCheckouts,
    allowDirty,
    allowSharedCheckouts,
  }: {
    itemId: number;
    workspaceId: number;
    machineId: number | null;
    agent: Run["agent"];
    executionProfile: Run["execution_profile"];
    prompt: string;
    promptSelection: RunPromptSelection;
    expectedCheckouts: RunCheckout[];
    allowDirty: boolean;
    allowSharedCheckouts: boolean;
  }) =>
    command<Run>("start_direct_run", {
      itemId,
      workspaceId,
      machineId,
      agent,
      executionProfile,
      prompt,
      promptSelection,
      expectedCheckouts,
      allowDirty,
      allowSharedCheckouts,
    }),
  refreshExternalObject: (externalObjectId: number) =>
    command<ExternalSnapshot>("refresh_external_object", { externalObjectId }),
  createGithubIssue: (itemId: number, repository: string, title: string, body: string) =>
    command<ExternalLinkAction>("create_github_issue", {
      itemId,
      repository,
      title,
      body,
    }),
  setLinkAttentionPolicy: (
    linkId: number,
    policy: ExternalLinkView["link"]["attention_policy"],
  ) => command<ExternalLinkView>("set_link_attention_policy", { linkId, policy }),
  markLinkReviewed: (linkId: number) =>
    command<ExternalLinkView>("mark_link_reviewed", { linkId }),
  setLinkWatchUntil: (linkId: number, watchUntil: string | null) =>
    command<ExternalLinkView>("set_link_watch_until", { linkId, watchUntil }),
  setLinkReviewAt: (linkId: number, reviewAt: string | null) =>
    command<ExternalLinkView>("set_link_review_at", { linkId, reviewAt }),
  clearLinkReviewAt: (linkId: number) =>
    command<ExternalLinkView>("clear_link_review_at", { linkId }),
  addExternalComment: (linkId: number, body: string) =>
    command<ExternalLinkView>("add_external_comment", { linkId, body }),
  openExternalTerminal: (runId: number) =>
    command<void>("open_external_terminal", { runId }),
};
