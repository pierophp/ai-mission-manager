import { type FormEvent, useEffect, useMemo, useState } from "react";

import { Alert, AlertDescription } from "../../components/ui/alert";
import { Badge } from "../../components/ui/badge";
import { Button } from "../../components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "../../components/ui/card";
import { Checkbox } from "../../components/ui/checkbox";
import { EmptyDescription } from "../../components/ui/empty";
import { Input } from "../../components/ui/input";
import {
  NativeSelect,
  NativeSelectOption,
} from "../../components/ui/native-select";
import { structureAdapter } from "../../runtime/adapters";
import { useAppRuntime } from "../../runtime/AppRuntimeProvider";
import { errorMessage } from "../../runtime/errors";
import type {
  Context,
  ExternalChangePolicy,
  ExternalObjectKind,
  ItemStatus,
  MachineDeletionPreview,
  MachineTransport,
  ParentDeletionPreview,
  ParentDeletionResult,
  RepositoryDeletionPreview,
  ResetLocalDataPreview,
} from "../../runtime/types";
import {
  externalObjectKindLabel,
  flattenHome,
  uniqueItems,
} from "../work/work-utils";

const itemStatuses: ItemStatus[] = ["Inbox", "Active", "Waiting", "Done"];
const objectKinds: ExternalObjectKind[] = ["issue", "pull_request", "generic"];
const defaultAttentionPolicy: ExternalChangePolicy = {
  title: true,
  state: true,
  metadata: true,
};

type StructurePageProps = {
  onResetComplete?: () => void;
};

export function StructurePage({ onResetComplete }: StructurePageProps) {
  const {
    structure,
    home,
    setError,
    refreshStructure,
    refreshHome,
    refreshSearch,
    refreshRunSuggestions,
    refreshActivity,
    refreshAll,
  } = useAppRuntime();
  const { contexts, projects, repositories, machines, attentionDefaults } = structure;
  const allItems = useMemo(
    () => uniqueItems(home ? flattenHome(home) : []),
    [home],
  );

  const [selectedContextId, setSelectedContextId] = useState<number>();
  const [selectedProjectId, setSelectedProjectId] = useState<number>();
  const [contextName, setContextName] = useState("");
  const [projectName, setProjectName] = useState("");
  const [projectDefaultStatus, setProjectDefaultStatus] =
    useState<ItemStatus>("Inbox");
  const [repositoryName, setRepositoryName] = useState("");
  const [repositoryRemoteUrl, setRepositoryRemoteUrl] = useState("");
  const [machineName, setMachineName] = useState("");
  const [machineSocketName, setMachineSocketName] = useState(
    "ai-mission-manager",
  );
  const [machineKind, setMachineKind] = useState<"local" | "ssh">("ssh");
  const [machineHost, setMachineHost] = useState("");
  const [machineUser, setMachineUser] = useState("");
  const [machinePort, setMachinePort] = useState("");
  const [machineIdentityFile, setMachineIdentityFile] = useState("");
  const [machineKnownHostsFile, setMachineKnownHostsFile] = useState("");
  const [machineStrictHostKeyChecking, setMachineStrictHostKeyChecking] =
    useState("accept-new");
  const [attentionObjectKind, setAttentionObjectKind] =
    useState<ExternalObjectKind>("pull_request");
  const [attentionDefaultPolicy, setAttentionDefaultPolicy] =
    useState<ExternalChangePolicy>(defaultAttentionPolicy);
  const [repositoryDeletionPreview, setRepositoryDeletionPreview] =
    useState<RepositoryDeletionPreview>();
  const [parentDeletionPreview, setParentDeletionPreview] =
    useState<ParentDeletionPreview>();
  const [machineDeletionPreview, setMachineDeletionPreview] =
    useState<MachineDeletionPreview>();
  const [resetLocalDataPreview, setResetLocalDataPreview] =
    useState<ResetLocalDataPreview>();
  const [isSaving, setIsSaving] = useState(false);

  const selectedProjects = projects.filter(
    (project) => project.context_id === selectedContextId,
  );
  const selectedRepositories = repositories.filter(
    (repository) => repository.project_id === selectedProjectId,
  );
  const selectedMachines = machines.filter(
    (machine) => machine.context_id === selectedContextId,
  );

  useEffect(() => {
    const nextContextId =
      contexts.find((context) => context.id === selectedContextId)?.id ??
      contexts[0]?.id;
    const nextProjectId =
      projects.find(
        (project) =>
          project.id === selectedProjectId &&
          project.context_id === nextContextId,
      )?.id ?? projects.find((project) => project.context_id === nextContextId)?.id;

    if (nextContextId !== selectedContextId) setSelectedContextId(nextContextId);
    if (nextProjectId !== selectedProjectId) setSelectedProjectId(nextProjectId);
  }, [contexts, projects, selectedContextId, selectedProjectId]);

  useEffect(() => {
    const configured = attentionDefaults.find(
      (attentionDefault) =>
        attentionDefault.context_id === selectedContextId &&
        attentionDefault.object_kind === attentionObjectKind,
    );
    setAttentionDefaultPolicy(configured?.policy ?? defaultAttentionPolicy);
  }, [attentionDefaults, attentionObjectKind, selectedContextId]);

  async function refreshAfterEdit() {
    const [nextStructure] = await Promise.all([
      refreshStructure(),
      refreshHome(),
      refreshSearch(),
      refreshRunSuggestions(),
      refreshActivity(),
    ]);
    return nextStructure;
  }

  function handleContextChange(contextId: number) {
    setSelectedContextId(contextId || undefined);
    setSelectedProjectId(
      projects.find((project) => project.context_id === contextId)?.id,
    );
  }

  async function handleCreateContext(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!contextName.trim()) return;

    setIsSaving(true);
    try {
      const context = await structureAdapter.createContext(contextName.trim());
      const nextStructure = await refreshAfterEdit();
      setSelectedContextId(context.id);
      setSelectedProjectId(
        nextStructure.projects.find((project) => project.context_id === context.id)
          ?.id,
      );
      setContextName("");
      setError(undefined);
    } catch (saveError) {
      setError(errorMessage(saveError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleCreateProject(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!selectedContextId || !projectName.trim()) {
      setError("Choose a Context before creating a Project.");
      return;
    }

    setIsSaving(true);
    try {
      const project = await structureAdapter.createProject(
        projectName.trim(),
        selectedContextId,
        projectDefaultStatus,
      );
      await refreshAfterEdit();
      setSelectedProjectId(project.id);
      setProjectName("");
      setProjectDefaultStatus("Inbox");
      setError(undefined);
    } catch (saveError) {
      setError(errorMessage(saveError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handlePrepareProjectDeletion(projectId: number) {
    setIsSaving(true);
    try {
      setParentDeletionPreview(await structureAdapter.prepareProjectDeletion(projectId));
      setError(undefined);
    } catch (previewError) {
      setError(errorMessage(previewError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handlePrepareContextDeletion(contextId: number) {
    setIsSaving(true);
    try {
      setParentDeletionPreview(await structureAdapter.prepareContextDeletion(contextId));
      setError(undefined);
    } catch (previewError) {
      setError(errorMessage(previewError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleDeleteProject(projectId: number) {
    if (
      !parentDeletionPreview ||
      parentDeletionPreview.plan.projectId !== projectId ||
      parentDeletionPreview.blockers.length > 0
    ) {
      return;
    }

    const { plan } = parentDeletionPreview;
    const deleteWorksetDirectories = confirmWorksetCleanup(
      plan.worksets,
      parentDeletionPreview.worksets,
    );

    setIsSaving(true);
    try {
      const result = await structureAdapter.deleteProject(
        projectId,
        plan.items.map((item) => item.id),
        plan.repositories.map((repository) => repository.id),
        plan.worksets.map((workset) => workset.id),
        deleteWorksetDirectories,
      );
      setParentDeletionPreview(undefined);
      await refreshAfterEdit();
      showParentDeletionResult("Project", result);
    } catch (deleteError) {
      setParentDeletionPreview(undefined);
      window.alert(errorMessage(deleteError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleDeleteContext(contextId: number) {
    if (
      !parentDeletionPreview ||
      parentDeletionPreview.plan.contextId !== contextId ||
      parentDeletionPreview.blockers.length > 0
    ) {
      return;
    }

    const { plan } = parentDeletionPreview;
    const deleteWorksetDirectories = confirmWorksetCleanup(
      plan.worksets,
      parentDeletionPreview.worksets,
    );

    setIsSaving(true);
    try {
      const result = await structureAdapter.deleteContext({
        contextId,
        projectIds: plan.projects.map((project) => project.id),
        itemIds: plan.items.map((item) => item.id),
        repositoryIds: plan.repositories.map((repository) => repository.id),
        worksetIds: plan.worksets.map((workset) => workset.id),
        machineIds: plan.machines.map((machine) => machine.id),
        deleteWorksetDirectories,
      });
      setParentDeletionPreview(undefined);
      await refreshAfterEdit();
      showParentDeletionResult("Context", result);
    } catch (deleteError) {
      setParentDeletionPreview(undefined);
      window.alert(errorMessage(deleteError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleRegisterRepository(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!selectedProjectId || !repositoryName.trim() || !repositoryRemoteUrl.trim()) {
      setError("Choose a Project and provide the Repository details.");
      return;
    }

    setIsSaving(true);
    try {
      await structureAdapter.registerRepository(
        selectedProjectId,
        repositoryName.trim(),
        repositoryRemoteUrl.trim(),
      );
      await refreshAfterEdit();
      setRepositoryName("");
      setRepositoryRemoteUrl("");
      setError(undefined);
    } catch (saveError) {
      setError(errorMessage(saveError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handlePrepareRepositoryDeletion(repositoryId: number) {
    setIsSaving(true);
    try {
      setRepositoryDeletionPreview(
        await structureAdapter.prepareRepositoryDeletion(repositoryId),
      );
      setError(undefined);
    } catch (previewError) {
      setError(errorMessage(previewError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleDeleteRepository(repositoryId: number) {
    if (
      !repositoryDeletionPreview ||
      repositoryDeletionPreview.plan.repositoryId !== repositoryId ||
      repositoryDeletionPreview.blockers.length > 0
    ) {
      return;
    }

    const { plan } = repositoryDeletionPreview;
    if (
      !window.confirm(
        `Delete Repository ${plan.name} and its local Workset records? This cannot be undone.`,
      )
    ) {
      return;
    }
    const deleteWorksetDirectories =
      plan.worksets.length === 0 ||
      window.confirm(
        `Permanently delete these ${plan.worksets.length} Workset director${plan.worksets.length === 1 ? "y" : "ies"} from disk too? Choose Cancel to keep the directories while removing their records.`,
      );

    setIsSaving(true);
    try {
      const result = await structureAdapter.deleteRepository(
        repositoryId,
        plan.worksets.map((workset) => workset.id),
        deleteWorksetDirectories,
      );
      setRepositoryDeletionPreview(undefined);
      await refreshAfterEdit();
      if (result.physicalCleanupWarning) window.alert(result.physicalCleanupWarning);
    } catch (deleteError) {
      setRepositoryDeletionPreview(undefined);
      window.alert(errorMessage(deleteError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleRegisterMachine(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!selectedContextId || !machineName.trim() || !machineSocketName.trim()) {
      setError("Choose a Context and name the Machine before registering it.");
      return;
    }

    const transport: MachineTransport =
      machineKind === "local"
        ? { kind: "local" }
        : {
            kind: "ssh",
            host: machineHost.trim(),
            user: machineUser.trim() || null,
            port: machinePort.trim() ? Number(machinePort) : null,
            identityFile: machineIdentityFile.trim() || null,
            knownHostsFile: machineKnownHostsFile.trim() || null,
            strictHostKeyChecking: machineStrictHostKeyChecking || null,
          };

    setIsSaving(true);
    try {
      await structureAdapter.registerMachine(
        selectedContextId,
        machineName.trim(),
        machineSocketName.trim(),
        transport,
      );
      await refreshAfterEdit();
      setMachineName("");
      setMachineHost("");
      setMachineUser("");
      setMachinePort("");
      setMachineIdentityFile("");
      setMachineKnownHostsFile("");
      setError(undefined);
    } catch (saveError) {
      setError(errorMessage(saveError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleCheckMachine(machineId: number) {
    setIsSaving(true);
    try {
      await structureAdapter.checkMachine(machineId);
      await refreshAfterEdit();
      setError(undefined);
    } catch (checkError) {
      setError(errorMessage(checkError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handlePrepareMachineDeletion(machineId: number) {
    setIsSaving(true);
    try {
      setMachineDeletionPreview(
        await structureAdapter.prepareMachineDeletion(machineId),
      );
      setError(undefined);
    } catch (previewError) {
      setError(errorMessage(previewError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleDeleteMachine(machineId: number) {
    if (
      !machineDeletionPreview ||
      machineDeletionPreview.plan.machineId !== machineId ||
      machineDeletionPreview.blockers.length > 0
    ) {
      return;
    }
    const { plan } = machineDeletionPreview;
    if (
      !window.confirm(
        `Delete Machine ${plan.name} and its ${plan.runs.length} Run record${plan.runs.length === 1 ? "" : "s"}? This cannot be undone.`,
      )
    ) {
      return;
    }

    setIsSaving(true);
    try {
      const result = await structureAdapter.deleteMachine(
        machineId,
        plan.runs.map((run) => run.id),
      );
      setMachineDeletionPreview(undefined);
      await refreshAfterEdit();
      window.alert(
        `Deleted Machine ${plan.name} and ${result.runCount} finished Run record${result.runCount === 1 ? "" : "s"}.`,
      );
    } catch (deleteError) {
      setMachineDeletionPreview(undefined);
      window.alert(errorMessage(deleteError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleDeleteFinishedRun(runId: number) {
    if (!window.confirm(`Delete finished Run #${runId} from Run history? This cannot be undone.`)) {
      return;
    }
    setIsSaving(true);
    try {
      await structureAdapter.deleteRun(runId);
      setMachineDeletionPreview(undefined);
      await refreshAfterEdit();
    } catch (deleteError) {
      setMachineDeletionPreview(undefined);
      window.alert(errorMessage(deleteError));
    } finally {
      setIsSaving(false);
    }
  }

  async function saveAttentionDefault(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!selectedContextId) return;

    setIsSaving(true);
    try {
      await structureAdapter.setAttentionDefault(
        selectedContextId,
        attentionObjectKind,
        attentionDefaultPolicy,
      );
      await refreshAfterEdit();
      setError(undefined);
    } catch (saveError) {
      setError(errorMessage(saveError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handlePrepareReset() {
    setIsSaving(true);
    try {
      setResetLocalDataPreview(await structureAdapter.prepareReset());
      setError(undefined);
    } catch (previewError) {
      setError(errorMessage(previewError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleReset() {
    if (!resetLocalDataPreview || resetLocalDataPreview.blockers.length > 0) return;
    const confirmation = window.prompt(
      `This permanently resets all local records. Type ${resetLocalDataPreview.confirmationPhrase} to continue. Workset directories will be confirmed separately.`,
      "",
    );
    if (confirmation === null) return;
    const deleteWorksetDirectories =
      resetLocalDataPreview.worksets.length === 0 ||
      window.confirm(
        `Permanently delete these ${resetLocalDataPreview.worksets.length} Workset director${resetLocalDataPreview.worksets.length === 1 ? "y" : "ies"} from disk too? Choose Cancel to keep the directories while removing their local records.`,
      );

    setIsSaving(true);
    try {
      const result = await structureAdapter.reset(
        confirmation,
        deleteWorksetDirectories,
      );
      setResetLocalDataPreview(undefined);
      onResetComplete?.();
      await refreshAll();
      const summary = result.summary;
      window.alert(
        `Reset local data. Removed ${summary.contextCount} Context(s), ${summary.projectCount} Project(s), ${summary.repositoryCount} Repository record(s), ${summary.itemCount} Item(s), ${summary.worksetCount} Workset(s), ${summary.machineCount} Machine(s), ${summary.runCount} Run(s), ${summary.reminderCount} reminder(s), ${summary.relationshipCount} relationship(s), ${summary.linkCount} Link(s), ${summary.externalObjectCount} External Object(s), ${summary.snapshotCount} snapshot(s), ${summary.activityCount} Activity record(s), ${summary.attentionDefaultCount} attention default(s), and ${result.auditEntryCount} prior audit entr${result.auditEntryCount === 1 ? "y" : "ies"}. A new Personal Context and Default Project are ready.`,
      );
      if (result.physicalCleanupWarning) window.alert(result.physicalCleanupWarning);
    } catch (resetError) {
      window.alert(errorMessage(resetError));
    } finally {
      setIsSaving(false);
    }
  }

  return (
    <div className="mt-6 space-y-6">
      <Card>
        <CardHeader className="border-b border-border/70">
          <div className="flex items-start justify-between gap-4">
            <div>
              <CardTitle>Contexts and Projects</CardTitle>
              <CardDescription>
                Define the boundaries that organize Items, Repositories, and Machines.
              </CardDescription>
            </div>
            <Badge variant="secondary">{contexts.length} Contexts</Badge>
          </div>
        </CardHeader>
        <CardContent className="grid gap-6 p-4 xl:grid-cols-2">
          <EntitySection title="Contexts" description="A Context owns its Projects and Machines.">
            <form className="grid gap-3 sm:grid-cols-[1fr_auto] sm:items-end" onSubmit={handleCreateContext}>
              <Field label="New Context">
                <Input value={contextName} onChange={(event) => setContextName(event.target.value)} placeholder="Work" disabled={isSaving} />
              </Field>
              <Button type="submit" disabled={isSaving || !contextName.trim()}>Add Context</Button>
            </form>
            <EntityList>
              {contexts.length === 0 ? (
                <EmptyDescription>No Contexts have been created yet.</EmptyDescription>
              ) : (
                contexts.map((context) => (
                  <EntityRow key={context.id} title={context.name} detail={`${projects.filter((project) => project.context_id === context.id).length} Projects`}>
                    <Button type="button" variant="outline" size="sm" disabled={isSaving} onClick={() => void handlePrepareContextDeletion(context.id)}>
                      Review deletion
                    </Button>
                    {parentDeletionPreview?.plan.contextId === context.id && (
                      <ParentDeletionPreviewCard preview={parentDeletionPreview} kind="Context" disabled={isSaving} onConfirm={() => void handleDeleteContext(context.id)} onCancel={() => setParentDeletionPreview(undefined)} />
                    )}
                  </EntityRow>
                ))
              )}
            </EntityList>
          </EntitySection>

          <EntitySection title="Projects" description="Projects supply defaults for new Items.">
            <form className="grid gap-3" onSubmit={handleCreateProject}>
              <ContextSelect contexts={contexts} value={selectedContextId} onChange={handleContextChange} disabled={isSaving} />
              <div className="grid gap-3 sm:grid-cols-2">
                <Field label="New Project">
                  <Input value={projectName} onChange={(event) => setProjectName(event.target.value)} placeholder="Billing" disabled={isSaving} />
                </Field>
                <Field label="New Item starts as">
                  <NativeSelect value={projectDefaultStatus} onChange={(event) => setProjectDefaultStatus(event.target.value as ItemStatus)} disabled={isSaving}>
                    {itemStatuses.map((status) => <NativeSelectOption value={status} key={status}>{status}</NativeSelectOption>)}
                  </NativeSelect>
                </Field>
              </div>
              <Button type="submit" className="w-fit" disabled={isSaving || !projectName.trim() || !selectedContextId}>Add Project</Button>
            </form>
            <EntityList>
              {selectedProjects.length === 0 ? (
                <EmptyDescription>No Projects remain in this Context.</EmptyDescription>
              ) : (
                selectedProjects.map((project) => (
                  <EntityRow key={project.id} title={project.name} detail={`${allItems.filter((item) => item.item.project_id === project.id).length} Items · starts ${project.defaults.item_status}`}>
                    <Button type="button" variant="outline" size="sm" disabled={isSaving} onClick={() => void handlePrepareProjectDeletion(project.id)}>
                      Review deletion
                    </Button>
                    {parentDeletionPreview?.plan.projectId === project.id && (
                      <ParentDeletionPreviewCard preview={parentDeletionPreview} kind="Project" disabled={isSaving} onConfirm={() => void handleDeleteProject(project.id)} onCancel={() => setParentDeletionPreview(undefined)} />
                    )}
                  </EntityRow>
                ))
              )}
            </EntityList>
          </EntitySection>
        </CardContent>
      </Card>

      <div className="grid gap-6 xl:grid-cols-2">
        <Card>
          <CardHeader className="border-b border-border/70">
            <CardTitle>Repositories</CardTitle>
            <CardDescription>Register the repositories that Projects use in Worksets.</CardDescription>
          </CardHeader>
          <CardContent className="space-y-4 p-4">
            <form className="grid gap-3" onSubmit={handleRegisterRepository}>
              <Field label="Project">
                <NativeSelect value={selectedProjectId ?? ""} onChange={(event) => setSelectedProjectId(Number(event.target.value) || undefined)} disabled={isSaving || selectedProjects.length === 0}>
                  <NativeSelectOption value="">Choose a Project</NativeSelectOption>
                  {selectedProjects.map((project) => <NativeSelectOption value={project.id} key={project.id}>{project.name}</NativeSelectOption>)}
                </NativeSelect>
              </Field>
              <div className="grid gap-3 sm:grid-cols-2">
                <Field label="Directory name">
                  <Input value={repositoryName} onChange={(event) => setRepositoryName(event.target.value)} placeholder="service-a" disabled={isSaving} />
                </Field>
                <Field label="Remote URL">
                  <Input value={repositoryRemoteUrl} onChange={(event) => setRepositoryRemoteUrl(event.target.value)} placeholder="git@github.com:acme/service-a.git" disabled={isSaving} />
                </Field>
              </div>
              <Button type="submit" className="w-fit" disabled={isSaving || !selectedProjectId || !repositoryName.trim() || !repositoryRemoteUrl.trim()}>Register Repository</Button>
            </form>
            <EntityList>
              {selectedRepositories.length === 0 ? <EmptyDescription>No Repositories are registered under this Project.</EmptyDescription> : selectedRepositories.map((repository) => (
                <EntityRow key={repository.id} title={repository.name} detail={repository.remote_url}>
                  <Button type="button" variant="outline" size="sm" disabled={isSaving} onClick={() => void handlePrepareRepositoryDeletion(repository.id)}>Review deletion</Button>
                  {repositoryDeletionPreview?.plan.repositoryId === repository.id && (
                    <RepositoryDeletionPreviewCard preview={repositoryDeletionPreview} disabled={isSaving} onConfirm={() => void handleDeleteRepository(repository.id)} onCancel={() => setRepositoryDeletionPreview(undefined)} />
                  )}
                </EntityRow>
              ))}
            </EntityList>
          </CardContent>
        </Card>

        <Card>
          <CardHeader className="border-b border-border/70">
            <CardTitle>Machines</CardTitle>
            <CardDescription>Configure local or SSH execution targets for Runs.</CardDescription>
          </CardHeader>
          <CardContent className="space-y-4 p-4">
            <form className="grid gap-3" onSubmit={handleRegisterMachine}>
              <div className="grid gap-3 sm:grid-cols-2">
                <ContextSelect contexts={contexts} value={selectedContextId} onChange={handleContextChange} disabled={isSaving} />
                <Field label="Name">
                  <Input value={machineName} onChange={(event) => setMachineName(event.target.value)} placeholder="Build Mac" disabled={isSaving} />
                </Field>
              </div>
              <div className="grid gap-3 sm:grid-cols-2">
                <Field label="Transport">
                  <NativeSelect value={machineKind} onChange={(event) => setMachineKind(event.target.value as "local" | "ssh")} disabled={isSaving}>
                    <NativeSelectOption value="ssh">SSH remote</NativeSelectOption>
                    <NativeSelectOption value="local">Local</NativeSelectOption>
                  </NativeSelect>
                </Field>
                <Field label="tmux socket">
                  <Input value={machineSocketName} onChange={(event) => setMachineSocketName(event.target.value)} placeholder="ai-mission-manager" disabled={isSaving} />
                </Field>
              </div>
              {machineKind === "ssh" && (
                <div className="grid gap-3 sm:grid-cols-2">
                  <Field label="Host"><Input value={machineHost} onChange={(event) => setMachineHost(event.target.value)} placeholder="build.example.com" disabled={isSaving} /></Field>
                  <Field label="User"><Input value={machineUser} onChange={(event) => setMachineUser(event.target.value)} placeholder="runner" disabled={isSaving} /></Field>
                  <Field label="Port"><Input type="number" min="1" value={machinePort} onChange={(event) => setMachinePort(event.target.value)} placeholder="22" disabled={isSaving} /></Field>
                  <Field label="Identity file"><Input value={machineIdentityFile} onChange={(event) => setMachineIdentityFile(event.target.value)} placeholder="~/.ssh/mission" disabled={isSaving} /></Field>
                  <Field label="Known hosts file"><Input value={machineKnownHostsFile} onChange={(event) => setMachineKnownHostsFile(event.target.value)} placeholder="~/.ssh/known_hosts" disabled={isSaving} /></Field>
                  <Field label="Host-key checking">
                    <NativeSelect value={machineStrictHostKeyChecking} onChange={(event) => setMachineStrictHostKeyChecking(event.target.value)} disabled={isSaving}>
                      <NativeSelectOption value="yes">Strict</NativeSelectOption>
                      <NativeSelectOption value="accept-new">Accept new</NativeSelectOption>
                      <NativeSelectOption value="no">Disabled</NativeSelectOption>
                    </NativeSelect>
                  </Field>
                </div>
              )}
              <Button type="submit" className="w-fit" disabled={isSaving || !selectedContextId || !machineName.trim() || !machineSocketName.trim() || (machineKind === "ssh" && !machineHost.trim())}>Register Machine</Button>
            </form>
            <EntityList>
              {selectedMachines.length === 0 ? <EmptyDescription>No Machines are registered in this Context.</EmptyDescription> : selectedMachines.map((machine) => (
                <EntityRow key={machine.id} title={`${machine.name} · ${machine.transport.kind === "ssh" ? "SSH" : "Local"}`} detail={`Last observed: ${machine.last_observed}${machine.last_observed_at ? ` · ${new Date(machine.last_observed_at * 1000).toLocaleString()}` : ""}`}>
                  <div className="flex flex-wrap gap-2">
                    <Button type="button" variant="ghost" size="sm" disabled={isSaving} onClick={() => void handleCheckMachine(machine.id)}>Check</Button>
                    <Button type="button" variant="outline" size="sm" disabled={isSaving} onClick={() => void handlePrepareMachineDeletion(machine.id)}>Review deletion</Button>
                  </div>
                  {machineDeletionPreview?.plan.machineId === machine.id && (
                    <MachineDeletionPreviewCard preview={machineDeletionPreview} disabled={isSaving} onConfirm={() => void handleDeleteMachine(machine.id)} onDeleteRun={(runId) => void handleDeleteFinishedRun(runId)} onCancel={() => setMachineDeletionPreview(undefined)} />
                  )}
                </EntityRow>
              ))}
            </EntityList>
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardHeader className="border-b border-border/70">
          <CardTitle>Attention defaults</CardTitle>
          <CardDescription>Choose which External Object changes interrupt Links in a Context.</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4 p-4">
          <form className="grid gap-4 lg:grid-cols-[1fr_1fr_2fr_auto] lg:items-end" onSubmit={saveAttentionDefault}>
            <ContextSelect contexts={contexts} value={selectedContextId} onChange={handleContextChange} disabled={isSaving} />
            <Field label="External Object type">
              <NativeSelect value={attentionObjectKind} onChange={(event) => setAttentionObjectKind(event.target.value as ExternalObjectKind)} disabled={isSaving}>
                {objectKinds.map((kind) => <NativeSelectOption value={kind} key={kind}>{externalObjectKindLabel(kind)}</NativeSelectOption>)}
              </NativeSelect>
            </Field>
            <div className="flex flex-wrap gap-4 pb-1">
              {(["title", "state", "metadata"] as const).map((kind) => (
                <label className="flex items-center gap-2 text-sm" key={kind}>
                  <Checkbox checked={attentionDefaultPolicy[kind]} onCheckedChange={(checked) => setAttentionDefaultPolicy((current) => ({ ...current, [kind]: checked === true }))} disabled={isSaving} />
                  {kind[0].toUpperCase() + kind.slice(1)} changes
                </label>
              ))}
            </div>
            <Button type="submit" disabled={isSaving || !selectedContextId}>Save defaults</Button>
          </form>
        </CardContent>
      </Card>

      <Card className="border-destructive/30 bg-destructive/5">
        <CardHeader>
          <CardTitle>Reset all local data</CardTitle>
          <CardDescription>
            Remove Mission Manager&apos;s local working model, cached External Objects, and Activity history. Provider-owned Issues and pull requests are never deleted.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <Button type="button" variant="destructive" disabled={isSaving} onClick={() => void handlePrepareReset()}>Review reset impact</Button>
          {resetLocalDataPreview && <ResetLocalDataPreviewCard preview={resetLocalDataPreview} disabled={isSaving} onConfirm={() => void handleReset()} onCancel={() => setResetLocalDataPreview(undefined)} />}
        </CardContent>
      </Card>
    </div>
  );
}

function EntitySection({ title, description, children }: { title: string; description: string; children: React.ReactNode }) {
  return (
    <section className="space-y-4" aria-labelledby={`${title.toLowerCase()}-heading`}>
      <div>
        <h3 id={`${title.toLowerCase()}-heading`} className="font-heading text-base font-medium">{title}</h3>
        <p className="mt-1 text-sm text-muted-foreground">{description}</p>
      </div>
      {children}
    </section>
  );
}

function EntityList({ children }: { children: React.ReactNode }) {
  return <div className="grid gap-2">{children}</div>;
}

function EntityRow({ title, detail, children }: { title: string; detail: string; children: React.ReactNode }) {
  return (
    <div className="flex flex-wrap items-start gap-3 rounded-lg border border-border/70 p-3">
      <div className="min-w-0 flex-1">
        <div className="font-medium">{title}</div>
        <div className="truncate text-sm text-muted-foreground">{detail}</div>
      </div>
      <div className="contents">{children}</div>
    </div>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return <label className="grid gap-1.5 text-sm font-medium"><span>{label}</span>{children}</label>;
}

function ContextSelect({ contexts, value, onChange, disabled }: { contexts: Context[]; value: number | undefined; onChange: (contextId: number) => void; disabled: boolean }) {
  return (
    <Field label="Context">
      <NativeSelect value={value ?? ""} onChange={(event) => onChange(Number(event.target.value))} disabled={disabled || contexts.length === 0}>
        <NativeSelectOption value="">Choose a Context</NativeSelectOption>
        {contexts.map((context) => <NativeSelectOption value={context.id} key={context.id}>{context.name}</NativeSelectOption>)}
      </NativeSelect>
    </Field>
  );
}

function ParentDeletionPreviewCard({ preview, kind, disabled, onConfirm, onCancel }: { preview: ParentDeletionPreview; kind: "Project" | "Context"; disabled: boolean; onConfirm: () => void; onCancel: () => void }) {
  const { plan } = preview;
  const counts = [
    [plan.projects.length, "Projects"],
    [plan.items.length, "Items"],
    [plan.repositories.length, "Repositories"],
    [plan.machines.length, "Machines"],
    [plan.worksets.length, "Worksets"],
    [plan.runs.length, "Runs"],
    [plan.linkIds.length, "Links"],
    [plan.attentionDefaults.length, "attention defaults"],
    [plan.orphanedExternalObjectIds.length, "orphaned External Objects"],
  ];
  const physicalCleanupBlockers = preview.worksets.flatMap((workset) => workset.blockers.map((blocker) => `Workset #${workset.worksetId}: ${blocker}`));

  return (
    <div className="w-full grid gap-3 rounded-lg border border-destructive/30 bg-destructive/5 p-4 text-sm" role="alert">
      <strong>{kind} deletion preview</strong>
      <p>Deleting <b>{plan.name}</b> removes the complete local dependency graph below. Provider-owned Issues and pull requests are never deleted.</p>
      <div className="grid gap-2 sm:grid-cols-3 lg:grid-cols-5">{counts.map(([count, label]) => <span key={label as string}><b>{count}</b> {label}</span>)}</div>
      {plan.worksets.length > 0 && <PreviewList label="Workset roots" items={plan.worksets.map((workset) => `#${workset.id} · ${workset.branch}${workset.archived ? " · Archived" : ""} · ${workset.rootDirectory}`)} />}
      {physicalCleanupBlockers.length > 0 && <PreviewWarnings title="Physical cleanup unavailable" items={physicalCleanupBlockers} />}
      {preview.blockers.length > 0 && <PreviewWarnings title="Deletion blocked" items={preview.blockers} />}
      <div className="flex flex-wrap gap-2">
        <Button type="button" variant="destructive" disabled={disabled || preview.blockers.length > 0} onClick={onConfirm}>Confirm and delete {kind}</Button>
        <Button type="button" variant="ghost" disabled={disabled} onClick={onCancel}>Cancel</Button>
      </div>
    </div>
  );
}

function RepositoryDeletionPreviewCard({ preview, disabled, onConfirm, onCancel }: { preview: RepositoryDeletionPreview; disabled: boolean; onConfirm: () => void; onCancel: () => void }) {
  return (
    <div className="w-full grid gap-3 rounded-lg border border-destructive/30 bg-destructive/5 p-4 text-sm" role="alert">
      <strong>Repository deletion preview</strong>
      <p>Deleting <b>{preview.plan.name}</b> removes its local record. Referencing Worksets are shown before confirmation.</p>
      {preview.worksets.length > 0 ? <PreviewList label="Affected Workset directories" items={preview.worksets.map((workset) => `${workset.branch}${workset.archived ? " · Archived" : ""} · ${workset.rootDirectory}${workset.blockers.length > 0 ? ` · ${workset.blockers.join("; ")}` : ""}`)} /> : <p>No Worksets reference this Repository.</p>}
      {preview.blockers.length > 0 && <PreviewWarnings title="Deletion blocked" items={preview.blockers} />}
      <div className="flex flex-wrap gap-2">
        <Button type="button" variant="destructive" disabled={disabled || preview.blockers.length > 0} onClick={onConfirm}>Confirm logical deletion</Button>
        <Button type="button" variant="ghost" disabled={disabled} onClick={onCancel}>Cancel</Button>
      </div>
    </div>
  );
}

function MachineDeletionPreviewCard({ preview, disabled, onConfirm, onDeleteRun, onCancel }: { preview: MachineDeletionPreview; disabled: boolean; onConfirm: () => void; onDeleteRun: (runId: number) => void; onCancel: () => void }) {
  return (
    <div className="w-full grid gap-3 rounded-lg border border-destructive/30 bg-destructive/5 p-4 text-sm" role="alert">
      <strong>Machine deletion preview</strong>
      <p>Deleting <b>{preview.plan.name}</b> removes the Machine record and every finished Run that points to it. Panes remain owned by the Terminal Runtime.</p>
      {preview.plan.runs.length > 0 ? <PreviewList label="Runs to remove" items={preview.plan.runs.map((run) => `Run #${run.id} · ${run.itemIdentifier} · ${run.itemTitle} · ${run.state} · Pane ${run.paneStatus}`)} /> : <p>No Runs reference this Machine.</p>}
      {preview.plan.runs.filter((run) => run.state === "finished").map((run) => <Button key={run.id} type="button" variant="ghost" size="sm" className="w-fit" disabled={disabled} onClick={() => onDeleteRun(run.id)}>Delete finished Run #{run.id}</Button>)}
      {preview.blockers.length > 0 && <PreviewWarnings title="Deletion blocked" items={preview.blockers} />}
      <div className="flex flex-wrap gap-2">
        <Button type="button" variant="destructive" disabled={disabled || preview.blockers.length > 0} onClick={onConfirm}>Confirm and delete Machine</Button>
        <Button type="button" variant="ghost" disabled={disabled} onClick={onCancel}>Cancel</Button>
      </div>
    </div>
  );
}

function ResetLocalDataPreviewCard({ preview, disabled, onConfirm, onCancel }: { preview: ResetLocalDataPreview; disabled: boolean; onConfirm: () => void; onCancel: () => void }) {
  const { summary } = preview.plan;
  const counts: [number, string][] = [
    [summary.contextCount, "Contexts"], [summary.projectCount, "Projects"], [summary.repositoryCount, "Repositories"], [summary.itemCount, "Items"], [summary.worksetCount, "Worksets"], [summary.machineCount, "Machines"], [summary.runCount, "Runs"], [summary.reminderCount, "reminders"], [summary.relationshipCount, "relationships"], [summary.linkCount, "Links"], [summary.externalObjectCount, "External Objects"], [summary.snapshotCount, "snapshots"], [summary.activityCount, "Activity records"], [summary.attentionDefaultCount, "attention defaults"], [preview.auditEntryCount, "prior audit entries"],
  ];
  return (
    <div className="grid gap-3 rounded-lg border border-destructive/30 bg-background p-4 text-sm" role="alert">
      <strong>Reset impact preview</strong>
      <p>This removes only Mission Manager&apos;s local working model. Workset directories are checked separately, and provider-owned data is never deleted.</p>
      <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-4">{counts.map(([count, label]) => <span key={label}><b>{count}</b> {label}</span>)}</div>
      {preview.worksets.length > 0 ? <PreviewList label="Workset roots to stage and remove" items={preview.worksets.map((workset) => `Workset #${workset.worksetId} · ${workset.branch}${workset.archived ? " · Archived" : ""} · ${workset.rootDirectory}`)} /> : <p>No Workset roots are registered. Local records will still be reset.</p>}
      {preview.blockers.length > 0 && <PreviewWarnings title="Reset blocked" items={preview.blockers} />}
      <p>Confirmation requires typing <code>{preview.confirmationPhrase}</code> exactly.</p>
      <div className="flex flex-wrap gap-2">
        <Button type="button" variant="destructive" disabled={disabled || preview.blockers.length > 0} onClick={onConfirm}>Reset all local data</Button>
        <Button type="button" variant="ghost" disabled={disabled} onClick={onCancel}>Cancel</Button>
      </div>
    </div>
  );
}

function PreviewList({ label, items }: { label: string; items: string[] }) {
  return <div className="grid gap-1"><span className="font-medium text-muted-foreground">{label}</span>{items.map((item) => <span key={item}>{item}</span>)}</div>;
}

function PreviewWarnings({ title, items }: { title: string; items: string[] }) {
  return <Alert variant="destructive"><AlertDescription><strong>{title}</strong><ul className="mt-1 list-disc pl-5">{items.map((item) => <li key={item}>{item}</li>)}</ul></AlertDescription></Alert>;
}

function showParentDeletionResult(kind: "Project" | "Context", result: ParentDeletionResult) {
  const { summary } = result;
  window.alert(`Deleted ${kind}: ${summary.projectCount} Project(s), ${summary.itemCount} Item(s), ${summary.repositoryCount} Repository record(s), ${summary.machineCount} Machine(s), ${summary.worksetCount} Workset(s), ${summary.runCount} Run(s), ${summary.linkCount} Link(s), and ${summary.externalObjectCount} orphaned External Object(s).`);
  if (result.physicalCleanupWarning) window.alert(result.physicalCleanupWarning);
}

function confirmWorksetCleanup(worksets: { id: number }[], previews: { safe: boolean }[]): boolean {
  if (worksets.length === 0) return false;
  if (!previews.every((workset) => workset.safe)) return false;
  return window.confirm(`Permanently delete these ${worksets.length} Workset director${worksets.length === 1 ? "y" : "ies"} from disk too? Choose Cancel to keep the directories while removing their records.`);
}
