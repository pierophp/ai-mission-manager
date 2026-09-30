import { type FormEvent, useEffect, useState } from "react";
import { cn } from "cn";
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "../../../components/ui/alert";
import { Badge } from "../../../components/ui/badge";
import { Button } from "../../../components/ui/button";
import { Checkbox } from "../../../components/ui/checkbox";
import {
  NativeSelect,
  NativeSelectOption,
} from "../../../components/ui/native-select";
import { Spinner } from "../../../components/ui/spinner";
import { Textarea } from "../../../components/ui/textarea";
import { errorMessage } from "../../../runtime/errors";
import type {
  DirectRunPreview,
  GrillAgentCatalog,
  GrillConfiguration,
  ItemView,
  RunLaunchOptions,
  Workspace,
  Worktree,
} from "../../../runtime/types";
import type { ExecutionProfile, Workflow } from "../../../runtime/execution-types";
import type { ItemCommands } from "../use-item-commands";
import { workActions } from "../work-commands";
import {
  changeDraftProfile,
  changeDraftWorkflow,
  checkoutApprovalsReady,
  initialLaunchDraft,
  launchDraftSignature,
  previewIsCurrent as isLaunchPreviewCurrent,
  profileOptionForDraft,
  updateDraftConfiguration,
  workflowOptions,
  type RunLaunchDraft,
} from "./run-launch-draft";

/** Where a Run starts: the Project checkouts, or one registered Worktree. */
export type RunLaunchTarget =
  | { kind: "checkout"; workspace: Workspace }
  | {
      kind: "worktree";
      workspace: Workspace;
      worktree: Worktree;
      repositoryName: string;
    };

const profiles: {
  value: ExecutionProfile;
  label: string;
  hint: string;
  placeholder: string;
}[] = [
  {
    value: "autonomous",
    label: "Autonomous",
    hint: "Route the Initial Prompt through poteto-mode and its playbooks.",
    placeholder: "Anything the agent should focus on (optional)",
  },
  {
    value: "plan",
    label: "Plan",
    hint: "Run the pstack multi-phase plan playbook, write the plan, and stop for Go.",
    placeholder: "What should the plan cover? (optional)",
  },
  {
    value: "pstack-review",
    label: "pstack Review",
    hint: "Run interrogate with the configured reviewers and synthesize a read-only verdict.",
    placeholder: "Name the pull request or branch to review.",
  },
  {
    value: "grill",
    label: "Grill",
    hint: "Stress-test a decision with structured questions before acting.",
    placeholder:
      "What decision, assumption, or plan should the Grill stress-test?",
  },
  {
    value: "investigate",
    label: "Investigate",
    hint: "Inspect the relevant code and report findings before changing files.",
    placeholder: "Anything the agent should focus on (optional)",
  },
  {
    value: "implement",
    label: "Implement",
    hint: "Make the change, run the checks, and leave it ready for review.",
    placeholder: "Anything the agent should focus on (optional)",
  },
  {
    value: "review",
    label: "Review",
    hint: "Review the current changes for correctness, regressions, and missing tests.",
    placeholder: "Anything the agent should focus on (optional)",
  },
  {
    value: "custom",
    label: "Custom",
    hint: "The initial prompt is the whole instruction.",
    placeholder: "Tell the agent exactly what to do",
  },
];

const languages = [
  { value: "portuguese", label: "Português" },
  { value: "english", label: "English" },
] as const;

/** The Notes travel in the initial prompt, so only the objective is added. */
const objectiveOnly = {
  includeObjective: true,
  externalObjectIds: [],
};

/**
 * The one inline form that starts a Run, whatever its Execution Profile. The
 * initial prompt starts from the Item's Notes; the composed prompt (with any
 * skill it carries) stays folded away until the user asks to preview it.
 */
export function RunLaunchForm({
  view,
  modelCatalog,
  commands,
  target,
  onClose,
}: {
  view: ItemView;
  modelCatalog: GrillAgentCatalog[];
  commands: ItemCommands;
  target: RunLaunchTarget;
  onClose: () => void;
}) {
  const { isSaving, whileSaving, confirm, workCommand, onChanged } = commands;
  const [launchOptions, setLaunchOptions] = useState<RunLaunchOptions>();
  const [draft, setDraft] = useState<RunLaunchDraft>();

  const [checkoutPreview, setCheckoutPreview] = useState<DirectRunPreview>();
  const [checkoutError, setCheckoutError] = useState<string>();
  const [primaryRepositoryId, setPrimaryRepositoryId] = useState<number>();
  const [dirtyConfirmed, setDirtyConfirmed] = useState(false);
  const [sharedConfirmed, setSharedConfirmed] = useState(false);

  const [previewOpen, setPreviewOpen] = useState(false);
  const [composed, setComposed] = useState<{
    prompt: string;
    original: string;
    signature: string;
  }>();
  const [launchError, setLaunchError] = useState<{
    title: string;
    message: string;
  }>();

  const workspaceId = target.workspace.id;
  useEffect(() => {
    let cancelled = false;
    void whileSaving(async () => {
      try {
        const options = await workCommand.execute(
          workActions.getRunLaunchOptions(view.item.id, target.kind), false,
        );
        if (cancelled) return;
        setLaunchOptions(options);
        setDraft(initialLaunchDraft(options, target.kind, view.item.notes));
      } catch (optionsError) {
        if (!cancelled) setLaunchError({ title: "Could not load Run options", message: errorMessage(optionsError) });
      }
    });
    return () => { cancelled = true; };
  }, [target.kind, view.item.id]);
  useEffect(() => {
    if (target.kind !== "checkout") return;
    let cancelled = false;
    void whileSaving(async () => {
      try {
        const preview = await workCommand.execute(
          workActions.prepareDirectRun(view.item.id, workspaceId, null),
          false,
        );
        if (cancelled) return;
        setCheckoutPreview(preview);
        setPrimaryRepositoryId(
          preview.checkoutDetails.length === 1
            ? preview.checkoutDetails[0].repositoryId
            : undefined,
        );
      } catch (previewError) {
        if (!cancelled) setCheckoutError(errorMessage(previewError));
      }
    });
    return () => {
      cancelled = true;
    };
    // The checkout is prepared once, when the form opens for this target.
  }, [target.kind, workspaceId, view.item.id]);

  if (!launchOptions || !draft) {
    return launchError ? (
      <div className="grid gap-3"><Alert variant="destructive"><AlertTitle>{launchError.title}</AlertTitle><AlertDescription>{launchError.message}</AlertDescription></Alert><Button type="button" variant="outline" onClick={onClose}>Cancel</Button></div>
    ) : <p className="flex items-center gap-2 text-sm text-muted-foreground"><Spinner /> Loading Run options…</p>;
  }
  const projectedOptions = launchOptions;
  const currentDraft = draft;
  const workflow = currentDraft.workflow;
  const profile = currentDraft.profile;
  const configuration = currentDraft.configuration;
  const language = currentDraft.language;
  const initialPrompt = currentDraft.initialPrompt;
  const availableProfiles = workflowOptions(projectedOptions, workflow).profiles
    .map((option) => ({ ...profiles.find((entry) => entry.value === option.executionProfile)!, ...option }));

  const selectedProfile =
    availableProfiles.find((candidate) => candidate.value === profile) ??
    availableProfiles[0];
  const agentCatalog = modelCatalog.find(
    (catalog) => catalog.agent === configuration.agent,
  );
  const selectedModel = agentCatalog?.models.find(
    (model) => model.id === configuration.model,
  );
  const promptMissing =
    Boolean(profileOptionForDraft(projectedOptions, currentDraft)?.requiresInitialPrompt) && !initialPrompt.trim();
  const signature = launchDraftSignature(currentDraft);
  const previewIsCurrent = isLaunchPreviewCurrent(composed?.signature, currentDraft);
  const previewEdited = Boolean(composed && composed.prompt !== composed.original);
  const checkoutReady =
    target.kind === "worktree" ||
    (checkoutPreview !== undefined &&
      primaryRepositoryId !== undefined &&
      checkoutApprovalsReady({
        dirtyRepositoryCount: checkoutPreview.dirtyRepositoryIds.length,
        sharedPathCount: checkoutPreview.sharedPaths.length,
        dirtyConfirmed,
        sharedConfirmed,
      }));
  const canStart =
    !isSaving &&
    checkoutReady &&
    Boolean(selectedModel) &&
    !promptMissing &&
    !(previewIsCurrent && !composed?.prompt.trim());

  function selectProfile(next: ExecutionProfile) {
    setDraft(changeDraftProfile(projectedOptions, currentDraft, next));
  }

  function selectWorkflow(next: Workflow) {
    setDraft(changeDraftWorkflow(projectedOptions, currentDraft, next));
  }
  function updateConfiguration(next: GrillConfiguration) {
    setDraft(updateDraftConfiguration(currentDraft, next));
  }

  function composeAction() {
    return profile === "grill"
      ? workActions.composeGrillPrompt(
          view.item.id,
          configuration,
          language,
          initialPrompt,
        )
      : workActions.composeRunPrompt(
          view.item.id,
          profile,
          objectiveOnly,
          language,
          initialPrompt.trim() ? initialPrompt : null,
          workflow,
        );
  }

  async function composePreview() {
    if (promptMissing || !selectedModel) return;
    setLaunchError(undefined);
    await whileSaving(async () => {
      try {
        const prompt = await workCommand.execute(composeAction(), false);
        setComposed({ prompt, original: prompt, signature });
      } catch (composeError) {
        setLaunchError({
          title: "Could not compose the prompt",
          message: errorMessage(composeError),
        });
      }
    });
  }

  function refreshPreview() {
    if (previewEdited) {
      confirm({
        title: "Replace your edited prompt?",
        description: "Composing again replaces the prompt you edited.",
        confirmLabel: "Replace prompt",
        onConfirm: () => void composePreview(),
      });
      return;
    }
    void composePreview();
  }

  function togglePreview() {
    const opening = !previewOpen;
    setPreviewOpen(opening);
    if (opening && !previewIsCurrent && !previewEdited) void composePreview();
  }

  function startAction(prompt: string) {
    const agent = configuration.agent;
    if (target.kind === "worktree") {
      return workActions.startRun({
        itemId: view.item.id,
        workspaceId,
        strategy: {
          kind: "worktree",
          worktreeId: target.worktree.id,
          agent,
          configuration,
          executionProfile: profile,
          workflow,
          prompt,
          promptSelection: objectiveOnly,
        },
      });
    }
    return profile === "grill"
      ? workActions.startRun({
          itemId: view.item.id,
          workspaceId,
          strategy: {
            kind: "grill",
            machineId: null,
            primaryRepositoryId: primaryRepositoryId!,
            language,
            configuration,
            prompt,
            expectedCheckouts: checkoutPreview!.checkouts,
            allowDirty: checkoutPreview!.dirtyRepositoryIds.length > 0,
            allowSharedCheckouts: checkoutPreview!.sharedPaths.length > 0,
          },
        })
      : workActions.startRun({
          itemId: view.item.id,
          workspaceId,
          strategy: {
            kind: "direct",
            machineId: null,
            primaryRepositoryId: primaryRepositoryId!,
            agent,
            configuration,
            executionProfile: profile,
            workflow,
            prompt,
            promptSelection: objectiveOnly,
            expectedCheckouts: checkoutPreview!.checkouts,
            allowDirty: checkoutPreview!.dirtyRepositoryIds.length > 0,
            allowSharedCheckouts: checkoutPreview!.sharedPaths.length > 0,
          },
        });
  }

  async function handleStart(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!canStart) return;
    setLaunchError(undefined);
    await whileSaving(async () => {
      try {
        const prompt =
          previewIsCurrent && composed
            ? composed.prompt
            : await workCommand.execute(composeAction(), false);
        await workCommand.execute(startAction(prompt));
        await onChanged();
        // The new Run appears in the Runs list, with its questions for a Grill.
        onClose();
      } catch (startError) {
        setLaunchError({
          title: "Could not start the Run",
          message: errorMessage(startError),
        });
      }
    });
  }

  const machineName = checkoutPreview?.machineName;

  return (
    <form
      className="grid gap-5 rounded-lg border border-primary/30 bg-primary/5 p-4"
      onSubmit={(event) => void handleStart(event)}
    >
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h4 className="m-0 text-base font-medium">
            {target.kind === "worktree" ? "Start Worktree Run" : "Start Run"}
          </h4>
          <p className="mt-1 text-sm text-muted-foreground">
            {selectedProfile.hint}
          </p>
        </div>
        {machineName && <Badge variant="outline">Machine: {machineName}</Badge>}
      </div>

      <label className="grid gap-1.5 text-sm font-medium">
        <span>Workflow</span>
        <NativeSelect value={workflow} onChange={(event) => selectWorkflow(event.target.value as Workflow)} disabled={isSaving}>
          {launchOptions.workflows.map((option) => <NativeSelectOption key={option.workflow} value={option.workflow}>{option.workflow === "pstack" ? "pstack" : "Matt Pocock"}</NativeSelectOption>)}
        </NativeSelect>
      </label>
      <SegmentedChoice
        legend="Execution Profile"
        name="run-execution-profile"
        options={availableProfiles}
        value={profile}
        onChange={selectProfile}
        disabled={isSaving}
      />

      {target.kind === "worktree" ? (
        <div className="grid gap-1 rounded-md border p-3 text-sm">
          <span className="font-medium">Worktree</span>
          <span>
            {target.repositoryName} · <code>{target.worktree.branch}</code>
          </span>
          <code className="break-all text-xs text-muted-foreground">
            {target.worktree.path}
          </code>
        </div>
      ) : (
        <CheckoutSection
          preview={checkoutPreview}
          error={checkoutError}
          primaryRepositoryId={primaryRepositoryId}
          onPrimaryRepositoryChange={setPrimaryRepositoryId}
          dirtyConfirmed={dirtyConfirmed}
          onDirtyConfirmedChange={setDirtyConfirmed}
          sharedConfirmed={sharedConfirmed}
          onSharedConfirmedChange={setSharedConfirmed}
          disabled={isSaving}
        />
      )}

      <div className="grid gap-3 md:grid-cols-3">
        <label className="grid gap-1.5 text-sm font-medium">
          <span>Agent</span>
          <NativeSelect
            value={configuration.agent}
            onChange={(event) => {
              const agent = event.target.value as GrillConfiguration["agent"];
              const model = modelCatalog.find(
                (catalog) => catalog.agent === agent,
              )?.models[0];
              updateConfiguration({
                agent,
                model: model?.id ?? "",
                effort: model?.efforts[0]?.id ?? "",
              });
            }}
            disabled={isSaving || modelCatalog.length === 0}
          >
            {modelCatalog.map((catalog) => (
              <NativeSelectOption value={catalog.agent} key={catalog.agent}>
                {catalog.agent === "claude" ? "Claude Code" : "Codex"}
              </NativeSelectOption>
            ))}
          </NativeSelect>
        </label>
        <label className="grid gap-1.5 text-sm font-medium">
          <span>Model</span>
          <NativeSelect
            value={configuration.model}
            onChange={(event) => {
              const model = agentCatalog?.models.find(
                (candidate) => candidate.id === event.target.value,
              );
              updateConfiguration({
                ...configuration,
                model: event.target.value,
                effort: model?.efforts[0]?.id ?? "",
              });
            }}
            disabled={isSaving || !agentCatalog}
          >
            {agentCatalog?.models.map((model) => (
              <NativeSelectOption value={model.id} key={model.id}>
                {model.label} ({model.id})
              </NativeSelectOption>
            ))}
          </NativeSelect>
        </label>
        <label className="grid gap-1.5 text-sm font-medium">
          <span>Effort</span>
          <NativeSelect
            value={configuration.effort}
            onChange={(event) =>
              updateConfiguration({
                ...configuration,
                effort: event.target.value,
              })
            }
            disabled={isSaving || !selectedModel}
          >
            {selectedModel?.efforts.map((effort) => (
              <NativeSelectOption value={effort.id} key={effort.id}>
                {effort.label} ({effort.id})
              </NativeSelectOption>
            ))}
          </NativeSelect>
        </label>
      </div>

      <SegmentedChoice
        legend="Response language"
        name="run-response-language"
        options={languages}
        value={language}
        onChange={(next) => setDraft({ ...currentDraft, language: next })}
        disabled={isSaving}
      />

      <label className="grid gap-1.5 text-sm font-medium">
        <span className="flex flex-wrap items-baseline justify-between gap-2">
          Initial prompt
          <span className="text-xs font-normal text-muted-foreground">
            Starts from the Item&apos;s Notes. Edits here stay with this Run.
          </span>
        </span>
        <Textarea
          autoFocus
          value={initialPrompt}
          onChange={(event) => setDraft({ ...currentDraft, initialPrompt: event.target.value })}
          rows={5}
          placeholder={selectedProfile.placeholder}
          disabled={isSaving}
          aria-invalid={promptMissing || undefined}
        />
        {promptMissing && (
          <span className="text-xs font-normal text-muted-foreground">
            {profile === "custom"
              ? "A Custom Run sends only this prompt, so write what the agent should do."
              : profile === "pstack-review"
                ? "Name the pull request or branch to review."
                : "Tell the Grill what to stress-test."}
          </span>
        )}
      </label>

      <div className="grid gap-2 rounded-md border bg-background/60">
        <button
          type="button"
          className="flex items-center justify-between gap-2 px-3 py-2 text-left text-sm font-medium"
          aria-expanded={previewOpen}
          onClick={togglePreview}
          disabled={isSaving && !previewOpen}
        >
          <span>Preview prompt</span>
          <span className="text-xs font-normal text-muted-foreground">
            {previewOpen
              ? "Hide"
              : "See and edit the full prompt, including any skill it carries"}
          </span>
        </button>
        {previewOpen && (
          <div className="grid gap-2 border-t px-3 pb-3 pt-2">
            {promptMissing && !composed && (
              <p className="m-0 text-sm text-muted-foreground">
                Write an initial prompt to preview it.
              </p>
            )}
            {composed && !previewIsCurrent && (
              <Alert>
                <AlertTitle>The settings changed after this preview</AlertTitle>
                <AlertDescription>
                  Start composes a fresh prompt from the current settings
                  {previewEdited ? " and drops your edits" : ""}.
                  <div className="mt-2">
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      disabled={isSaving || promptMissing || !selectedModel}
                      onClick={refreshPreview}
                    >
                      Refresh preview
                    </Button>
                  </div>
                </AlertDescription>
              </Alert>
            )}
            {composed ? (
              <Textarea
                aria-label="Composed prompt"
                value={composed.prompt}
                onChange={(event) =>
                  setComposed({ ...composed, prompt: event.target.value })
                }
                rows={12}
                disabled={isSaving}
              />
            ) : (
              !promptMissing && (
                <p className="m-0 flex items-center gap-2 text-sm text-muted-foreground">
                  <Spinner /> Composing prompt…
                </p>
              )
            )}
          </div>
        )}
      </div>

      {previewEdited && !previewIsCurrent && !previewOpen && (
        <Alert>
          <AlertTitle>Your prompt edits are out of date</AlertTitle>
          <AlertDescription>
            The settings changed after you edited the preview. Start composes a
            fresh prompt and drops those edits; open the preview to refresh it.
          </AlertDescription>
        </Alert>
      )}

      {launchError && (
        <Alert variant="destructive">
          <AlertTitle>{launchError.title}</AlertTitle>
          <AlertDescription>{launchError.message}</AlertDescription>
        </Alert>
      )}

      <div className="flex flex-wrap gap-2">
        <Button type="submit" disabled={!canStart}>
          {isSaving ? "Starting…" : "Start Run"}
        </Button>
        <Button
          type="button"
          size="sm"
          variant="ghost"
          disabled={isSaving}
          onClick={onClose}
        >
          Cancel
        </Button>
      </div>
    </form>
  );
}

function SegmentedChoice<T extends string>({
  legend,
  name,
  options,
  value,
  onChange,
  disabled,
}: {
  legend: string;
  name: string;
  options: readonly { value: T; label: string }[];
  value: T;
  onChange: (value: T) => void;
  disabled: boolean;
}) {
  return (
    <fieldset className="grid gap-2 text-sm font-medium" disabled={disabled}>
      <legend className="mb-2">{legend}</legend>
      <div className="inline-flex w-fit flex-wrap rounded-md border bg-background/60 p-1">
        {options.map((option) => (
          <label
            className={cn(
              "cursor-pointer rounded px-3 py-1.5 text-sm transition-colors",
              value === option.value
                ? "bg-primary text-primary-foreground"
                : "text-muted-foreground hover:bg-muted hover:text-foreground",
            )}
            key={option.value}
          >
            <input
              checked={value === option.value}
              className="peer sr-only"
              name={name}
              onChange={() => onChange(option.value)}
              type="radio"
              value={option.value}
            />
            <span className="peer-focus-visible:outline peer-focus-visible:outline-2 peer-focus-visible:outline-ring">
              {option.label}
            </span>
          </label>
        ))}
      </div>
    </fieldset>
  );
}

function CheckoutSection({
  preview,
  error,
  primaryRepositoryId,
  onPrimaryRepositoryChange,
  dirtyConfirmed,
  onDirtyConfirmedChange,
  sharedConfirmed,
  onSharedConfirmedChange,
  disabled,
}: {
  preview: DirectRunPreview | undefined;
  error: string | undefined;
  primaryRepositoryId: number | undefined;
  onPrimaryRepositoryChange: (repositoryId: number | undefined) => void;
  dirtyConfirmed: boolean;
  onDirtyConfirmedChange: (confirmed: boolean) => void;
  sharedConfirmed: boolean;
  onSharedConfirmedChange: (confirmed: boolean) => void;
  disabled: boolean;
}) {
  if (error) {
    return (
      <Alert variant="destructive">
        <AlertTitle>Could not prepare the checkout</AlertTitle>
        <AlertDescription>
          <p>{error}</p>
          <p>
            Register this Repository&apos;s checkout for the Context execution
            Machine under Settings, then start the Run again.
          </p>
        </AlertDescription>
      </Alert>
    );
  }
  if (!preview) {
    return (
      <p className="m-0 flex items-center gap-2 text-sm text-muted-foreground">
        <Spinner /> Preparing checkout preview…
      </p>
    );
  }
  return (
    <div className="grid gap-3">
      <div className="grid gap-2 rounded-md border p-3 text-sm">
        <p className="m-0 font-medium">Registered checkouts</p>
        {preview.checkoutDetails.map((checkout) => (
          <div
            className="flex flex-wrap justify-between gap-2"
            key={checkout.repositoryId}
          >
            <span>
              {checkout.repositoryName} · {checkout.branch}
            </span>
            <code className="break-all text-xs text-muted-foreground">
              {checkout.path}
            </code>
          </div>
        ))}
      </div>
      {preview.checkoutDetails.length > 1 && (
        <label className="grid gap-1.5 text-sm font-medium">
          <span>Primary Repository / working directory</span>
          <NativeSelect
            value={primaryRepositoryId ?? ""}
            onChange={(event) =>
              onPrimaryRepositoryChange(Number(event.target.value) || undefined)
            }
            disabled={disabled}
          >
            <NativeSelectOption value="">
              Choose the checkout for this Run
            </NativeSelectOption>
            {preview.checkoutDetails.map((checkout) => (
              <NativeSelectOption
                value={checkout.repositoryId}
                key={checkout.repositoryId}
              >
                {checkout.repositoryName} · {checkout.path}
              </NativeSelectOption>
            ))}
          </NativeSelect>
        </label>
      )}
      {new Set(preview.currentBranches).size > 1 && (
        <Alert>
          <AlertTitle>Repositories are on different branches</AlertTitle>
          <AlertDescription>
            {preview.currentBranches.join(", ")}. The Run will use each
            checkout&apos;s current branch without switching it.
          </AlertDescription>
        </Alert>
      )}
      {preview.dirtyRepositoryIds.length > 0 && (
        <Alert variant="destructive">
          <AlertTitle>Checkout has local changes</AlertTitle>
          <AlertDescription>
            Existing uncommitted changes will remain in the checkout.
            <label className="mt-2 flex items-center gap-2 font-normal">
              <Checkbox
                checked={dirtyConfirmed}
                onCheckedChange={(checked) =>
                  onDirtyConfirmedChange(checked === true)
                }
                disabled={disabled}
              />
              I understand and want to use the dirty checkout.
            </label>
          </AlertDescription>
        </Alert>
      )}
      {preview.sharedPaths.length > 0 && (
        <Alert variant="destructive">
          <AlertTitle>Checkout is shared with an active Run</AlertTitle>
          <AlertDescription>
            {preview.sharedRuns.map((shared) => (
              <div key={`${shared.runId}-${shared.path}`}>
                Run #{shared.runId} · {shared.path}
              </div>
            ))}
            <label className="mt-2 flex items-center gap-2 font-normal">
              <Checkbox
                checked={sharedConfirmed}
                onCheckedChange={(checked) =>
                  onSharedConfirmedChange(checked === true)
                }
                disabled={disabled}
              />
              I understand and want to share this checkout.
            </label>
          </AlertDescription>
        </Alert>
      )}
    </div>
  );
}
