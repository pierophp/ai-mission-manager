import type * as Generated from "./bindings";
import type {
  AttentionEntry,
  ContextConfiguration,
  HomeView,
  ItemView,
  Machine,
  MachineTransport,
  RunLaunchRequest,
} from "./types";
import type { Run as UiRun } from "./execution-types";

function required<T>(value: T | undefined, field: string): T {
  if (value === undefined) throw new Error(`Rust response omitted required ${field}`);
  return value;
}

export function normalizeContextConfiguration(
  configuration: Generated.ContextConfiguration,
): ContextConfiguration {
  return {
    ...configuration,
    defaultWorkflow: required(configuration.defaultWorkflow, "defaultWorkflow"),
    implementDefaults: required(configuration.implementDefaults, "implementDefaults"),
    pstackDefaults: required(configuration.pstackDefaults, "pstackDefaults"),
    pstackRoles: required(configuration.pstackRoles, "pstackRoles"),
  };
}

function normalizeItemView(view: Generated.ItemView): ItemView {
  return {
    ...view,
    runs: view.runs.map(normalizeRun),
    implementation_queues: (view.implementation_queues ?? []).map((queue) => ({
      ...queue,
      pausedReason: queue.pausedReason ?? null,
      entries: queue.entries.map((entry) => ({
        ...entry,
        done: required(entry.done, "ImplementationQueueEntry.done"),
      })),
    })),
    links: view.links.map((linkView) => ({
      ...linkView,
      link: {
        ...linkView.link,
        purpose: required(linkView.link.purpose, "ExternalLink.purpose"),
        spec_external_object_id: linkView.link.spec_external_object_id ?? null,
        provenance: linkView.link.provenance ?? null,
      },
      attention_entry: linkView.attention_entry
        ? normalizeAttentionEntry(linkView.attention_entry)
        : null,
    })),
    worktrees: view.worktrees.map((worktree) => ({
      id: worktree.id,
      workspace_id: worktree.workspaceId,
      repository_id: worktree.repositoryId,
      machine_id: worktree.machineId,
      path: worktree.path,
      branch: worktree.branch,
      base_branch: worktree.baseBranch,
      is_dirty: worktree.isDirty,
    })),
    workspaces: view.workspaces.map((workspace) => ({
      ...workspace,
      repositories: workspace.repositories.map((repository) => ({
        repository_id: repository.repositoryId,
        branch: repository.branch,
        base_branch: repository.baseBranch,
      })),
    })),
  };
}

function normalizeAttentionEntry(entry: Generated.AttentionEntry): AttentionEntry {
  return {
    ...entry,
    kind: entry.kind === "ImplementationQueue" ? "implementation_queue" : entry.kind,
    queue_id: entry.queue_id ?? null,
  };
}

function normalizeRun(run: Generated.Run): UiRun {
  return {
    ...run,
    cli_configuration_profile: run.cli_configuration_profile ?? null,
    workflow: required(run.workflow, "Run.workflow"),
    model: run.model ?? null,
    effort: run.effort ?? null,
    transcript: run.transcript ?? "",
    reported_pull_requests: run.reported_pull_requests ?? [],
    attention_summary: run.attention_summary ?? null,
    grill_question_group: run.grill_question_group
      ? {
          ...run.grill_question_group,
          round: required(run.grill_question_group.round, "GrillQuestionGroup.round"),
        }
      : null,
    grill_answers: run.grill_answers ?? [],
    grill_decisions: run.grill_decisions ?? [],
    grill_response: run.grill_response ?? null,
    grill_phase: run.grill_phase ?? null,
    grill_action: run.grill_action ?? null,
    plan_phase: run.plan_phase ?? null,
    plan_path: run.plan_path ?? null,
  };
}

export function normalizeHomeView(view: Generated.HomeView): HomeView {
  return {
    ...view,
    needs_attention: view.needs_attention.map(normalizeItemView),
    running: view.running.map(normalizeItemView),
    waiting: view.waiting.map(normalizeItemView),
    due: view.due.map(normalizeItemView),
    completed: view.completed.map(normalizeItemView),
    attention_entries: view.attention_entries.map(normalizeAttentionEntry),
  };
}

export function normalizeItemViews(views: Generated.ItemView[]): ItemView[] {
  return views.map(normalizeItemView);
}

export function toGeneratedMachineTransport(
  transport: MachineTransport,
): Generated.MachineTransport {
  if (transport.kind === "local") return transport;
  return {
    kind: "ssh",
    host: transport.host,
    user: transport.user,
    port: transport.port,
    identity_file: transport.identityFile,
    known_hosts_file: transport.knownHostsFile,
    strict_host_key_checking: transport.strictHostKeyChecking,
  };
}

function normalizeMachineTransport(
  transport: Generated.MachineTransport,
): MachineTransport {
  if (transport.kind === "local") return transport;
  return {
    kind: "ssh",
    host: transport.host,
    user: transport.user,
    port: transport.port,
    identityFile: transport.identity_file,
    knownHostsFile: transport.known_hosts_file,
    strictHostKeyChecking: transport.strict_host_key_checking,
  };
}

export function normalizeMachine(machine: Generated.Machine): Machine {
  return {
    ...machine,
    transport: normalizeMachineTransport(machine.transport),
  };
}

export function normalizeMachines(machines: Generated.MachineSettingsView[]): Machine[] {
  return machines.map(normalizeMachine);
}

export function normalizeRunLaunchRequest(
  request: RunLaunchRequest,
): Generated.RunLaunchRequest {
  const strategy = request.strategy;
  switch (strategy.kind) {
    case "direct":
      return {
        itemId: request.itemId,
        workspaceId: request.workspaceId,
        queueAttachment: request.queueAttachment,
        strategy: {
          kind: "direct",
          machine_id: strategy.machineId,
          primary_repository_id: strategy.primaryRepositoryId,
          agent: strategy.agent,
          configuration: strategy.configuration ?? null,
          implementation_queue: strategy.implementationQueue ?? null,
          execution_profile: strategy.executionProfile,
          workflow: strategy.workflow,
          prompt: strategy.prompt,
          prompt_selection: strategy.promptSelection,
          expected_checkouts: strategy.expectedCheckouts,
          allow_dirty: strategy.allowDirty,
          allow_shared_checkouts: strategy.allowSharedCheckouts,
        },
      };
    case "grill":
      return {
        itemId: request.itemId,
        workspaceId: request.workspaceId,
        queueAttachment: request.queueAttachment,
        strategy: {
          kind: "grill",
          machine_id: strategy.machineId,
          primary_repository_id: strategy.primaryRepositoryId,
          configuration: strategy.configuration,
          language: strategy.language,
          prompt: strategy.prompt,
          expected_checkouts: strategy.expectedCheckouts,
          allow_dirty: strategy.allowDirty,
          allow_shared_checkouts: strategy.allowSharedCheckouts,
        },
      };
    case "worktree":
      return {
        itemId: request.itemId,
        workspaceId: request.workspaceId,
        queueAttachment: request.queueAttachment,
        strategy: {
          kind: "worktree",
          worktree_id: strategy.worktreeId,
          agent: strategy.agent,
          configuration: strategy.configuration ?? null,
          execution_profile: strategy.executionProfile,
          workflow: strategy.workflow,
          prompt: strategy.prompt,
          prompt_selection: strategy.promptSelection,
        },
      };
  }
}
