import type {
  ExternalLinkAction,
  ExternalLinkDeletionResult,
  ExternalLinkView,
  ExternalObjectDeletionPreview,
  ExternalObjectDeletionResult,
  ExternalSnapshot,
  DirectRunPreview,
  GrillAnswer,
  GrillConfiguration,
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
  WorktreeRemovalReport,
  Worktree,
} from "../types";
import type { GrillContinuationAction } from "../execution-types";
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
  prepareGrillRun: (
    itemId: number,
    workspaceId: number,
    machineId: number | null,
  ) =>
    command<DirectRunPreview>("prepare_grill_run", {
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
  prepareWorktree: (
    workspaceId: number,
    repositoryId: number,
    machineId: number,
    reuseExistingBranch: boolean,
    confirmDirtyAttachment: boolean,
  ) =>
    command<Worktree>("prepare_worktree", {
      workspaceId,
      repositoryId,
      machineId,
      reuseExistingBranch,
      confirmDirtyAttachment,
    }),
  attachWorktree: (
    workspaceId: number,
    repositoryId: number,
    machineId: number,
    path: string,
    confirmDirtyAttachment: boolean,
  ) =>
    command<Worktree>("attach_worktree", {
      workspaceId,
      repositoryId,
      machineId,
      path,
      confirmDirtyAttachment,
    }),
  prepareWorktreeRemoval: (worktreeId: number) =>
    command<WorktreeRemovalReport>("prepare_worktree_removal", { worktreeId }),
  removeWorktree: (
    worktreeId: number,
    confirmed: boolean,
    destructiveConfirmed: boolean,
  ) =>
    command<{ worktreeId: number; branchPreserved: boolean }>("remove_worktree", {
      worktreeId,
      confirmed,
      destructiveConfirmed,
    }),
  stopRun: (runId: number) => command<Run>("stop_run", { runId }),
  finishRun: (runId: number) => command<Run>("finish_run", { runId }),
  submitGrillAnswers: (runId: number, answers: GrillAnswer[]) =>
    command<Run>("submit_grill_answers", { runId, answers }),
  continueGrill: (runId: number, action: GrillContinuationAction) =>
    command<Run>("continue_grill", { runId, action }),
  deleteRun: (runId: number) =>
    command<{ runId: number }>("delete_run", { runId, confirmed: true }),
  prepareItemDeletion: (itemId: number) =>
    command<ItemDeletionPreview>("prepare_item_deletion", { itemId }),
  deleteItem: (itemId: number) =>
    command<ItemDeletionResult>("delete_item", { itemId, confirmed: true }),
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
  composeGrillPrompt: (
    itemId: number,
    configuration: GrillConfiguration,
    initialPrompt: string,
  ) =>
    command<string>("compose_grill_prompt", {
      itemId,
      configuration,
      initialPrompt,
    }),
  startDirectRun: ({
    itemId,
    workspaceId,
    primaryRepositoryId,
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
    primaryRepositoryId: number;
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
      primaryRepositoryId,
      machineId,
      agent,
      executionProfile,
      prompt,
      promptSelection,
      expectedCheckouts,
      allowDirty,
      allowSharedCheckouts,
    }),
  startGrillRun: ({
    itemId,
    workspaceId,
    primaryRepositoryId,
    machineId,
    configuration,
    initialPrompt,
    expectedCheckouts,
    allowDirty,
    allowSharedCheckouts,
  }: {
    itemId: number;
    workspaceId: number;
    primaryRepositoryId: number;
    machineId: number | null;
    configuration: GrillConfiguration;
    initialPrompt: string;
    expectedCheckouts: RunCheckout[];
    allowDirty: boolean;
    allowSharedCheckouts: boolean;
  }) =>
    command<Run>("start_grill_run", {
      itemId,
      workspaceId,
      primaryRepositoryId,
      machineId,
      configuration,
      initialPrompt,
      expectedCheckouts,
      allowDirty,
      allowSharedCheckouts,
    }),
  startWorktreeRun: ({
    itemId,
    workspaceId,
    worktreeId,
    agent,
    executionProfile,
    prompt,
    promptSelection,
  }: {
    itemId: number;
    workspaceId: number;
    worktreeId: number;
    agent: Run["agent"];
    executionProfile: Run["execution_profile"];
    prompt: string;
    promptSelection: RunPromptSelection;
  }) =>
    command<Run>("start_worktree_run", {
      itemId,
      workspaceId,
      worktreeId,
      agent,
      executionProfile,
      prompt,
      promptSelection,
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
