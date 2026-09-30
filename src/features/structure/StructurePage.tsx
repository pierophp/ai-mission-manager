import { type FormEvent, useEffect, useMemo, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { Link, useBlocker } from "@tanstack/react-router";
import { open } from "@tauri-apps/plugin-dialog";
import { Monitor, Moon, Sun } from "lucide-react";

import { Alert, AlertDescription } from "../../components/ui/alert";
import { useAppShell } from "../../components/app-shell";
import { Button } from "../../components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "../../components/ui/card";
import { ConfirmationDialog } from "../../components/ui/confirmation-dialog";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "../../components/ui/dialog";
import { EmptyDescription } from "../../components/ui/empty";
import { Input } from "../../components/ui/input";
import { ToggleGroup, ToggleGroupItem } from "../../components/ui/toggle-group";
import {
  NativeSelect,
  NativeSelectOption,
} from "../../components/ui/native-select";
import { errorMessage } from "../../runtime/errors";
import { ContextEditor } from "./ContextEditor";
import { invalidateStructureQueries } from "../../runtime/query-invalidation";
import {
  structureKeys,
  useStructureData,
  structureQueryOptions,
} from "./structure-queries";
import {
  activityQueryOptions,
  homeQueryOptions,
  runSuggestionsQueryOptions,
  useHomeQuery,
} from "../work/work-queries";
import {
  healthStatusQueryOptions,
  setupStateQueryOptions,
} from "../setup/setup-queries";
import {
  structureActions,
  useStructureCommand,
} from "./structure-commands";
import type {
  Context,
  AgentHookReadiness,
  ItemStatus,
  ExecutionMode,
  MachineDeletionPreview,
  Machine,
  MachineTransport,
  ParentDeletionPreview,
  ParentDeletionResult,
  RepositoryDeletionPreview,
  ResetLocalDataPreview,
} from "../../runtime/types";
import type { ThemePreference } from "../../theme";
import {
  flattenHome,
  uniqueItems,
} from "../work/work-utils";

const itemStatuses: ItemStatus[] = ["Inbox", "Active", "Waiting", "Done"];

function hookReadinessLabel(agent: string, readiness?: AgentHookReadiness): string {
  if (!readiness || readiness.provisioned === null || readiness.current === null) {
    return `${agent} hooks not checked`;
  }
  if (readiness.error) return `${agent} hooks error: ${readiness.error}`;
  return readiness.provisioned && readiness.current
    ? `${agent} hooks current`
    : `${agent} hooks need provisioning`;
}

function machineReadinessDetail(machine: Machine): string {
  const parts = [
    `Last observed: ${machine.last_observed}${machine.last_observed_at ? ` · ${new Date(machine.last_observed_at * 1000).toLocaleString()}` : ""}`,
  ];
  if (machine.readiness) {
    if (machine.readiness.error) parts.push(`Machine check error: ${machine.readiness.error}`);
    parts.push(machine.readiness.bunAvailable === null
      ? "Bun not checked"
      : machine.readiness.bunAvailable
        ? "Bun available"
        : `Bun unavailable${machine.readiness.bunError ? `: ${machine.readiness.bunError}` : ""}`);
    parts.push(hookReadinessLabel("Claude Code", machine.readiness.claudeHooks));
    parts.push(hookReadinessLabel("Codex", machine.readiness.codexHooks));
    if (machine.readiness.lastProvisioningError) {
      parts.push(`Last provisioning error: ${machine.readiness.lastProvisioningError}`);
    }
  } else {
    parts.push("Agent hooks not checked");
  }
  return parts.join(" · ");
}

type StructureConfirmation = {
  title: string;
  description: string;
  confirmLabel: string;
  confirmationPhrase?: string;
  onConfirm: (confirmationPhrase: string) => void;
};

type SettingsSection =
  | "contexts"
  | "projects"
  | "repositories"
  | "machines"
  | "appearance"
  | "reset";

type AddDialog = "context" | "project" | "repository" | "machine";

type EditDialog = {
  kind: AddDialog;
  id: number;
};

const settingsSections = [
  { id: "contexts", label: "Contexts", path: "/settings/contexts" },
  { id: "projects", label: "Projects", path: "/settings/projects" },
  { id: "repositories", label: "Repositories", path: "/settings/repositories" },
  { id: "machines", label: "Machines", path: "/settings/machines" },
  { id: "appearance", label: "Appearance", path: "/settings/appearance" },
  { id: "reset", label: "Reset local data", path: "/settings/reset" },
] as const;

const themeOptions: { value: ThemePreference; label: string; Icon: typeof Sun }[] = [
  { value: "light", label: "Light", Icon: Sun },
  { value: "dark", label: "Dark", Icon: Moon },
  { value: "system", label: "System", Icon: Monitor },
];

export function StructurePage({ section }: { section: SettingsSection }) {
  const activeSection = section;
  const { closeTerminal, themePreference, setThemePreference } = useAppShell();
  const queryClient = useQueryClient();
  const structureCommand = useStructureCommand();
  const structure = useStructureData().data;
  const home = useHomeQuery(undefined).data;
  const [error, setError] = useState<string>();
  const {
    contexts,
    projects,
    repositories,
    repositoryLocations,
    machines,
    cliConfigurationProfiles,
    attentionDefaults,
    grillModelCatalog,
  } = structure;
  const allItems = useMemo(
    () => uniqueItems(home ? flattenHome(home) : []),
    [home],
  );

  const [selectedContextId, setSelectedContextId] = useState<number>();
  const [selectedProjectId, setSelectedProjectId] = useState<number>();
  const [projectName, setProjectName] = useState("");
  const [projectDefaultStatus, setProjectDefaultStatus] =
    useState<ItemStatus>("Inbox");
  const [projectExecutionMode, setProjectExecutionMode] =
    useState<ExecutionMode>("worktree");
  const [repositoryName, setRepositoryName] = useState("");
  const [repositoryRemoteUrl, setRepositoryRemoteUrl] = useState("");
  const [repositoryBaseBranch, setRepositoryBaseBranch] = useState("main");
  const [repositoryCheckoutPath, setRepositoryCheckoutPath] = useState("");
  const [repositoryWorktreeRoot, setRepositoryWorktreeRoot] = useState("~/worktrees");
  const [repositoryMachineId, setRepositoryMachineId] = useState<number>();
  const [originalRepositoryMachineId, setOriginalRepositoryMachineId] = useState<number | null>(null);
  const [repositoryPreparation, setRepositoryPreparation] = useState<"existing" | "clone">("existing");
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
  const [cliProfileProvider, setCliProfileProvider] = useState<"claude" | "codex">("claude");
  const [cliProfileName, setCliProfileName] = useState("");
  const [cliProfileAppManaged, setCliProfileAppManaged] = useState(true);
  const [cliProfileDirectory, setCliProfileDirectory] = useState("");
  const [cliProfileMachineId, setCliProfileMachineId] = useState<number>();
  const [repositoryDeletionPreview, setRepositoryDeletionPreview] =
    useState<RepositoryDeletionPreview>();
  const [parentDeletionPreview, setParentDeletionPreview] =
    useState<ParentDeletionPreview>();
  const [machineDeletionPreview, setMachineDeletionPreview] =
    useState<MachineDeletionPreview>();
  const [resetLocalDataPreview, setResetLocalDataPreview] =
    useState<ResetLocalDataPreview>();
  const [confirmation, setConfirmation] = useState<StructureConfirmation>();
  const [confirmationPhrase, setConfirmationPhrase] = useState("");
  const [isSaving, setIsSaving] = useState(false);
  const [addDialog, setAddDialog] = useState<AddDialog>();
  const [editDialog, setEditDialog] = useState<EditDialog>();
  const [editingContextId, setEditingContextId] = useState<number>();
  const [creatingContext, setCreatingContext] = useState(false);
  const [contextEditorDirty, setContextEditorDirty] = useState(false);

  const selectedProjects = projects.filter(
    (project) => project.context_id === selectedContextId,
  );
  const selectedRepositories = repositories.filter(
    (repository) => repository.project_id === selectedProjectId,
  );
  const selectedMachines = machines.filter(
    (machine) => machine.context_id === selectedContextId,
  );
  const selectedContext = contexts.find(
    (context) => context.id === selectedContextId,
  );
  const selectedExecutionMachine = machines.find(
    (machine) => machine.id === selectedContext?.execution_machine_id,
  );
  const selectedProfileMachine = selectedMachines.find((machine) => machine.id === cliProfileMachineId)
    ?? selectedExecutionMachine
    ?? selectedMachines[0];

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
    if (!selectedMachines.some((machine) => machine.id === repositoryMachineId)) {
      const configuredMachineId = contexts.find(
        (context) => context.id === nextContextId,
      )?.execution_machine_id;
      setRepositoryMachineId(
        machines.find((machine) => machine.id === configuredMachineId)?.id ??
          machines.find((machine) => machine.context_id === nextContextId)?.id,
      );
    }
  }, [contexts, machines, projects, repositoryMachineId, selectedContextId, selectedProjectId, selectedMachines]);

  useEffect(() => {
    if (!contextEditorDirty) return;
    const handleBeforeUnload = (event: BeforeUnloadEvent) => {
      event.preventDefault();
      event.returnValue = "";
    };
    window.addEventListener("beforeunload", handleBeforeUnload);
    return () => window.removeEventListener("beforeunload", handleBeforeUnload);
  }, [contextEditorDirty]);

  useBlocker({
    shouldBlockFn: () => {
      if (!contextEditorDirty) return false;
      if (!window.confirm("Discard unsaved Context changes?")) return true;
      setEditingContextId(undefined);
      setCreatingContext(false);
      setContextEditorDirty(false);
      return false;
    },
    enableBeforeUnload: false,
  });

  async function refreshAfterEdit() {
    await invalidateStructureQueries(queryClient);
    await queryClient.refetchQueries({
      queryKey: structureKeys.all,
      type: "active",
    });
  }

  function handleContextChange(contextId: number) {
    setSelectedContextId(contextId || undefined);
    setSelectedProjectId(
      projects.find((project) => project.context_id === contextId)?.id,
    );
  }

  function openEditContext(context: Context) {
    if (contextEditorDirty && !window.confirm("Discard unsaved Context changes?")) return;
    setEditingContextId(context.id);
    setCreatingContext(false);
    setSelectedContextId(context.id);
    setAddDialog(undefined);
    setContextEditorDirty(false);
  }

  function closeContextEditor() {
    if (contextEditorDirty && !window.confirm("Discard unsaved Context changes?")) return;
    setEditingContextId(undefined);
    setCreatingContext(false);
    setContextEditorDirty(false);
  }

  function openCreateContext() {
    if (contextEditorDirty && !window.confirm("Discard unsaved Context changes?")) return;
    setEditingContextId(undefined);
    setCreatingContext(true);
    setAddDialog(undefined);
    setEditDialog(undefined);
    setContextEditorDirty(false);
  }

  async function saveContextConfiguration(contextId: number, configuration: Parameters<typeof structureActions.updateContextConfiguration>[1]) {
    setIsSaving(true);
    try {
      await structureCommand.execute(structureActions.updateContextConfiguration(contextId, configuration));
      await refreshAfterEdit();
      setSelectedContextId(contextId);
      setEditingContextId(undefined);
      setCreatingContext(false);
      setContextEditorDirty(false);
      setError(undefined);
    } finally {
      setIsSaving(false);
    }
  }

  function openEditProject(project: (typeof projects)[number]) {
    setSelectedContextId(project.context_id);
    setSelectedProjectId(project.id);
    setProjectName(project.name);
    setProjectDefaultStatus(project.defaults.item_status);
    setProjectExecutionMode(project.defaults.execution_mode);
    setEditDialog({ kind: "project", id: project.id });
    setAddDialog(undefined);
  }

  function openEditRepository(repository: (typeof repositories)[number]) {
    const project = projects.find((candidate) => candidate.id === repository.project_id);
    const location = repositoryLocations.find(
      (candidate) => candidate.repository_id === repository.id,
    );
    if (project) {
      setSelectedContextId(project.context_id);
      setSelectedProjectId(project.id);
    }
    setRepositoryName(repository.name);
    setRepositoryRemoteUrl(repository.remote_url);
    setRepositoryBaseBranch(repository.base_branch);
    setRepositoryPreparation("existing");
    setRepositoryMachineId(location?.machine_id);
    setOriginalRepositoryMachineId(location?.machine_id ?? null);
    setRepositoryCheckoutPath(location?.checkout_path ?? "");
    setRepositoryWorktreeRoot(location?.worktree_root ?? "~/worktrees");
    setEditDialog({ kind: "repository", id: repository.id });
    setAddDialog(undefined);
  }

  function openEditMachine(machine: (typeof machines)[number]) {
    setSelectedContextId(machine.context_id);
    setMachineName(machine.name);
    setMachineSocketName(machine.socket_name);
    if (machine.transport.kind === "local") {
      setMachineKind("local");
      setMachineHost("");
      setMachineUser("");
      setMachinePort("");
      setMachineIdentityFile("");
      setMachineKnownHostsFile("");
      setMachineStrictHostKeyChecking("accept-new");
    } else {
      setMachineKind("ssh");
      setMachineHost(machine.transport.host);
      setMachineUser(machine.transport.user ?? "");
      setMachinePort(machine.transport.port?.toString() ?? "");
      setMachineIdentityFile(machine.transport.identityFile ?? "");
      setMachineKnownHostsFile(machine.transport.knownHostsFile ?? "");
      setMachineStrictHostKeyChecking(machine.transport.strictHostKeyChecking ?? "accept-new");
    }
    setEditDialog({ kind: "machine", id: machine.id });
    setAddDialog(undefined);
  }

  async function createContextConfiguration(configuration: Parameters<typeof structureActions.createContextConfiguration>[0]) {
    setIsSaving(true);
    try {
      const context = await structureCommand.execute(
        structureActions.createContextConfiguration(configuration),
      );
      await refreshAfterEdit();
      setSelectedContextId(context.id);
      setSelectedProjectId(undefined);
      setCreatingContext(false);
      setAddDialog(undefined);
      setEditDialog(undefined);
      setContextEditorDirty(false);
      setError(undefined);
    } finally {
      setIsSaving(false);
    }
  }

  async function handleCreateProject(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const editingProjectId = editDialog?.kind === "project" ? editDialog.id : undefined;
    if ((!selectedContextId && !editingProjectId) || !projectName.trim()) {
      setError("Choose a Context before creating a Project.");
      return;
    }

    setIsSaving(true);
    try {
      const project = editingProjectId
        ? await structureCommand.execute(
            structureActions.updateProject(
              editingProjectId,
              projectName.trim(),
              projectDefaultStatus,
              projectExecutionMode,
            ),
          )
        : await structureCommand.execute(
            structureActions.createProject(
              projectName.trim(),
              selectedContextId!,
              projectDefaultStatus,
              projectExecutionMode,
            ),
          );
      await refreshAfterEdit();
      setSelectedProjectId(project.id);
      setProjectName("");
      setProjectDefaultStatus("Inbox");
      setProjectExecutionMode("worktree");
      setAddDialog(undefined);
      setEditDialog(undefined);
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
      setParentDeletionPreview(
        await structureCommand.execute(
          structureActions.prepareProjectDeletion(projectId),
          false,
        ),
      );
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
      setParentDeletionPreview(
        await structureCommand.execute(
          structureActions.prepareContextDeletion(contextId),
          false,
        ),
      );
      setError(undefined);
    } catch (previewError) {
      setError(errorMessage(previewError));
    } finally {
      setIsSaving(false);
    }
  }

  async function executeDeleteProject(
    projectId: number,
    plan: ParentDeletionPreview["plan"],
  ) {
    setIsSaving(true);
    try {
      const result = await structureCommand.execute(
        structureActions.deleteProject(
          projectId,
          plan.items.map((item) => item.id),
          plan.repositories.map((repository) => repository.id),
          plan.workspaces.map((workspace) => workspace.id),
        ),
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

  function handleDeleteProject(projectId: number) {
    if (
      !parentDeletionPreview ||
      parentDeletionPreview.plan.projectId !== projectId ||
      parentDeletionPreview.blockers.length > 0
    ) {
      return;
    }

    const { plan } = parentDeletionPreview;
    setParentDeletionPreview(undefined);
    void executeDeleteProject(projectId, plan);
  }

  async function executeDeleteContext(
    contextId: number,
    plan: ParentDeletionPreview["plan"],
  ) {
    setIsSaving(true);
    try {
      const result = await structureCommand.execute(
        structureActions.deleteContext({
          contextId,
          projectIds: plan.projects.map((project) => project.id),
          itemIds: plan.items.map((item) => item.id),
          repositoryIds: plan.repositories.map((repository) => repository.id),
          workspaceIds: plan.workspaces.map((workspace) => workspace.id),
          machineIds: plan.machines.map((machine) => machine.id),
        }),
      );
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

  function handleDeleteContext(contextId: number) {
    if (
      !parentDeletionPreview ||
      parentDeletionPreview.plan.contextId !== contextId ||
      parentDeletionPreview.blockers.length > 0
    ) {
      return;
    }

    const { plan } = parentDeletionPreview;
    setParentDeletionPreview(undefined);
    void executeDeleteContext(contextId, plan);
  }

  async function handleRegisterRepository(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const editingRepositoryId = editDialog?.kind === "repository" ? editDialog.id : undefined;
    if (editingRepositoryId) {
      if (
        !repositoryMachineId ||
        !repositoryName.trim() ||
        !repositoryRemoteUrl.trim() ||
        !repositoryCheckoutPath.trim() ||
        !repositoryBaseBranch.trim()
      ) {
        setError("Provide the Repository name, remote URL, Machine, checkout path, and base branch.");
        return;
      }
      setIsSaving(true);
      try {
        await structureCommand.execute(
          structureActions.updateRepository(
            editingRepositoryId,
            repositoryName.trim(),
            repositoryRemoteUrl.trim(),
            repositoryBaseBranch.trim(),
          ),
        );
        await structureCommand.execute(
          structureActions.updateRepositoryLocation({
            repositoryId: editingRepositoryId,
            previousMachineId: originalRepositoryMachineId,
            machineId: repositoryMachineId!,
            checkoutPath: repositoryCheckoutPath.trim(),
            worktreeRoot: repositoryWorktreeRoot.trim() || "~/worktrees",
          }),
        );
        await refreshAfterEdit();
        setRepositoryName("");
        setRepositoryRemoteUrl("");
        setRepositoryBaseBranch("main");
        setRepositoryMachineId(undefined);
        setOriginalRepositoryMachineId(null);
        setRepositoryCheckoutPath("");
        setRepositoryWorktreeRoot("~/worktrees");
        setAddDialog(undefined);
        setEditDialog(undefined);
        setError(undefined);
      } catch (saveError) {
        setError(errorMessage(saveError));
      } finally {
        setIsSaving(false);
      }
      return;
    }
    if (
      !selectedProjectId ||
      !repositoryMachineId ||
      !repositoryName.trim() ||
      !repositoryCheckoutPath.trim() ||
      !repositoryBaseBranch.trim() ||
      (repositoryPreparation === "clone" && !repositoryRemoteUrl.trim())
    ) {
      setError("Choose a Project and Machine, then provide the checkout details.");
      return;
    }

    setIsSaving(true);
    try {
      await structureCommand.execute(
        structureActions.registerRepositoryAtLocation({
          projectId: selectedProjectId,
          name: repositoryName.trim(),
          remoteUrl: repositoryRemoteUrl.trim() || null,
          baseBranch: repositoryBaseBranch.trim(),
          machineId: repositoryMachineId,
          checkoutPath: repositoryCheckoutPath.trim(),
          worktreeRoot: repositoryWorktreeRoot.trim() || null,
          cloneIntoDestination: repositoryPreparation === "clone",
        }),
      );
      await refreshAfterEdit();
      setRepositoryName("");
      setRepositoryRemoteUrl("");
      setRepositoryBaseBranch("main");
      setRepositoryCheckoutPath("");
      setRepositoryWorktreeRoot("~/worktrees");
      setAddDialog(undefined);
      setEditDialog(undefined);
      setError(undefined);
    } catch (saveError) {
      setError(errorMessage(saveError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleRepositoryDirectoryPick() {
    try {
      const selectedPath = await open({
        directory: true,
        multiple: false,
        title: "Select repository checkout",
      });
      if (typeof selectedPath === "string") {
        setRepositoryCheckoutPath(selectedPath);
        setError(undefined);
      }
    } catch (pickerError) {
      setError(errorMessage(pickerError));
    }
  }

  async function handlePrepareRepositoryDeletion(repositoryId: number) {
    setIsSaving(true);
    try {
      setRepositoryDeletionPreview(
        await structureCommand.execute(
          structureActions.prepareRepositoryDeletion(repositoryId),
          false,
        ),
      );
      setError(undefined);
    } catch (previewError) {
      setError(errorMessage(previewError));
    } finally {
      setIsSaving(false);
    }
  }

  async function executeDeleteRepository(
    repositoryId: number,
    plan: RepositoryDeletionPreview["plan"],
  ) {
    setIsSaving(true);
    try {
      await structureCommand.execute(
        structureActions.deleteRepository(
          repositoryId,
          plan.workspaces.map((workspace) => workspace.id),
        ),
      );
      setRepositoryDeletionPreview(undefined);
      await refreshAfterEdit();
    } catch (deleteError) {
      setRepositoryDeletionPreview(undefined);
      window.alert(errorMessage(deleteError));
    } finally {
      setIsSaving(false);
    }
  }

  function handleDeleteRepository(repositoryId: number) {
    if (
      !repositoryDeletionPreview ||
      repositoryDeletionPreview.plan.repositoryId !== repositoryId ||
      repositoryDeletionPreview.blockers.length > 0
    ) {
      return;
    }

    const { plan } = repositoryDeletionPreview;
    setRepositoryDeletionPreview(undefined);
    void executeDeleteRepository(repositoryId, plan);
  }

  async function handleRegisterMachine(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const editingMachineId = editDialog?.kind === "machine" ? editDialog.id : undefined;
    if ((!selectedContextId && !editingMachineId) || !machineName.trim() || !machineSocketName.trim()) {
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
      await structureCommand.execute(
        editingMachineId
          ? structureActions.updateMachine(
              editingMachineId,
              machineName.trim(),
              machineSocketName.trim(),
              transport,
            )
          : structureActions.registerMachine(
              selectedContextId!,
              machineName.trim(),
              machineSocketName.trim(),
              transport,
            ),
      );
      await refreshAfterEdit();
      setMachineName("");
      setMachineHost("");
      setMachineUser("");
      setMachinePort("");
      setMachineIdentityFile("");
      setMachineKnownHostsFile("");
      setAddDialog(undefined);
      setEditDialog(undefined);
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
      await structureCommand.execute(structureActions.checkMachine(machineId));
      await refreshAfterEdit();
      setError(undefined);
    } catch (checkError) {
      setError(errorMessage(checkError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleCreateCliProfile(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!selectedProfileMachine || !cliProfileName.trim()) return;
    setIsSaving(true);
    try {
      await structureCommand.execute(structureActions.createCliConfigurationProfile({
        machineId: selectedProfileMachine.id,
        provider: cliProfileProvider,
        name: cliProfileName,
        appManaged: cliProfileAppManaged,
        existingDirectory: cliProfileAppManaged ? null : cliProfileDirectory,
      }));
      setCliProfileName("");
      setCliProfileDirectory("");
      await refreshAfterEdit();
      setError(undefined);
    } catch (saveError) { setError(errorMessage(saveError)); }
    finally { setIsSaving(false); }
  }

  async function handleDeleteCliProfile(profileId: number) {
    setIsSaving(true);
    try {
      await structureCommand.execute(structureActions.deleteCliConfigurationProfile(profileId));
      await refreshAfterEdit();
      setError(undefined);
    } catch (deleteError) { setError(errorMessage(deleteError)); }
    finally { setIsSaving(false); }
  }

  async function handlePrepareMachineDeletion(machineId: number) {
    setIsSaving(true);
    try {
      setMachineDeletionPreview(
        await structureCommand.execute(
          structureActions.prepareMachineDeletion(machineId),
          false,
        ),
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
      machineDeletionPreview.plan.machineId !== machineId
    ) {
      return;
    }
    const { plan } = machineDeletionPreview;
    setMachineDeletionPreview(undefined);
    setIsSaving(true);
    try {
      const result = await structureCommand.execute(
        structureActions.deleteMachine(
          machineId,
          plan.runs.map((run) => run.id),
          plan.worktreeIds,
          plan.repositoryLocationRepositoryIds,
        ),
      );
      setMachineDeletionPreview(undefined);
      await refreshAfterEdit();
      const stopped = result.stopAttemptCount - result.stopFailureCount;
      const stopMessage = result.stopFailureCount
        ? ` Could not stop ${result.stopFailureCount} Run pane${result.stopFailureCount === 1 ? "" : "s"}; an agent may still be running without a Mission Manager record.`
        : ` Stopped ${stopped} Run pane${stopped === 1 ? "" : "s"}.`;
      window.alert(
        `Deleted Machine ${plan.name}, ${result.runCount} Run record${result.runCount === 1 ? "" : "s"}, ${result.worktreeCount} Worktree record${result.worktreeCount === 1 ? "" : "s"}, and ${result.repositoryLocationCount} Repository location record${result.repositoryLocationCount === 1 ? "" : "s"}.${stopMessage} Checkout and Worktree files remain on the Machine.`,
      );
    } catch (deleteError) {
      setMachineDeletionPreview(undefined);
      window.alert(errorMessage(deleteError));
    } finally {
      setIsSaving(false);
    }
  }

  const isEditingProject = editDialog?.kind === "project";
  const isEditingRepository = editDialog?.kind === "repository";
  const isEditingMachine = editDialog?.kind === "machine";

  async function handlePrepareReset() {
    setIsSaving(true);
    try {
      setResetLocalDataPreview(
        await structureCommand.execute(structureActions.prepareReset(), false),
      );
      setError(undefined);
    } catch (previewError) {
      setError(errorMessage(previewError));
    } finally {
      setIsSaving(false);
    }
  }

  async function handleReset(
    confirmed = false,
    typedConfirmation = "",
  ) {
    if (!resetLocalDataPreview || resetLocalDataPreview.blockers.length > 0) return;
    if (!confirmed) {
      setConfirmationPhrase("");
      setConfirmation({
        title: "Reset all local data?",
        description: `This permanently resets all local records. Type ${resetLocalDataPreview.confirmationPhrase} to continue. Provider-owned Issues and pull requests are never deleted.`,
        confirmLabel: "Reset all local data",
        confirmationPhrase: resetLocalDataPreview.confirmationPhrase,
        onConfirm: (phrase) => void handleReset(true, phrase),
      });
      return;
    }
    if (typedConfirmation !== resetLocalDataPreview.confirmationPhrase) return;

    setIsSaving(true);
    try {
      const result = await structureCommand.execute(
        structureActions.reset(typedConfirmation),
      );
      setResetLocalDataPreview(undefined);
      closeTerminal();
      queryClient.clear();
      await Promise.all([
        queryClient.fetchQuery(setupStateQueryOptions()),
        queryClient.fetchQuery(healthStatusQueryOptions(null)),
        queryClient.fetchQuery(structureQueryOptions.contexts()),
        queryClient.fetchQuery(structureQueryOptions.projects()),
        queryClient.fetchQuery(structureQueryOptions.repositories()),
        queryClient.fetchQuery(structureQueryOptions.machines()),
        queryClient.fetchQuery(structureQueryOptions.attentionDefaults()),
        queryClient.fetchQuery(homeQueryOptions(undefined)),
        queryClient.fetchQuery(runSuggestionsQueryOptions()),
        queryClient.fetchQuery(activityQueryOptions()),
      ]);
      const summary = result.summary;
      window.alert(
        `Reset local data. Removed ${summary.contextCount} Context(s), ${summary.projectCount} Project(s), ${summary.repositoryCount} Repository record(s), ${summary.itemCount} Item(s), ${summary.machineCount} Machine(s), ${summary.runCount} Run(s), ${summary.reminderCount} reminder(s), ${summary.relationshipCount} relationship(s), ${summary.linkCount} Link(s), ${summary.externalObjectCount} External Object(s), ${summary.snapshotCount} snapshot(s), ${summary.activityCount} Activity record(s), ${summary.attentionDefaultCount} attention default(s), and ${result.auditEntryCount} prior audit entr${result.auditEntryCount === 1 ? "y" : "ies"}. A new Personal Context and Default Project are ready.`,
      );
    } catch (resetError) {
      window.alert(errorMessage(resetError));
    } finally {
      setIsSaving(false);
    }
  }

  return (
    <div className="mt-6 space-y-6">
      {error && (
        <Alert variant="destructive">
          <AlertDescription>{error}</AlertDescription>
        </Alert>
      )}

      <div className="grid gap-6 lg:grid-cols-[220px_minmax(0,1fr)]">
        <aside className="h-fit rounded-xl border border-border/70 bg-card p-2" aria-label="Settings sections">
          <div className="px-3 py-2">
            <p className="text-xs font-bold uppercase tracking-[0.12em] text-muted-foreground">Settings</p>
            <p className="mt-1 text-sm text-muted-foreground">Manage your work model</p>
          </div>
          <nav className="grid gap-1" aria-label="Settings sections">
            {settingsSections.map(({ id, label, path }) => (
              <Button
                asChild
                key={id}
                variant="ghost"
                className={`h-auto w-full justify-between rounded-lg px-3 py-2.5 text-left text-sm font-normal transition-colors hover:bg-secondary/60 hover:text-foreground ${
                  section === id
                    ? "bg-secondary font-semibold text-foreground"
                    : "text-muted-foreground hover:bg-secondary/60 hover:text-foreground"
                }`}
                aria-current={section === id ? "page" : undefined}
              >
                <Link to={path}>
                  <span>{label}</span>
                  {section === id && <span className="size-1.5 rounded-full bg-primary" aria-hidden="true" />}
                </Link>
              </Button>
            ))}
          </nav>
        </aside>

        <div className="min-w-0 space-y-6">
          {activeSection === "contexts" && (
            <Card>
              <CardHeader className="border-b border-border/70">
                <div className="flex items-start justify-between gap-4">
                  <div>
                    <CardTitle>Contexts</CardTitle>
                    <CardDescription>A Context owns its Projects and Machines.</CardDescription>
                  </div>
                  <Button
                    type="button"
                    onClick={openCreateContext}
                    disabled={isSaving || creatingContext}
                  >
                    Add Context
                  </Button>
                </div>
              </CardHeader>
              <CardContent className="p-4">
                <EntityList>
                  {contexts.length === 0 ? (
                    <EmptyDescription>No Contexts have been created yet.</EmptyDescription>
                  ) : contexts.map((context) => (
                    <EntityRow key={context.id} title={context.name} detail={`${projects.filter((project) => project.context_id === context.id).length} Projects`} selected={context.id === selectedContextId}>
                      <Button type="button" variant="ghost" size="sm" disabled={isSaving} onClick={() => openEditContext(context)}>Edit</Button>
                      <Button type="button" variant="outline" size="sm" disabled={isSaving || Boolean(editingContextId) || creatingContext} onClick={() => void handlePrepareContextDeletion(context.id)}>Delete</Button>
                    </EntityRow>
                  ))}
                </EntityList>
                {contexts.map((context) => context.id === editingContextId && (
                  <ContextEditor
                    key={context.id}
                    context={context}
                    attentionDefaults={attentionDefaults}
                    machines={machines}
                    profiles={cliConfigurationProfiles}
                    catalog={grillModelCatalog}
                    isSaving={isSaving}
                    onSave={(configuration) => saveContextConfiguration(context.id, configuration)}
                    onCancel={closeContextEditor}
                    onDirtyChange={setContextEditorDirty}
                  />
                ))}
                {creatingContext && (
                  <ContextEditor
                    context={undefined}
                    attentionDefaults={attentionDefaults}
                    machines={machines}
                    profiles={cliConfigurationProfiles}
                    catalog={grillModelCatalog}
                    isSaving={isSaving}
                    onSave={createContextConfiguration}
                    onCancel={closeContextEditor}
                    onDirtyChange={setContextEditorDirty}
                  />
                )}
              </CardContent>
            </Card>
          )}

          {activeSection === "projects" && (
            <Card>
              <CardHeader className="border-b border-border/70">
                <div className="flex flex-wrap items-end justify-between gap-4">
                  <div>
                    <CardTitle>Projects</CardTitle>
                    <CardDescription>Projects supply defaults for new Items.</CardDescription>
                  </div>
                  <div className="flex flex-wrap items-end gap-3">
                    <ContextSelect contexts={contexts} value={selectedContextId} onChange={handleContextChange} disabled={isSaving} />
                    <Dialog open={addDialog === "project" || isEditingProject} onOpenChange={(open) => { if (open) { setAddDialog("project"); setEditDialog(undefined); } else { setAddDialog(undefined); setEditDialog(undefined); } }}>
                      <DialogTrigger asChild>
                        <Button type="button" disabled={!selectedContextId}>Add Project</Button>
                      </DialogTrigger>
                      <DialogContent className="sm:max-w-2xl">
                        <form className="grid gap-4" onSubmit={handleCreateProject}>
                          <DialogHeader>
                            <DialogTitle>{isEditingProject ? "Edit Project" : "Add Project"}</DialogTitle>
                            <DialogDescription>{isEditingProject ? "Update this Project&apos;s name and defaults." : "Choose the Context that will own this Project and its defaults."}</DialogDescription>
                          </DialogHeader>
                          {!isEditingProject && <ContextSelect contexts={contexts} value={selectedContextId} onChange={handleContextChange} disabled={isSaving} />}
                          <div className="grid gap-3 sm:grid-cols-3">
                            <Field label="Project name"><Input value={projectName} onChange={(event) => setProjectName(event.target.value)} placeholder="Billing" disabled={isSaving} autoFocus /></Field>
                            <Field label="New Item starts as"><NativeSelect value={projectDefaultStatus} onChange={(event) => setProjectDefaultStatus(event.target.value as ItemStatus)} disabled={isSaving}>{itemStatuses.map((status) => <NativeSelectOption value={status} key={status}>{status}</NativeSelectOption>)}</NativeSelect></Field>
                            <Field label="Default execution mode"><NativeSelect value={projectExecutionMode} onChange={(event) => setProjectExecutionMode(event.target.value as ExecutionMode)} disabled={isSaving}><NativeSelectOption value="worktree">Worktree</NativeSelectOption><NativeSelectOption value="direct">Direct checkout</NativeSelectOption></NativeSelect></Field>
                          </div>
                          <DialogFooter>
                            <DialogClose asChild><Button type="button" variant="outline">Cancel</Button></DialogClose>
                            <Button type="submit" disabled={isSaving || !projectName.trim() || (!selectedContextId && !isEditingProject)}>{isEditingProject ? "Save changes" : "Add Project"}</Button>
                          </DialogFooter>
                        </form>
                      </DialogContent>
                    </Dialog>
                  </div>
                </div>
              </CardHeader>
              <CardContent className="p-4">
                <EntityList>
                  {selectedProjects.length === 0 ? <EmptyDescription>No Projects remain in this Context.</EmptyDescription> : selectedProjects.map((project) => (
                    <EntityRow key={project.id} title={project.name} detail={`${allItems.filter((item) => item.item.project_id === project.id).length} Items · ${repositories.filter((repository) => repository.project_id === project.id).length} Repositories · starts ${project.defaults.item_status} · ${project.defaults.execution_mode === "worktree" ? "Worktree" : "Direct"} default`}>
                      <Button type="button" variant="ghost" size="sm" disabled={isSaving} onClick={() => openEditProject(project)}>Edit</Button>
                      <Button type="button" variant="outline" size="sm" disabled={isSaving} onClick={() => void handlePrepareProjectDeletion(project.id)}>Delete</Button>
                    </EntityRow>
                  ))}
                </EntityList>
                <section className="mt-5 grid gap-4 border-t border-border/70 pt-4" aria-label="CLI configuration profiles">
                  <div><h3 className="m-0 text-sm font-semibold">CLI configuration profiles</h3><p className="mb-0 mt-1 text-xs text-muted-foreground">Profiles stay on their owning Machine. App-managed profiles get a dedicated directory and a copyable sign-in command.</p></div>
                  <Field label="Machine"><NativeSelect value={selectedProfileMachine?.id ?? ""} onChange={(event) => setCliProfileMachineId(Number(event.target.value) || undefined)} disabled={isSaving || selectedMachines.length === 0}><NativeSelectOption value="">Choose a Machine</NativeSelectOption>{selectedMachines.map((machine) => <NativeSelectOption value={machine.id} key={machine.id}>{machine.name}</NativeSelectOption>)}</NativeSelect></Field>
                  <EntityList>
                    {cliConfigurationProfiles.filter(({ profile }) => profile.machineId === selectedProfileMachine?.id).map(({ profile, signInCommand }) => <EntityRow key={profile.id} title={`${profile.name} · ${profile.provider === "claude" ? "Claude Code" : "Codex"}`} detail={`${profile.appManaged ? "App-managed" : "Existing directory"} · ${profile.directory}`}>
                      {signInCommand && <Button type="button" size="sm" variant="ghost" disabled={isSaving} onClick={() => void navigator.clipboard.writeText(signInCommand).catch((copyError) => setError(errorMessage(copyError)))}>Copy sign-in command</Button>}
                      <Button type="button" size="sm" variant="outline" disabled={isSaving} onClick={() => void handleDeleteCliProfile(profile.id)}>Delete</Button>
                    </EntityRow>)}
                  </EntityList>
                  {selectedProfileMachine && <form className="grid gap-3 sm:grid-cols-2" onSubmit={(event) => void handleCreateCliProfile(event)}>
                    <Field label="Provider"><NativeSelect value={cliProfileProvider} onChange={(event) => setCliProfileProvider(event.target.value as "claude" | "codex")} disabled={isSaving}><NativeSelectOption value="claude">Claude Code</NativeSelectOption><NativeSelectOption value="codex">Codex</NativeSelectOption></NativeSelect></Field>
                    <Field label="Profile name"><Input value={cliProfileName} onChange={(event) => setCliProfileName(event.target.value)} placeholder="Work" disabled={isSaving} /></Field>
                    <Field label="Directory"><NativeSelect value={cliProfileAppManaged ? "managed" : "existing"} onChange={(event) => setCliProfileAppManaged(event.target.value === "managed")} disabled={isSaving}><NativeSelectOption value="managed">Create an app-managed directory</NativeSelectOption><NativeSelectOption value="existing">Use an existing directory</NativeSelectOption></NativeSelect></Field>
                    {!cliProfileAppManaged && <Field label="Existing configuration directory"><Input value={cliProfileDirectory} onChange={(event) => setCliProfileDirectory(event.target.value)} placeholder="~/.claude-work" disabled={isSaving} /></Field>}
                    <div className="sm:col-span-2"><Button type="submit" disabled={isSaving || !cliProfileName.trim() || (!cliProfileAppManaged && !cliProfileDirectory.trim())}>Create profile</Button></div>
                  </form>}
                </section>
              </CardContent>
            </Card>
          )}

          {activeSection === "repositories" && (
            <Card>
              <CardHeader className="border-b border-border/70">
                <div className="flex flex-wrap items-end justify-between gap-4">
                  <div>
                    <CardTitle>Repositories</CardTitle>
                    <CardDescription>Repositories configured for a Project are available to every Item in that Project.</CardDescription>
                  </div>
                  <div className="flex flex-wrap items-end gap-3">
                    <ContextSelect contexts={contexts} value={selectedContextId} onChange={handleContextChange} disabled={isSaving} />
                    <Field label="Project"><NativeSelect value={selectedProjectId ?? ""} onChange={(event) => setSelectedProjectId(Number(event.target.value) || undefined)} disabled={isSaving || selectedProjects.length === 0}><NativeSelectOption value="">Choose a Project</NativeSelectOption>{selectedProjects.map((project) => <NativeSelectOption value={project.id} key={project.id}>{project.name}</NativeSelectOption>)}</NativeSelect></Field>
                    <Dialog open={addDialog === "repository" || isEditingRepository} onOpenChange={(open) => { if (open) { setAddDialog("repository"); setEditDialog(undefined); } else { setAddDialog(undefined); setEditDialog(undefined); } }}>
                      <DialogTrigger asChild><Button type="button" disabled={!selectedProjectId || !repositoryMachineId}>Add Repository</Button></DialogTrigger>
                      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-2xl">
                        <form className="grid gap-4" onSubmit={handleRegisterRepository}>
                          <DialogHeader><DialogTitle>{isEditingRepository ? "Edit Repository" : "Add Repository"}</DialogTitle><DialogDescription>{isEditingRepository ? "Update this Repository&apos;s identity and checkout location." : "Register the checkout that a Project will use on a Machine."}</DialogDescription></DialogHeader>
                          <div className="grid gap-3 sm:grid-cols-2">
                            <Field label="Project"><NativeSelect value={selectedProjectId ?? ""} onChange={(event) => setSelectedProjectId(Number(event.target.value) || undefined)} disabled={isSaving || isEditingRepository || selectedProjects.length === 0}><NativeSelectOption value="">Choose a Project</NativeSelectOption>{selectedProjects.map((project) => <NativeSelectOption value={project.id} key={project.id}>{project.name}</NativeSelectOption>)}</NativeSelect></Field>
                            <Field label="Machine"><NativeSelect value={repositoryMachineId ?? ""} onChange={(event) => setRepositoryMachineId(Number(event.target.value) || undefined)} disabled={isSaving || selectedMachines.length === 0}><NativeSelectOption value="">Choose a Machine</NativeSelectOption>{selectedMachines.map((machine) => <NativeSelectOption value={machine.id} key={machine.id}>{machine.name}</NativeSelectOption>)}</NativeSelect></Field>
                          </div>
                          <div className="grid gap-3 sm:grid-cols-2">
                            <Field label={isEditingRepository ? "Repository name" : "Directory name"}><Input value={repositoryName} onChange={(event) => setRepositoryName(event.target.value)} placeholder="service-a" disabled={isSaving} autoFocus /></Field>
                            <Field label="Preparation"><NativeSelect value={repositoryPreparation} onChange={(event) => setRepositoryPreparation(event.target.value as "existing" | "clone")} disabled={isSaving || isEditingRepository}><NativeSelectOption value="existing">Adopt existing checkout</NativeSelectOption><NativeSelectOption value="clone">Clone into destination</NativeSelectOption></NativeSelect></Field>
                          </div>
                          <div className="grid gap-3 sm:grid-cols-2">
                            <Field label="Checkout path"><div className="flex gap-2"><Input value={repositoryCheckoutPath} onChange={(event) => setRepositoryCheckoutPath(event.target.value)} placeholder="~/src/service-a or relative/path" disabled={isSaving} /><Button type="button" variant="outline" onClick={() => void handleRepositoryDirectoryPick()} disabled={isSaving}>Browse</Button></div></Field>
                            <Field label="Default base branch"><Input value={repositoryBaseBranch} onChange={(event) => setRepositoryBaseBranch(event.target.value)} placeholder="main" disabled={isSaving} /></Field>
                          </div>
                          <div className="grid gap-3 sm:grid-cols-2">
                            <Field label="Remote URL"><Input value={repositoryRemoteUrl} onChange={(event) => setRepositoryRemoteUrl(event.target.value)} placeholder={repositoryPreparation === "existing" ? "Optional; detected from checkout" : "git@github.com:acme/service-a.git"} disabled={isSaving} /></Field>
                            <Field label="Default Worktree root"><Input value={repositoryWorktreeRoot} onChange={(event) => setRepositoryWorktreeRoot(event.target.value)} placeholder="~/worktrees" disabled={isSaving} /></Field>
                          </div>
                          <DialogFooter><DialogClose asChild><Button type="button" variant="outline">Cancel</Button></DialogClose><Button type="submit" disabled={isSaving || (isEditingRepository ? !repositoryName.trim() || !repositoryRemoteUrl.trim() || !repositoryMachineId || !repositoryCheckoutPath.trim() || !repositoryBaseBranch.trim() : !selectedProjectId || !repositoryMachineId || !repositoryName.trim() || !repositoryCheckoutPath.trim() || !repositoryBaseBranch.trim() || (repositoryPreparation === "clone" && !repositoryRemoteUrl.trim()))}>{isEditingRepository ? "Save changes" : "Add Repository"}</Button></DialogFooter>
                        </form>
                      </DialogContent>
                    </Dialog>
                  </div>
                </div>
              </CardHeader>
              <CardContent className="p-4">
                <EntityList>
                  {selectedRepositories.length === 0 ? <EmptyDescription>No Repositories are registered under this Project.</EmptyDescription> : selectedRepositories.map((repository) => (
                    <EntityRow key={repository.id} title={repository.name} detail={`${repository.remote_url} · base ${repository.base_branch} · ${repositoryLocations.filter((location) => location.repository_id === repository.id).length} Machine location(s)`}>
                      <Button type="button" variant="ghost" size="sm" disabled={isSaving} onClick={() => openEditRepository(repository)}>Edit</Button>
                      <Button type="button" variant="outline" size="sm" disabled={isSaving} onClick={() => void handlePrepareRepositoryDeletion(repository.id)}>Delete</Button>
                    </EntityRow>
                  ))}
                </EntityList>
              </CardContent>
            </Card>
          )}

          {activeSection === "machines" && (
            <Card>
              <CardHeader className="border-b border-border/70">
                <div className="flex flex-wrap items-end justify-between gap-4">
                  <div><CardTitle>Machines</CardTitle><CardDescription>Configure local or SSH execution targets for Runs.</CardDescription></div>
                  <div className="flex flex-wrap items-end gap-3">
                    <ContextSelect contexts={contexts} value={selectedContextId} onChange={handleContextChange} disabled={isSaving} />
                    <Dialog open={addDialog === "machine" || isEditingMachine} onOpenChange={(open) => { if (open) { setAddDialog("machine"); setEditDialog(undefined); } else { setAddDialog(undefined); setEditDialog(undefined); } }}>
                      <DialogTrigger asChild><Button type="button" disabled={!selectedContextId}>Add Machine</Button></DialogTrigger>
                      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-2xl">
                        <form className="grid gap-4" onSubmit={handleRegisterMachine}>
                          <DialogHeader><DialogTitle>{isEditingMachine ? "Edit Machine" : "Add Machine"}</DialogTitle><DialogDescription>{isEditingMachine ? "Update this Machine&apos;s connection settings." : "Configure a local or SSH execution target for Runs."}</DialogDescription></DialogHeader>
                          <div className="grid gap-3 sm:grid-cols-2">{!isEditingMachine && <ContextSelect contexts={contexts} value={selectedContextId} onChange={handleContextChange} disabled={isSaving} />}<Field label="Name"><Input value={machineName} onChange={(event) => setMachineName(event.target.value)} placeholder="Build Mac" disabled={isSaving} autoFocus /></Field></div>
                          <div className="grid gap-3 sm:grid-cols-2"><Field label="Transport"><NativeSelect value={machineKind} onChange={(event) => setMachineKind(event.target.value as "local" | "ssh")} disabled={isSaving}><NativeSelectOption value="ssh">SSH remote</NativeSelectOption><NativeSelectOption value="local">Local</NativeSelectOption></NativeSelect></Field><Field label="tmux socket"><Input value={machineSocketName} onChange={(event) => setMachineSocketName(event.target.value)} placeholder="ai-mission-manager" disabled={isSaving} /></Field></div>
                          {machineKind === "ssh" && <div className="grid gap-3 sm:grid-cols-2"><Field label="Host"><Input value={machineHost} onChange={(event) => setMachineHost(event.target.value)} placeholder="build.example.com" disabled={isSaving} /></Field><Field label="User"><Input value={machineUser} onChange={(event) => setMachineUser(event.target.value)} placeholder="runner" disabled={isSaving} /></Field><Field label="Port"><Input type="number" min="1" value={machinePort} onChange={(event) => setMachinePort(event.target.value)} placeholder="22" disabled={isSaving} /></Field><Field label="Identity file"><Input value={machineIdentityFile} onChange={(event) => setMachineIdentityFile(event.target.value)} placeholder="~/.ssh/mission" disabled={isSaving} /></Field><Field label="Known hosts file"><Input value={machineKnownHostsFile} onChange={(event) => setMachineKnownHostsFile(event.target.value)} placeholder="~/.ssh/known_hosts" disabled={isSaving} /></Field><Field label="Host-key checking"><NativeSelect value={machineStrictHostKeyChecking} onChange={(event) => setMachineStrictHostKeyChecking(event.target.value)} disabled={isSaving}><NativeSelectOption value="yes">Strict</NativeSelectOption><NativeSelectOption value="accept-new">Accept new</NativeSelectOption><NativeSelectOption value="no">Disabled</NativeSelectOption></NativeSelect></Field></div>}
                          <DialogFooter><DialogClose asChild><Button type="button" variant="outline">Cancel</Button></DialogClose><Button type="submit" disabled={isSaving || (!selectedContextId && !isEditingMachine) || !machineName.trim() || !machineSocketName.trim() || (machineKind === "ssh" && !machineHost.trim())}>{isEditingMachine ? "Save changes" : "Add Machine"}</Button></DialogFooter>
                        </form>
                      </DialogContent>
                    </Dialog>
                  </div>
                </div>
              </CardHeader>
              <CardContent className="p-4">
                <EntityList>
                  {selectedMachines.length === 0 ? <EmptyDescription>No Machines are registered in this Context.</EmptyDescription> : selectedMachines.map((machine) => (
                    <EntityRow key={machine.id} title={`${machine.name} · ${machine.transport.kind === "ssh" ? "SSH" : "Local"}`} detail={machineReadinessDetail(machine)}>
                      <div className="flex flex-wrap gap-2"><Button type="button" variant="ghost" size="sm" disabled={isSaving} onClick={() => openEditMachine(machine)}>Edit</Button><Button type="button" variant="ghost" size="sm" disabled={isSaving} onClick={() => void handleCheckMachine(machine.id)}>Check</Button><Button type="button" variant="outline" size="sm" disabled={isSaving} onClick={() => void handlePrepareMachineDeletion(machine.id)}>Delete</Button></div>
                    </EntityRow>
                  ))}
                </EntityList>
              </CardContent>
            </Card>
          )}




          {activeSection === "appearance" && (
            <Card>
              <CardHeader className="border-b border-border/70">
                <CardTitle>Appearance</CardTitle>
                <CardDescription>Choose a color theme. System follows your operating system.</CardDescription>
              </CardHeader>
              <CardContent className="p-4">
                <ToggleGroup
                  type="single"
                  variant="outline"
                  spacing={0}
                  aria-label="Theme"
                  value={themePreference}
                  onValueChange={(value) => {
                    if (value) setThemePreference(value as ThemePreference);
                  }}
                >
                  {themeOptions.map(({ value, label, Icon }) => (
                    <ToggleGroupItem
                      key={value}
                      value={value}
                      className="gap-1.5 px-3 data-[state=on]:bg-primary data-[state=on]:text-primary-foreground data-[state=on]:hover:bg-primary/90"
                    >
                      <Icon aria-hidden="true" />
                      {label}
                    </ToggleGroupItem>
                  ))}
                </ToggleGroup>
              </CardContent>
            </Card>
          )}

          {activeSection === "reset" && <Card className="border-destructive/30 bg-destructive/5"><CardHeader><CardTitle>Reset all local data</CardTitle><CardDescription>Remove Mission Manager&apos;s local working model, cached External Objects, and Activity history. Provider-owned Issues and pull requests are never deleted.</CardDescription></CardHeader><CardContent className="space-y-4"><Button type="button" variant="destructive" disabled={isSaving} onClick={() => void handlePrepareReset()}>Review reset impact</Button>{resetLocalDataPreview && <ResetLocalDataPreviewCard preview={resetLocalDataPreview} disabled={isSaving} onConfirm={() => void handleReset()} onCancel={() => setResetLocalDataPreview(undefined)} />}</CardContent></Card>}
        </div>
      </div>
      <Dialog
        open={Boolean(
          parentDeletionPreview ||
            repositoryDeletionPreview ||
            machineDeletionPreview,
        )}
        onOpenChange={(open) => {
          if (!open && !isSaving) {
            setParentDeletionPreview(undefined);
            setRepositoryDeletionPreview(undefined);
            setMachineDeletionPreview(undefined);
          }
        }}
      >
        <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-2xl">
          <DialogHeader>
            <DialogTitle>Review deletion</DialogTitle>
            <DialogDescription>
              Review the local records that will be removed before confirming.
            </DialogDescription>
          </DialogHeader>
          {parentDeletionPreview && (
            <ParentDeletionPreviewCard
              preview={parentDeletionPreview}
              kind={parentDeletionPreview.plan.contextId !== null ? "Context" : "Project"}
              disabled={isSaving}
              onConfirm={() =>
                void (parentDeletionPreview.plan.contextId !== null
                  ? handleDeleteContext(parentDeletionPreview.plan.contextId)
                  : handleDeleteProject(parentDeletionPreview.plan.projectId!))
              }
              onCancel={() => setParentDeletionPreview(undefined)}
            />
          )}
          {repositoryDeletionPreview && (
            <RepositoryDeletionPreviewCard
              preview={repositoryDeletionPreview}
              disabled={isSaving}
              onConfirm={() => void handleDeleteRepository(repositoryDeletionPreview.plan.repositoryId)}
              onCancel={() => setRepositoryDeletionPreview(undefined)}
            />
          )}
          {machineDeletionPreview && (
            <MachineDeletionPreviewCard
              preview={machineDeletionPreview}
              disabled={isSaving}
              onConfirm={() => void handleDeleteMachine(machineDeletionPreview.plan.machineId)}
              onCancel={() => setMachineDeletionPreview(undefined)}
            />
          )}
        </DialogContent>
      </Dialog>
      {confirmation && (
        <ConfirmationDialog
          open
          title={confirmation.title}
          description={confirmation.description}
          confirmLabel={confirmation.confirmLabel}
          confirmDisabled={
            confirmation.confirmationPhrase !== undefined &&
            confirmationPhrase !== confirmation.confirmationPhrase
          }
          disabled={isSaving}
          onOpenChange={(open) => {
            if (!open && !isSaving) {
              setConfirmation(undefined);
              setConfirmationPhrase("");
            }
          }}
          onConfirm={() => {
            const currentConfirmation = confirmation;
            setConfirmation(undefined);
            currentConfirmation.onConfirm(confirmationPhrase);
          }}
        >
          {confirmation.confirmationPhrase && (
            <label className="grid gap-1.5 text-sm font-medium">
              <span>
                Type <code>{confirmation.confirmationPhrase}</code> to continue
              </span>
              <Input
                value={confirmationPhrase}
                onChange={(event) => setConfirmationPhrase(event.target.value)}
                autoFocus
                disabled={isSaving}
              />
            </label>
          )}
        </ConfirmationDialog>
      )}
    </div>
  );
}

function EntityList({ children }: { children: React.ReactNode }) {
  return <div className="grid gap-2">{children}</div>;
}

function EntityRow({ title, detail, children, selected = false }: { title: string; detail: string; children: React.ReactNode; selected?: boolean }) {
  return (
    <div
      data-selected={selected || undefined}
      className={`flex flex-wrap items-start gap-3 rounded-lg border p-3 ${
        selected
          ? "border-primary bg-primary/5"
          : "border-border/70"
      }`}
    >
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
    [plan.runs.length, "Runs"],
    [plan.linkIds.length, "Links"],
    [plan.attentionDefaults.length, "attention defaults"],
    [plan.orphanedExternalObjectIds.length, "orphaned External Objects"],
  ];
  return (
    <div className="w-full grid gap-3 rounded-lg border border-destructive/30 bg-destructive/5 p-4 text-sm" role="alert">
      <strong>{kind} deletion preview</strong>
      <p>Deleting <b>{plan.name}</b> removes the complete local dependency graph below. Provider-owned Issues and pull requests are never deleted.</p>
      <div className="grid gap-2 sm:grid-cols-3 lg:grid-cols-5">{counts.map(([count, label]) => <span key={label as string}><b>{count}</b> {label}</span>)}</div>
      {preview.blockers.length > 0 && <PreviewWarnings title="Deletion blocked" items={preview.blockers} />}
      <div className="flex flex-wrap gap-2">
        <Button type="button" variant="destructive" disabled={disabled || preview.blockers.length > 0} onClick={onConfirm}>Delete {kind}</Button>
        <Button type="button" variant="ghost" disabled={disabled} onClick={onCancel}>Cancel</Button>
      </div>
    </div>
  );
}

function RepositoryDeletionPreviewCard({ preview, disabled, onConfirm, onCancel }: { preview: RepositoryDeletionPreview; disabled: boolean; onConfirm: () => void; onCancel: () => void }) {
  return (
    <div className="w-full grid gap-3 rounded-lg border border-destructive/30 bg-destructive/5 p-4 text-sm" role="alert">
      <strong>Repository deletion preview</strong>
      <p>Deleting <b>{preview.plan.name}</b> removes its local record and makes it unavailable to Items in this Project.</p>
      {preview.plan.workspaces.length > 0 ? <PreviewList label="Items using this Repository" items={[...new Set(preview.plan.workspaces.map((workspace) => `Item #${workspace.itemId}`))]} /> : <p>No Items currently use this Repository.</p>}
      {preview.blockers.length > 0 && <PreviewWarnings title="Deletion blocked" items={preview.blockers} />}
      <div className="flex flex-wrap gap-2">
        <Button type="button" variant="destructive" disabled={disabled || preview.blockers.length > 0} onClick={onConfirm}>Delete Repository</Button>
        <Button type="button" variant="ghost" disabled={disabled} onClick={onCancel}>Cancel</Button>
      </div>
    </div>
  );
}

function MachineDeletionPreviewCard({ preview, disabled, onConfirm, onCancel }: { preview: MachineDeletionPreview; disabled: boolean; onConfirm: () => void; onCancel: () => void }) {
  return (
    <div className="w-full grid gap-3 rounded-lg border border-destructive/30 bg-destructive/5 p-4 text-sm" role="alert">
      <strong>Machine deletion preview</strong>
      <p>Deleting <b>{preview.plan.name}</b> removes this Machine&apos;s app records, Run history, Worktree records, and Repository location records. Mission Manager will try to stop each Run pane first.</p>
      <p>The checkout and Worktree files stay on the Machine. If a pane cannot be reached, its agent may keep running without a Mission Manager record.</p>
      <p>If this Machine is configured for a Context, that Context becomes unconfigured and must select a Machine before running work again.</p>
      {preview.plan.runs.length > 0 ? <PreviewList label="Run records to remove" items={preview.plan.runs.map((run) => `Run #${run.id} · ${run.itemIdentifier} · ${run.itemTitle} · ${run.state} · Pane ${run.paneStatus}`)} /> : <p>No Runs reference this Machine.</p>}
      <div className="grid gap-1 text-muted-foreground">
        <span>{preview.plan.activeRunIds.length} active Run record(s) are included in deletion.</span>
        <span>{preview.plan.worktreeIds.length} Worktree record(s) will be removed; files stay on disk.</span>
        <span>{preview.plan.repositoryLocationRepositoryIds.length} Repository location record(s) will be removed; checkout files stay on disk.</span>
      </div>
      {preview.blockers.length > 0 && <PreviewWarnings title="Deletion blocked" items={preview.blockers} />}
      <div className="flex flex-wrap gap-2">
        <Button type="button" variant="destructive" disabled={disabled || preview.blockers.length > 0} onClick={onConfirm}>Delete Machine and its records</Button>
        <Button type="button" variant="ghost" disabled={disabled} onClick={onCancel}>Cancel</Button>
      </div>
    </div>
  );
}

function ResetLocalDataPreviewCard({ preview, disabled, onConfirm, onCancel }: { preview: ResetLocalDataPreview; disabled: boolean; onConfirm: () => void; onCancel: () => void }) {
  const { summary } = preview.plan;
  const counts: [number, string][] = [
    [summary.contextCount, "Contexts"], [summary.projectCount, "Projects"], [summary.repositoryCount, "Repositories"], [summary.itemCount, "Items"], [summary.machineCount, "Machines"], [summary.runCount, "Runs"], [summary.reminderCount, "reminders"], [summary.relationshipCount, "relationships"], [summary.linkCount, "Links"], [summary.externalObjectCount, "External Objects"], [summary.snapshotCount, "snapshots"], [summary.activityCount, "Activity records"], [summary.attentionDefaultCount, "attention defaults"], [preview.auditEntryCount, "prior audit entries"],
  ];
  return (
    <div className="grid gap-3 rounded-lg border border-destructive/30 bg-background p-4 text-sm" role="alert">
      <strong>Reset impact preview</strong>
      <p>This removes only Mission Manager&apos;s local working model. Git Worktrees are managed separately, and provider-owned data is never deleted.</p>
      <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-4">{counts.map(([count, label]) => <span key={label}><b>{count}</b> {label}</span>)}</div>
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
  window.alert(`Deleted ${kind}: ${summary.projectCount} Project(s), ${summary.itemCount} Item(s), ${summary.repositoryCount} Repository record(s), ${summary.machineCount} Machine(s), ${summary.runCount} Run(s), ${summary.linkCount} Link(s), and ${summary.externalObjectCount} orphaned External Object(s).`);
}
