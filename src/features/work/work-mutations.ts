import { useMutation, useQueryClient } from "@tanstack/react-query";

import { workAdapter } from "../../runtime/adapters";
import { invalidateWorkQueries } from "../../runtime/query-invalidation";

export type WorkAction<TData> = () => Promise<TData>;

export const workActions = {
  setItemStatus: (itemId: number, status: Parameters<typeof workAdapter.setItemStatus>[1]) =>
    () => workAdapter.setItemStatus(itemId, status),
  setItemTitle: (itemId: number, title: string) =>
    () => workAdapter.setItemTitle(itemId, title),
  setItemNotes: (itemId: number, notes: string) =>
    () => workAdapter.setItemNotes(itemId, notes),
  addReminder: (itemId: number, remindAt: string) =>
    () => workAdapter.addReminder(itemId, remindAt),
  removeReminder: (itemId: number, reminderId: number) =>
    () => workAdapter.removeReminder(itemId, reminderId),
  setRelation: (fromItemId: number, toItemId: number, kind: Parameters<typeof workAdapter.setRelation>[2]) =>
    () => workAdapter.setRelation(fromItemId, toItemId, kind),
  linkExternalObject: (itemId: number, url: string) =>
    () => workAdapter.linkExternalObject(itemId, url),
  createWorkspace: (
    itemId: number,
    repositories: Parameters<typeof workAdapter.createWorkspace>[1],
    ) => () => workAdapter.createWorkspace(itemId, repositories),
  prepareWorktree: (
    workspaceId: number,
    repositoryId: number,
    machineId: number,
    reuseExistingBranch: boolean,
    confirmDirtyAttachment: boolean,
  ) =>
    () =>
      workAdapter.prepareWorktree(
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
      workAdapter.attachWorktree(
        workspaceId,
        repositoryId,
        machineId,
        path,
        confirmDirtyAttachment,
      ),
  prepareWorkspaceRemoval: (workspaceId: number) =>
    () => workAdapter.prepareWorkspaceRemoval(workspaceId),
  removeWorkspace: (
    workspaceId: number,
    confirmedWorktreeIds: number[],
    destructiveWorktreeIds: number[],
  ) =>
    () =>
      workAdapter.removeWorkspace(
        workspaceId,
        confirmedWorktreeIds,
        destructiveWorktreeIds,
      ),
  prepareDirectRun: (itemId: number, workspaceId: number, machineId: number | null) =>
    () => workAdapter.prepareDirectRun(itemId, workspaceId, machineId),
  prepareGrillRun: (itemId: number, workspaceId: number, machineId: number | null) =>
    () => workAdapter.prepareGrillRun(itemId, workspaceId, machineId),
  stopRun: (runId: number) => () => workAdapter.stopRun(runId),
  finishRun: (runId: number) => () => workAdapter.finishRun(runId),
  submitGrillAnswers: (runId: number, answers: Parameters<typeof workAdapter.submitGrillAnswers>[1]) =>
    () => workAdapter.submitGrillAnswers(runId, answers),
  deleteRun: (runId: number) => () => workAdapter.deleteRun(runId),
  prepareItemDeletion: (itemId: number) =>
    () => workAdapter.prepareItemDeletion(itemId),
  deleteItem: (itemId: number) => () => workAdapter.deleteItem(itemId),
  unlinkExternalLink: (linkId: number) =>
    () => workAdapter.unlinkExternalLink(linkId),
  prepareExternalObjectDeletion: (externalObjectId: number) =>
    () => workAdapter.prepareExternalObjectDeletion(externalObjectId),
  deleteExternalObject: (externalObjectId: number) =>
    () => workAdapter.deleteExternalObject(externalObjectId),
  composeRunPrompt: (
    itemId: number,
    executionProfile: Parameters<typeof workAdapter.composeRunPrompt>[1],
    selection: Parameters<typeof workAdapter.composeRunPrompt>[2],
    customPrompt: string | null,
  ) => () => workAdapter.composeRunPrompt(itemId, executionProfile, selection, customPrompt),
  composeGrillPrompt: (
    itemId: number,
    configuration: Parameters<typeof workAdapter.composeGrillPrompt>[1],
    initialPrompt: string,
  ) => () => workAdapter.composeGrillPrompt(itemId, configuration, initialPrompt),
  startDirectRun: (input: Parameters<typeof workAdapter.startDirectRun>[0]) =>
    () => workAdapter.startDirectRun(input),
  startGrillRun: (input: Parameters<typeof workAdapter.startGrillRun>[0]) =>
    () => workAdapter.startGrillRun(input),
  startWorktreeRun: (input: Parameters<typeof workAdapter.startWorktreeRun>[0]) =>
    () => workAdapter.startWorktreeRun(input),
  refreshExternalObject: (externalObjectId: number) =>
    () => workAdapter.refreshExternalObject(externalObjectId),
  createGithubIssue: (itemId: number, repository: string, title: string, body: string) =>
    () => workAdapter.createGithubIssue(itemId, repository, title, body),
  setLinkAttentionPolicy: (
    linkId: number,
    policy: Parameters<typeof workAdapter.setLinkAttentionPolicy>[1],
  ) => () => workAdapter.setLinkAttentionPolicy(linkId, policy),
  markLinkReviewed: (linkId: number) => () => workAdapter.markLinkReviewed(linkId),
  setLinkWatchUntil: (linkId: number, watchUntil: string | null) =>
    () => workAdapter.setLinkWatchUntil(linkId, watchUntil),
  setLinkReviewAt: (linkId: number, reviewAt: string | null) =>
    () => workAdapter.setLinkReviewAt(linkId, reviewAt),
  clearLinkReviewAt: (linkId: number) => () => workAdapter.clearLinkReviewAt(linkId),
  addExternalComment: (linkId: number, body: string) =>
    () => workAdapter.addExternalComment(linkId, body),
  openExternalTerminal: (runId: number) => () => workAdapter.openExternalTerminal(runId),
  attachRun: (suggestion: Parameters<typeof workAdapter.attachRun>[0]) =>
    () => workAdapter.attachRun(suggestion),
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
