import { type FormEvent, useEffect, useMemo, useState } from "react";

import { Alert, AlertDescription } from "../../components/ui/alert";
import { Button } from "../../components/ui/button";
import { Checkbox } from "../../components/ui/checkbox";
import { Input } from "../../components/ui/input";
import {
  NativeSelect,
  NativeSelectOption,
} from "../../components/ui/native-select";
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "../../components/ui/tabs";
import { errorMessage } from "../../runtime/errors";
import type {
  Context,
  ContextAttentionDefault,
  ContextConfiguration,
  CliProfileSettingsView,
  ExternalChangePolicy,
  ExternalObjectKind,
  GrillAgentCatalog,
  GrillConfiguration,
  Machine,
  PstackRole,
  PstackRoleTable,
} from "../../runtime/types";

const objectKinds: ExternalObjectKind[] = [
  "issue",
  "pull_request",
  "document",
  "generic",
];
const agentLabels: Record<GrillConfiguration["agent"], string> = {
  claude: "Claude Code",
  codex: "Codex",
};
const defaultPolicy: ExternalChangePolicy = {
  title: true,
  state: true,
  metadata: true,
};
const defaultGrillConfiguration = (): GrillConfiguration => ({
  agent: "claude",
  model: "claude-sonnet-5",
  effort: "high",
});
const defaultPstackRoleTable = (): PstackRoleTable => [
  { role: "code-delegate", configuration: { agent: "claude", model: "claude-opus-5", effort: "high" } },
  { role: "judge-and-prose", configuration: { agent: "codex", model: "gpt-6-sol", effort: "high" } },
  { role: "review-panel", configuration: { agent: "codex", model: "gpt-6-sol", effort: "high" } },
  { role: "explorers", configuration: { agent: "claude", model: "claude-sonnet-5", effort: "medium" } },
];
const pstackRoleLabels: Record<PstackRole, string> = {
  "code-delegate": "Code delegate",
  "judge-and-prose": "Judge and prose",
  "review-panel": "Review panel",
  explorers: "Explorers",
};

function initialConfiguration(
  context: Context | undefined,
  attentionDefaults: ContextAttentionDefault[],
): ContextConfiguration {
  return {
    name: context?.name ?? "",
    executionMachineId: context?.execution_machine_id ?? null,
    claudeProfileId: context?.claude_profile_id ?? null,
    codexProfileId: context?.codex_profile_id ?? null,
    checkDirtyCheckouts: context?.check_dirty_checkouts ?? true,
    grillDefaults: context?.grill_defaults ?? defaultGrillConfiguration(),
    implementDefaults:
      context?.implement_defaults ?? defaultGrillConfiguration(),
    defaultWorkflow: context?.default_workflow ?? "matt-pocock",
    pstackDefaults: context?.pstack_defaults ?? defaultGrillConfiguration(),
    pstackRoles: context?.pstack_roles ?? defaultPstackRoleTable(),
    ghExecutablePath: context?.gh_executable_path ?? null,
    twgExecutablePath: context?.twg_executable_path ?? null,
    azExecutablePath: context?.az_executable_path ?? null,
    atlassianSite: context?.atlassian_site ?? null,
    azureDevopsOrganization: context?.azure_devops_organization ?? null,
    bitbucketWorkspace: context?.bitbucket_workspace ?? null,
    attentionDefaults: objectKinds.map(
      (object_kind) =>
        attentionDefaults.find(
          (entry) => entry.object_kind === object_kind,
        ) ?? {
          context_id: context?.id ?? 0,
          object_kind,
          policy: defaultPolicy,
        },
    ),
  };
}

function SettingField({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <label className="grid gap-1.5 text-sm">
      <span className="font-medium">{label}</span>
      {children}
    </label>
  );
}

function ConfigurationFields({
  label,
  configuration,
  catalog,
  disabled,
  onChange,
}: {
  label: string;
  configuration: GrillConfiguration;
  catalog: GrillAgentCatalog[];
  disabled: boolean;
  onChange: (configuration: GrillConfiguration) => void;
}) {
  const agentCatalog = catalog.find(
    (entry) => entry.agent === configuration.agent,
  );
  const model = agentCatalog?.models.find(
    (entry) => entry.id === configuration.model,
  );
  return (
    <fieldset className="grid gap-3 rounded-md border p-3 sm:grid-cols-3">
      <legend className="px-1 text-sm font-semibold">{label} defaults</legend>
      <SettingField label={`${label} agent`}>
        <NativeSelect
          value={configuration.agent}
          disabled={disabled || catalog.length === 0}
          onChange={(event) => {
            const agent = event.target.value as GrillConfiguration["agent"];
            const firstModel = catalog.find((entry) => entry.agent === agent)
              ?.models[0];
            onChange({
              agent,
              model: firstModel?.id ?? "",
              effort: firstModel?.efforts[0]?.id ?? "",
            });
          }}
        >
          {catalog.map((entry) => (
            <NativeSelectOption key={entry.agent} value={entry.agent}>
              {agentLabels[entry.agent]}
            </NativeSelectOption>
          ))}
        </NativeSelect>
      </SettingField>
      <SettingField label={`${label} model`}>
        <NativeSelect
          value={configuration.model}
          disabled={disabled || !agentCatalog}
          onChange={(event) => {
            const nextModel = agentCatalog?.models.find(
              (entry) => entry.id === event.target.value,
            );
            onChange({
              ...configuration,
              model: event.target.value,
              effort: nextModel?.efforts[0]?.id ?? "",
            });
          }}
        >
          {agentCatalog?.models.map((entry) => (
            <NativeSelectOption key={entry.id} value={entry.id}>
              {entry.label}
            </NativeSelectOption>
          ))}
        </NativeSelect>
      </SettingField>
      <SettingField label={`${label} effort`}>
        <NativeSelect
          value={configuration.effort}
          disabled={disabled || !model}
          onChange={(event) =>
            onChange({ ...configuration, effort: event.target.value })
          }
        >
          {model?.efforts.map((entry) => (
            <NativeSelectOption key={entry.id} value={entry.id}>
              {entry.label}
            </NativeSelectOption>
          ))}
        </NativeSelect>
      </SettingField>
    </fieldset>
  );
}

export function ContextEditor({
  context,
  attentionDefaults,
  machines,
  profiles,
  catalog,
  isSaving,
  onSave,
  onCancel,
  onDirtyChange,
}: {
  context: Context | undefined;
  attentionDefaults: ContextAttentionDefault[];
  machines: Machine[];
  profiles: CliProfileSettingsView[];
  catalog: GrillAgentCatalog[];
  isSaving: boolean;
  onSave: (configuration: ContextConfiguration) => Promise<void>;
  onCancel: () => void;
  onDirtyChange: (dirty: boolean) => void;
}) {
  const savedConfiguration = useMemo(
    () =>
      initialConfiguration(
        context,
        attentionDefaults.filter((entry) => entry.context_id === context?.id),
      ),
    [attentionDefaults, context],
  );
  const [configuration, setConfiguration] = useState(savedConfiguration);
  const [error, setError] = useState<string>();
  const executionMachine = machines.find(
    (machine) => machine.id === configuration.executionMachineId,
  );
  const machineProfiles = profiles.filter(
    ({ profile }) => profile.machineId === executionMachine?.id,
  );

  useEffect(() => {
    setConfiguration(savedConfiguration);
    setError(undefined);
  }, [savedConfiguration]);

  useEffect(() => {
    onDirtyChange(
      JSON.stringify(configuration) !== JSON.stringify(savedConfiguration),
    );
  }, [configuration, onDirtyChange, savedConfiguration]);

  function setAttentionPolicy(
    kind: ExternalObjectKind,
    key: keyof ExternalChangePolicy,
    checked: boolean,
  ) {
    setConfiguration((current) => ({
      ...current,
      attentionDefaults: current.attentionDefaults.map((entry) =>
        entry.object_kind === kind
          ? { ...entry, policy: { ...entry.policy, [key]: checked } }
          : entry,
      ),
    }));
  }

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    try {
      await onSave({
        ...configuration,
        name: configuration.name.trim(),
      });
      setError(undefined);
    } catch (saveError) {
      setError(errorMessage(saveError));
    }
  }

  return (
    <form
      className="grid gap-4 border-t border-border/70 pt-4"
      onSubmit={(event) => void submit(event)}
    >
      <header>
        <h3 className="m-0 text-base font-semibold">
          {context ? `Edit ${context.name}` : "Create Context"}
        </h3>
        <p className="mb-0 mt-1 text-sm text-muted-foreground">
          {context
            ? "Update this Context and its settings together."
            : "Create a Context with its settings together."}
        </p>
      </header>
      {error && (
        <Alert variant="destructive">
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}
      <Tabs defaultValue="primary" className="grid gap-4">
        <TabsList aria-label="Context settings">
        <TabsTrigger value="primary">Primary settings</TabsTrigger>
        <TabsTrigger value="pstack">pstack</TabsTrigger>
          <TabsTrigger value="providers">Providers</TabsTrigger>
          <TabsTrigger value="attention">Needs Attention</TabsTrigger>
        </TabsList>
        <TabsContent value="primary" forceMount className="grid gap-4">
          <SettingField label="Context name">
            <Input
              value={configuration.name}
              onChange={(event) =>
                setConfiguration((current) => ({
                  ...current,
                  name: event.target.value,
                }))
              }
              disabled={isSaving}
            />
          </SettingField>
          <SettingField label="Execution Machine">
            <NativeSelect
              value={configuration.executionMachineId ?? ""}
              disabled={isSaving}
              onChange={(event) =>
                setConfiguration((current) => ({
                  ...current,
                  executionMachineId: Number(event.target.value) || null,
                }))
              }
            >
              <NativeSelectOption value="">
                No execution Machine
              </NativeSelectOption>
              {machines.map((machine) => (
                <NativeSelectOption key={machine.id} value={machine.id}>
                  {machine.name}
                </NativeSelectOption>
              ))}
            </NativeSelect>
          </SettingField>
          {!executionMachine && (
            <p className="m-0 text-sm text-muted-foreground">
              Runs cannot start until this Context has an execution Machine.
            </p>
          )}
          {(["claude", "codex"] as const).map((provider) => {
            const field =
              provider === "claude" ? "claudeProfileId" : "codexProfileId";
            const selected = configuration[field];
            return (
              <SettingField
                key={provider}
                label={`${agentLabels[provider]} Agent CLI Configuration Profile`}
              >
                <NativeSelect
                  value={selected ?? ""}
                  disabled={isSaving}
                  onChange={(event) =>
                    setConfiguration((current) => ({
                      ...current,
                      [field]: Number(event.target.value) || null,
                    }))
                  }
                >
                  <NativeSelectOption value="">
                    Use standard CLI configuration
                  </NativeSelectOption>
                  {selected !== null &&
                    !machineProfiles.some(
                      ({ profile }) => profile.id === selected,
                    ) && (
                      <NativeSelectOption value={selected} disabled>
                        Selected profile is unavailable on this Machine; clear
                        or replace it
                      </NativeSelectOption>
                    )}
                  {machineProfiles
                    .filter(({ profile }) => profile.provider === provider)
                    .map(({ profile }) => (
                      <NativeSelectOption key={profile.id} value={profile.id}>
                        {profile.name}
                      </NativeSelectOption>
                    ))}
                </NativeSelect>
              </SettingField>
            );
          })}
          <label className="flex items-start gap-2 text-sm">
            <Checkbox
              checked={configuration.checkDirtyCheckouts}
              onCheckedChange={(checked) =>
                setConfiguration((current) => ({
                  ...current,
                  checkDirtyCheckouts: checked === true,
                }))
              }
              disabled={isSaving}
            />
            <span>
              Check for dirty checkouts before Direct and Grill Runs and when
              advancing an Implementation Queue.
            </span>
          </label>
          <ConfigurationFields
            label="Grill"
            configuration={configuration.grillDefaults}
            catalog={catalog}
            disabled={isSaving}
            onChange={(grillDefaults) =>
              setConfiguration((current) => ({ ...current, grillDefaults }))
            }
          />
          <ConfigurationFields
            label="Implement"
            configuration={configuration.implementDefaults}
            catalog={catalog}
            disabled={isSaving}
            onChange={(implementDefaults) =>
              setConfiguration((current) => ({ ...current, implementDefaults }))
            }
          />
          <SettingField label="Default Workflow">
            <NativeSelect
              value={configuration.defaultWorkflow}
              onChange={(event) => setConfiguration((current) => ({
                ...current,
                defaultWorkflow: event.target.value as ContextConfiguration["defaultWorkflow"],
              }))}
              disabled={isSaving}
            >
              <NativeSelectOption value="matt-pocock">Matt Pocock</NativeSelectOption>
              <NativeSelectOption value="pstack">pstack</NativeSelectOption>
            </NativeSelect>
          </SettingField>
          <ConfigurationFields
            label="pstack"
            configuration={configuration.pstackDefaults}
            catalog={catalog}
            disabled={isSaving}
            onChange={(pstackDefaults) =>
              setConfiguration((current) => ({ ...current, pstackDefaults }))
            }
          />
        </TabsContent>
        <TabsContent value="pstack" forceMount className="grid gap-4">
          <p className="m-0 text-sm text-muted-foreground">
            Choose the CLI, model and effort pstack assigns to each role. Models and efforts come from each provider's model catalog.
          </p>
          {configuration.pstackRoles.map(({ role, configuration: roleConfiguration }) => (
            <ConfigurationFields
              key={role}
              label={pstackRoleLabels[role]}
              configuration={roleConfiguration}
              catalog={catalog}
              disabled={isSaving}
              onChange={(nextConfiguration) =>
                setConfiguration((current) => ({
                  ...current,
                  pstackRoles: current.pstackRoles.map((entry) =>
                    entry.role === role
                      ? { ...entry, configuration: nextConfiguration }
                      : entry,
                  ),
                }))
              }
            />
          ))}
        </TabsContent>
        <TabsContent value="providers" forceMount className="grid gap-4">
          <p className="m-0 text-sm text-muted-foreground">
            Configure executable locations and provider identifiers for this Context. Authentication stays in each CLI's own configuration.
          </p>
          {([
            ["ghExecutablePath", "GitHub CLI executable (`gh`)", "gh"],
            ["twgExecutablePath", "Atlassian CLI executable (`twg`)", "twg"],
            ["azExecutablePath", "Azure CLI executable (`az`)", "az"],
            ["atlassianSite", "Atlassian site", "company.atlassian.net"],
            ["azureDevopsOrganization", "Azure DevOps organization", "organization"],
            ["bitbucketWorkspace", "Bitbucket workspace", "workspace"],
          ] as const).map(([field, label, placeholder]) => (
            <SettingField key={field} label={label}>
              <Input
                value={configuration[field] ?? ""}
                placeholder={placeholder}
                autoComplete="off"
                onChange={(event) =>
                  setConfiguration((current) => ({
                    ...current,
                    [field]: event.target.value.trim() || null,
                  }))
                }
                disabled={isSaving}
              />
            </SettingField>
          ))}
          <p className="m-0 text-sm text-muted-foreground">
            No access tokens are stored by Mission Manager.
          </p>
        </TabsContent>
        <TabsContent value="attention" forceMount className="grid gap-3">
          <p className="m-0 text-sm text-muted-foreground">
            Choose which changes to titles, state, and metadata call for
            attention on Links in this Context.
          </p>
          {configuration.attentionDefaults.map((entry) => (
            <fieldset
              key={entry.object_kind}
              className="grid gap-3 rounded-md border p-3 sm:grid-cols-[1fr_repeat(3,auto)] sm:items-center"
            >
              <legend className="px-1 text-sm font-semibold">
                {entry.object_kind === "issue"
                  ? "Issue"
                  : entry.object_kind === "pull_request"
                    ? "Pull Request"
                    : "Generic External Object"}
              </legend>
              {(["title", "state", "metadata"] as const).map((key) => (
                <label key={key} className="flex items-center gap-2 text-sm">
                  <Checkbox
                    checked={entry.policy[key]}
                    onCheckedChange={(checked) =>
                      setAttentionPolicy(
                        entry.object_kind,
                        key,
                        checked === true,
                      )
                    }
                    disabled={isSaving}
                  />
                  {key[0].toUpperCase() + key.slice(1)}
                </label>
              ))}
            </fieldset>
          ))}
        </TabsContent>
      </Tabs>
      <footer className="flex justify-end gap-2">
        <Button
          type="button"
          variant="outline"
          onClick={onCancel}
          disabled={isSaving}
        >
          Cancel
        </Button>
        <Button type="submit" disabled={isSaving || !configuration.name.trim()}>
          {context ? "Save Context" : "Create Context"}
        </Button>
      </footer>
    </form>
  );
}
