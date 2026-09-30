import type {
  ExecutionProfile,
  Workflow,
} from "../../../runtime/execution-types";
import type {
  GrillConfiguration,
  GrillLanguage,
  RunLaunchOptions,
  RunLaunchTargetKind,
  RunLaunchWorkflowOptions,
} from "../../../runtime/types";

export type RunLaunchDraft = {
  workflow: Workflow;
  profile: ExecutionProfile;
  configuration: GrillConfiguration;
  configurationTouched: boolean;
  language: GrillLanguage;
  initialPrompt: string;
};

export type CheckoutApprovalState = {
  dirtyRepositoryCount: number;
  sharedPathCount: number;
  dirtyConfirmed: boolean;
  sharedConfirmed: boolean;
};

export function workflowOptions(
  options: RunLaunchOptions,
  workflow: Workflow,
): RunLaunchWorkflowOptions {
  const result = options.workflows.find((candidate) => candidate.workflow === workflow);
  if (!result) throw new Error(`Missing launch options for ${workflow}`);
  return result;
}

export function draftForProfile(
  options: RunLaunchOptions,
  target: RunLaunchTargetKind,
  profile: ExecutionProfile,
  initialPrompt = "",
): RunLaunchDraft {
  const preferred = workflowOptions(options, options.defaultWorkflow);
  const selected = preferred.profiles.some((candidate) => candidate.executionProfile === profile)
    ? preferred
    : options.workflows.find((candidate) => candidate.profiles.some((entry) => entry.executionProfile === profile));
  if (!selected) throw new Error(`No workflow offers ${profile} for ${target}`);
  const profileOption = selected.profiles.find((candidate) => candidate.executionProfile === profile)!;
  return {
    workflow: selected.workflow,
    profile,
    configuration: profileOption.configuration,
    configurationTouched: false,
    language: "portuguese",
    initialPrompt,
  };
}

export function initialLaunchDraft(
  options: RunLaunchOptions,
  target: RunLaunchTargetKind,
  initialPrompt = "",
): RunLaunchDraft {
  const selected = workflowOptions(options, options.defaultWorkflow);
  return draftForProfile(options, target, selected.defaultProfile, initialPrompt);
}

export function profileOptionForDraft(options: RunLaunchOptions, draft: RunLaunchDraft) {
  return workflowOptions(options, draft.workflow).profiles.find(
    (candidate) => candidate.executionProfile === draft.profile,
  );
}

export function changeDraftWorkflow(
  options: RunLaunchOptions,
  draft: RunLaunchDraft,
  workflow: Workflow,
): RunLaunchDraft {
  const selected = workflowOptions(options, workflow);
  const profile = selected.defaultProfile;
  const configuration = selected.profiles.find((entry) => entry.executionProfile === profile)!.configuration;
  return { ...draft, workflow, profile, configuration, configurationTouched: false };
}

export function changeDraftProfile(
  options: RunLaunchOptions,
  draft: RunLaunchDraft,
  profile: ExecutionProfile,
): RunLaunchDraft {
  const configuration = profileOptionForDraft(options, { ...draft, profile })?.configuration;
  if (!configuration) throw new Error(`${profile} is not offered by ${draft.workflow}`);
  return { ...draft, profile, configuration: draft.configurationTouched ? draft.configuration : configuration };
}

export function updateDraftConfiguration(
  draft: RunLaunchDraft,
  configuration: GrillConfiguration,
): RunLaunchDraft {
  return { ...draft, configuration, configurationTouched: true };
}

export function launchDraftSignature(draft: RunLaunchDraft): string {
  return JSON.stringify([draft.workflow, draft.profile, draft.configuration, draft.language, draft.initialPrompt]);
}

export function previewIsCurrent(signature: string | undefined, draft: RunLaunchDraft): boolean {
  return signature === launchDraftSignature(draft);
}

export function checkoutApprovalsReady(state: CheckoutApprovalState): boolean {
  return (state.dirtyRepositoryCount === 0 || state.dirtyConfirmed)
    && (state.sharedPathCount === 0 || state.sharedConfirmed);
}
