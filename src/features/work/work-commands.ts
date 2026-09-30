import { useMutation, useQueryClient } from "@tanstack/react-query";

import type {
  ExternalLinkView,
  GrillAnswer,
  GrillConfiguration,
  GrillLanguage,
  Item,
  ItemRelationKind,
  Run,
  RunLaunchRequest,
  RunLaunchTargetKind,
  RunPromptSelection,
  RunSuggestion,
} from "../../runtime/types";
import type { GrillContinuationAction, Workflow } from "../../runtime/execution-types";
import { command } from "../../runtime/command";
import {
  normalizeHomeView,
  normalizeItemViews,
  normalizeRunLaunchRequest,
} from "../../runtime/normalize-generated";

import { invalidateWorkQueries } from "../../runtime/query-invalidation";

export const workCommands = {
  getActivityTab: () => command("getActivityTab"),
  getRunLaunchOptions: (itemId: number, target: RunLaunchTargetKind) =>
    command("getRunLaunchOptions", itemId, target),
  getHome: (contextId: number | undefined, now: string) =>
    command("getHome", contextId ?? null, now).then(normalizeHomeView),
  searchItems: (query: string, contextId: number | undefined) =>
    command("searchItemsCommand", query, contextId ?? null).then(normalizeItemViews),
  reconcileRuns: () => command("reconcileRuns"),
  listRunSuggestions: () => command("listRunSuggestions"),
  attachRun: (suggestion: RunSuggestion) =>
    command("attachRun", suggestion),
  stopUntrackedAgent: (suggestion: RunSuggestion) =>
    command("stopUntrackedAgent", suggestion),
  deleteUntrackedAgent: (suggestion: RunSuggestion) =>
    command("deleteUntrackedAgent", suggestion),
  pollExternalObjects: () => command("pollExternalObjects"),
  setItemStatus: (itemId: number, status: Item["status"]) =>
    command("setItemStatus", itemId, status),
  setItemTitle: (itemId: number, title: string) =>
    command("setItemTitle", itemId, title),
  setItemNotes: (itemId: number, notes: string) =>
    command("setItemNotes", itemId, notes),
  addReminder: (itemId: number, remindAt: string) =>
    command("addItemReminder", itemId, remindAt),
  removeReminder: (itemId: number, reminderId: number) =>
    command("removeItemReminder", itemId, reminderId),
  setRelation: (fromItemId: number, toItemId: number, kind: ItemRelationKind) =>
    command("setItemRelation", fromItemId, toItemId, kind),
  linkExternalObject: (itemId: number, url: string) =>
    command("linkExternalObject", itemId, url),
  prepareDirectRun: (
    itemId: number,
    workspaceId: number,
    machineId: number | null,
  ) =>
    command("prepareDirectRun", itemId, workspaceId, machineId),
  prepareGrillRun: (
    itemId: number,
    workspaceId: number,
    machineId: number | null,
  ) =>
    command("prepareGrillRun", itemId, workspaceId, machineId),
  createWorktree: (
    workspaceId: number,
    repositoryId: number,
    machineId: number,
    path: string,
    branch: string,
    baseBranch: string,
  ) =>
    command("createWorktree", workspaceId, repositoryId, machineId, path, branch, baseBranch),
  prepareWorktree: (
    workspaceId: number,
    repositoryId: number,
    machineId: number,
    reuseExistingBranch: boolean,
    confirmDirtyAttachment: boolean,
  ) =>
    command("prepareWorktree", workspaceId, repositoryId, machineId, reuseExistingBranch, confirmDirtyAttachment),
  attachWorktree: (
    workspaceId: number,
    repositoryId: number,
    machineId: number,
    path: string,
    confirmDirtyAttachment: boolean,
  ) =>
    command("attachWorktree", workspaceId, repositoryId, machineId, path, confirmDirtyAttachment),
  prepareWorktreeRemoval: (worktreeId: number) =>
    command("prepareWorktreeRemoval", worktreeId),
  removeWorktree: (
    worktreeId: number,
    confirmed: boolean,
    destructiveConfirmed: boolean,
  ) =>
    command("removeWorktree", worktreeId, confirmed, destructiveConfirmed),
  stopRun: (runId: number) => command("stopRun", runId),
  finishRun: (runId: number) => command("finishRun", runId),
  submitGrillAnswers: (runId: number, answers: GrillAnswer[]) =>
    command("submitGrillAnswers", runId, answers),
  continueGrill: (runId: number, action: GrillContinuationAction) =>
    command("continueGrill", runId, action),
  goPlan: (runId: number) => command("goPlan", runId),
  deleteRun: (runId: number) =>
    command("deleteRun", runId, true),
  prepareItemDeletion: (itemId: number) =>
    command("prepareItemDeletion", itemId),
  deleteItem: (itemId: number) =>
    command("deleteItem", itemId, true),
  unlinkExternalLink: (linkId: number) =>
    command("unlinkExternalLink", linkId, true),
  prepareExternalObjectDeletion: (externalObjectId: number) =>
    command("prepareExternalObjectDeletion", externalObjectId),
  deleteExternalObject: (externalObjectId: number) =>
    command("deleteExternalObject", externalObjectId, true),
  composeRunPrompt: (
    itemId: number,
    executionProfile: Run["execution_profile"],
    selection: RunPromptSelection,
    language: GrillLanguage | null,
    initialPrompt: string | null,
    workflow: Workflow,
  ) =>
    command("composeRunPrompt", itemId, executionProfile, selection, language, initialPrompt, workflow),
  composeGrillPrompt: (
    itemId: number,
    configuration: GrillConfiguration,
    language: GrillLanguage,
    initialPrompt: string,
  ) =>
    command("composeGrillPrompt", itemId, configuration, language, initialPrompt),
  startRun: (request: RunLaunchRequest) =>
    command("startRun", normalizeRunLaunchRequest(request)),
  checkImplementationQueue: (queueId: number) =>
    command("checkImplementationQueue", queueId),
  skipImplementationQueueEntry: (queueId: number) =>
    command("skipImplementationQueueEntry", queueId),
  cancelImplementationQueue: (queueId: number) =>
    command("cancelImplementationQueue", queueId),
  refreshExternalObject: (externalObjectId: number) =>
    command("refreshExternalObject", externalObjectId),
  fetchIssueDocument: (externalObjectId: number) =>
    command("fetchIssueDocument", externalObjectId),
  fetchExternalDocument: (externalObjectId: number) =>
    command("fetchExternalDocument", externalObjectId),
  fetchExternalComments: (externalObjectId: number) =>
    command("fetchExternalComments", externalObjectId),
  createGithubIssue: (
    itemId: number,
    repositoryId: number,
    title: string,
    body: string,
  ) =>
    command("createGithubIssue", itemId, repositoryId, title, body),
  setLinkAttentionPolicy: (
    linkId: number,
    policy: ExternalLinkView["link"]["attention_policy"],
  ) =>
    command("setLinkAttentionPolicy", linkId, policy),
  setLinkPurpose: (
    linkId: number,
    purpose: ExternalLinkView["link"]["purpose"],
    specExternalObjectId: number | null,
  ) =>
    command("setLinkPurpose", linkId, purpose, specExternalObjectId),
  markLinkReviewed: (linkId: number) =>
    command("markLinkReviewed", linkId),
  setLinkWatchUntil: (linkId: number, watchUntil: string | null) =>
    command("setLinkWatchUntil", linkId, watchUntil),
  setLinkReviewAt: (linkId: number, reviewAt: string | null) =>
    command("setLinkReviewAt", linkId, reviewAt),
  clearLinkReviewAt: (linkId: number) =>
    command("clearLinkReviewAt", linkId),
  addExternalComment: (linkId: number, body: string) =>
    command("addExternalComment", linkId, body),
  openExternalTerminal: (runId: number) =>
    command("openExternalTerminal", runId),
};


export type WorkAction<TData> = () => Promise<TData>;

export const workActions = {
  getRunLaunchOptions: (itemId: number, target: Parameters<typeof workCommands.getRunLaunchOptions>[1]) =>
    () => workCommands.getRunLaunchOptions(itemId, target),
  setItemStatus: (itemId: number, status: Parameters<typeof workCommands.setItemStatus>[1]) =>
    () => workCommands.setItemStatus(itemId, status),
  setItemTitle: (itemId: number, title: string) =>
    () => workCommands.setItemTitle(itemId, title),
  setItemNotes: (itemId: number, notes: string) =>
    () => workCommands.setItemNotes(itemId, notes),
  addReminder: (itemId: number, remindAt: string) =>
    () => workCommands.addReminder(itemId, remindAt),
  removeReminder: (itemId: number, reminderId: number) =>
    () => workCommands.removeReminder(itemId, reminderId),
  setRelation: (fromItemId: number, toItemId: number, kind: Parameters<typeof workCommands.setRelation>[2]) =>
    () => workCommands.setRelation(fromItemId, toItemId, kind),
  linkExternalObject: (itemId: number, url: string) =>
    () => workCommands.linkExternalObject(itemId, url),
  prepareWorktree: (
    workspaceId: number,
    repositoryId: number,
    machineId: number,
    reuseExistingBranch: boolean,
    confirmDirtyAttachment: boolean,
  ) =>
    () =>
      workCommands.prepareWorktree(
        workspaceId,
        repositoryId,
        machineId,
        reuseExistingBranch,
        confirmDirtyAttachment,
      ),
  attachWorktree: (
    workspaceId: number,
    repositoryId: number,
    machineId: number,
    path: string,
    confirmDirtyAttachment: boolean,
  ) =>
    () =>
      workCommands.attachWorktree(
        workspaceId,
        repositoryId,
        machineId,
        path,
        confirmDirtyAttachment,
      ),
  prepareWorktreeRemoval: (worktreeId: number) =>
    () => workCommands.prepareWorktreeRemoval(worktreeId),
  removeWorktree: (worktreeId: number, destructiveConfirmed: boolean) =>
    () => workCommands.removeWorktree(worktreeId, true, destructiveConfirmed),
  prepareDirectRun: (itemId: number, workspaceId: number, machineId: number | null) =>
    () => workCommands.prepareDirectRun(itemId, workspaceId, machineId),
  prepareGrillRun: (itemId: number, workspaceId: number, machineId: number | null) =>
    () => workCommands.prepareGrillRun(itemId, workspaceId, machineId),
  stopRun: (runId: number) => () => workCommands.stopRun(runId),
  finishRun: (runId: number) => () => workCommands.finishRun(runId),
  submitGrillAnswers: (runId: number, answers: Parameters<typeof workCommands.submitGrillAnswers>[1]) =>
    () => workCommands.submitGrillAnswers(runId, answers),
  continueGrill: (
    runId: number,
    action: Parameters<typeof workCommands.continueGrill>[1],
  ) => () => workCommands.continueGrill(runId, action),
  goPlan: (runId: number) => () => workCommands.goPlan(runId),
  deleteRun: (runId: number) => () => workCommands.deleteRun(runId),
  prepareItemDeletion: (itemId: number) =>
    () => workCommands.prepareItemDeletion(itemId),
  deleteItem: (itemId: number) => () => workCommands.deleteItem(itemId),
  unlinkExternalLink: (linkId: number) =>
    () => workCommands.unlinkExternalLink(linkId),
  prepareExternalObjectDeletion: (externalObjectId: number) =>
    () => workCommands.prepareExternalObjectDeletion(externalObjectId),
  deleteExternalObject: (externalObjectId: number) =>
    () => workCommands.deleteExternalObject(externalObjectId),
  composeRunPrompt: (
    itemId: number,
    executionProfile: Parameters<typeof workCommands.composeRunPrompt>[1],
    selection: Parameters<typeof workCommands.composeRunPrompt>[2],
    language: Parameters<typeof workCommands.composeRunPrompt>[3],
    initialPrompt: string | null,
    workflow: Parameters<typeof workCommands.composeRunPrompt>[5],
  ) =>
    () =>
      workCommands.composeRunPrompt(
        itemId,
        executionProfile,
        selection,
        language,
        initialPrompt,
        workflow,
      ),
  composeGrillPrompt: (
    itemId: number,
    configuration: Parameters<typeof workCommands.composeGrillPrompt>[1],
    language: Parameters<typeof workCommands.composeGrillPrompt>[2],
    initialPrompt: string,
  ) =>
    () =>
      workCommands.composeGrillPrompt(
        itemId,
        configuration,
        language,
        initialPrompt,
      ),
  startRun: (request: Parameters<typeof workCommands.startRun>[0]) =>
    () => workCommands.startRun(request),
  checkImplementationQueue: (queueId: number) => () => workCommands.checkImplementationQueue(queueId),
  skipImplementationQueueEntry: (queueId: number) => () => workCommands.skipImplementationQueueEntry(queueId),
  cancelImplementationQueue: (queueId: number) => () => workCommands.cancelImplementationQueue(queueId),
  refreshExternalObject: (externalObjectId: number) =>
    () => workCommands.refreshExternalObject(externalObjectId),
  createGithubIssue: (itemId: number, repositoryId: number, title: string, body: string) =>
    () => workCommands.createGithubIssue(itemId, repositoryId, title, body),
  setLinkAttentionPolicy: (
    linkId: number,
    policy: Parameters<typeof workCommands.setLinkAttentionPolicy>[1],
  ) => () => workCommands.setLinkAttentionPolicy(linkId, policy),
  setLinkPurpose: (
    linkId: number,
    purpose: Parameters<typeof workCommands.setLinkPurpose>[1],
    specExternalObjectId: Parameters<typeof workCommands.setLinkPurpose>[2],
  ) => () => workCommands.setLinkPurpose(linkId, purpose, specExternalObjectId),
  markLinkReviewed: (linkId: number) => () => workCommands.markLinkReviewed(linkId),
  setLinkWatchUntil: (linkId: number, watchUntil: string | null) =>
    () => workCommands.setLinkWatchUntil(linkId, watchUntil),
  setLinkReviewAt: (linkId: number, reviewAt: string | null) =>
    () => workCommands.setLinkReviewAt(linkId, reviewAt),
  clearLinkReviewAt: (linkId: number) => () => workCommands.clearLinkReviewAt(linkId),
  addExternalComment: (linkId: number, body: string) =>
    () => workCommands.addExternalComment(linkId, body),
  openExternalTerminal: (runId: number) => () => workCommands.openExternalTerminal(runId),
  attachRun: (suggestion: Parameters<typeof workCommands.attachRun>[0]) =>
    () => workCommands.attachRun(suggestion),
  stopUntrackedAgent: (suggestion: Parameters<typeof workCommands.stopUntrackedAgent>[0]) =>
    () => workCommands.stopUntrackedAgent(suggestion),
  deleteUntrackedAgent: (suggestion: Parameters<typeof workCommands.deleteUntrackedAgent>[0]) =>
    () => workCommands.deleteUntrackedAgent(suggestion),
};

export function useWorkCommand() {
  const queryClient = useQueryClient();
  const mutation = useMutation({
    mutationFn: ({ action }: { action: WorkAction<unknown>; invalidate: boolean }) =>
      action(),
    onSuccess: async (_, variables) => {
      if (variables.invalidate) await invalidateWorkQueries(queryClient);
    },
  });

  return {
    isPending: mutation.isPending,
    execute<TData>(action: WorkAction<TData>, invalidate = true) {
      return mutation.mutateAsync({ action, invalidate }) as Promise<TData>;
    },
  };
}
