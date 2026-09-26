import { useMemo, useState } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { RefreshCwIcon } from "lucide-react";
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
import { errorMessage } from "../../../runtime/errors";
import type { Context, DirectRunPreview, ExternalLinkView, GrillAgentCatalog, GrillConfiguration, ImplementationQueueStart, ItemView, Repository } from "../../../runtime/types";
import type { ImplementationQueuePauseReason } from "../../../runtime/types";
import { useIssueDocumentQuery } from "../work-queries";
import type { ItemCommands } from "../use-item-commands";
import { workActions } from "../work-mutations";
import { itemExecution } from "./shared";
import { SectionLabel } from "./shared";

/**
 * A spec read fresh from GitHub: the tickets that break it down (its native
 * sub-issues) first, then its description.
 */
export function SpecTab({
  view,
  specs,
  repositories,
  contexts,
  modelCatalog,
  commands,
  onOpenRun,
}: {
  view: ItemView;
  specs: ExternalLinkView[];
  repositories: Repository[];
  contexts: Context[];
  modelCatalog: GrillAgentCatalog[];
  commands: ItemCommands;
  onOpenRun: (runId: number) => void;
}) {
  const [selectedSpecId, setSelectedSpecId] = useState(specs[0]?.object.id);
  const spec =
    specs.find((candidate) => candidate.object.id === selectedSpecId) ?? specs[0];
  if (!spec) return null;

  return (
    <div className="grid gap-5">
      {specs.length > 1 && (
        <label className="grid gap-1.5 text-sm font-medium">
          <span>Spec</span>
          <NativeSelect
            value={spec.object.id}
            onChange={(event) => setSelectedSpecId(Number(event.target.value))}
          >
            {specs.map((candidate) => (
              <NativeSelectOption value={candidate.object.id} key={candidate.object.id}>
                {candidate.snapshot?.title ?? candidate.object.canonical_url}
                {candidate.link.provenance
                  ? ` · Run #${candidate.link.provenance.run_id}`
                  : ""}
              </NativeSelectOption>
            ))}
          </NativeSelect>
        </label>
      )}
      <SpecDocument key={spec.object.id} view={view} spec={spec} repositories={repositories} contexts={contexts} modelCatalog={modelCatalog} commands={commands} onOpenRun={onOpenRun} />
    </div>
  );
}

function queuePauseLabel(reason: ImplementationQueuePauseReason): string {
  switch (reason.kind) {
    case "ticket_still_open": return "ticket still open";
    case "checkout_dirty": return "checkout is dirty";
    case "run_stopped": return "Run stopped";
    case "pane_missing": return "Run Pane missing";
    case "launch_failed": return `launch failed: ${reason.message}`;
  }
}

function SpecDocument({
  view,
  spec,
  repositories,
  contexts,
  modelCatalog,
  commands,
  onOpenRun,
}: {
  view: ItemView;
  spec: ExternalLinkView;
  repositories: Repository[];
  contexts: Context[];
  modelCatalog: GrillAgentCatalog[];
  commands: ItemCommands;
  onOpenRun: (runId: number) => void;
}) {
  const document = useIssueDocumentQuery(spec.object.id);
  const linkedUrls = new Set(view.links.map((link) => link.object.canonical_url));
  const number = spec.snapshot?.metadata.find((entry) => entry.key === "number")?.value;
  const [selected, setSelected] = useState<number[]>([]);
  const [launchOpen, setLaunchOpen] = useState(false);
  const [configuration, setConfiguration] = useState<GrillConfiguration>({ agent: "claude", model: "claude-sonnet-4-5", effort: "high" });
  const [preview, setPreview] = useState<DirectRunPreview>();
  const [workspaceId, setWorkspaceId] = useState(view.workspaces[0]?.id);
  const [repositoryId, setRepositoryId] = useState<number>();
  const [dirtyConsent, setDirtyConsent] = useState(false);
  const [sharedConsent, setSharedConsent] = useState(false);
  const { itemContext } = itemExecution(view, repositories, contexts, []);
  const specQueues = view.implementation_queues.filter((queue) => queue.specExternalObjectId === spec.object.id);
  const activeQueue = view.implementation_queues.find((queue) => queue.active);
  const specActiveQueue = specQueues.find((queue) => queue.active);
  const progressQueue = specActiveQueue ?? specQueues.at(-1);
  const openTickets = document.data?.subIssues.filter((ticket) => ticket.state.toLowerCase() === "open") ?? [];
  const allSelected = openTickets.length > 0 && openTickets.every((ticket) => selected.includes(ticket.number));
  const selectedTickets = useMemo(() => document.data?.subIssues.filter((ticket) => ticket.state.toLowerCase() === "open" && selected.includes(ticket.number)) ?? [], [document.data?.subIssues, selected]);
  const agentCatalog = modelCatalog.find((catalog) => catalog.agent === configuration.agent);
  const modelCatalogItem = agentCatalog?.models.find((model) => model.id === configuration.model);

  async function refreshPreview(nextWorkspaceId: number) {
    setWorkspaceId(nextWorkspaceId);
    setPreview(undefined);
    setRepositoryId(undefined);
    try {
      const checkoutPreview = await commands.workCommand.execute(workActions.prepareDirectRun(view.item.id, nextWorkspaceId, null), false);
      setPreview(checkoutPreview);
      setRepositoryId(checkoutPreview.checkoutDetails[0]?.repositoryId);
    } catch (error) {
      window.alert(error instanceof Error ? error.message : String(error));
    }
  }

  async function openLaunch() {
    const defaults = itemContext?.implement_defaults;
    if (defaults) setConfiguration(defaults);
    setLaunchOpen(true);
    setPreview(undefined);
    setDirtyConsent(false);
    setSharedConsent(false);
    const workspace = view.workspaces.find((candidate) => candidate.id === workspaceId) ?? view.workspaces[0];
    if (workspace) await refreshPreview(workspace.id);
  }

  async function startQueue() {
    if (!document.data || !preview || !workspaceId || !repositoryId || selectedTickets.length === 0) return;
    const start: ImplementationQueueStart = {
      specExternalObjectId: spec.object.id,
      specUrl: spec.object.canonical_url,
      entries: selectedTickets.map((ticket, position) => ({ position, ticketNumber: ticket.number, ticketTitle: ticket.title, ticketUrl: ticket.url, ticketState: ticket.state, runId: null, done: false, skipped: false })),
    };
    const result = await commands.saveItem(workActions.startDirectRun({
      itemId: view.item.id,
      workspaceId,
      primaryRepositoryId: repositoryId,
      machineId: null,
      agent: configuration.agent,
      configuration,
      implementationQueue: start,
      executionProfile: "implement",
      prompt: "Implementation Queue",
      promptSelection: { includeObjective: true, includeNotes: false, externalObjectIds: [] },
      expectedCheckouts: preview.checkouts,
      allowDirty: preview.dirtyRepositoryIds.length > 0 && dirtyConsent,
      allowSharedCheckouts: preview.sharedPaths.length > 0 && sharedConsent,
    }));
    if (result) setLaunchOpen(false);
  }

  return (
    <article className="grid gap-5">
      <header className="flex flex-wrap items-start justify-between gap-3">
        <div className="grid gap-1">
          <a
            href={spec.object.canonical_url}
            target="_blank"
            rel="noreferrer"
            className="font-heading text-base font-medium underline-offset-4 hover:underline"
          >
            {number ? `#${number} · ` : ""}
            {spec.snapshot?.title ?? spec.object.canonical_url}
          </a>
          <span className="text-xs text-muted-foreground">
            {spec.snapshot?.state ?? "Not fetched"}
            {spec.link.provenance && ` · Created by Run #${spec.link.provenance.run_id}`}
          </span>
        </div>
        <Button
          type="button"
          size="sm"
          variant="outline"
          disabled={document.isFetching}
          onClick={() => void document.refetch()}
        >
          <RefreshCwIcon aria-hidden="true" />
          {document.isFetching ? "Refreshing…" : "Refresh"}
        </Button>
      </header>

      {document.isPending ? (
        <p className="flex items-center gap-2 text-sm text-muted-foreground">
          <Spinner /> Reading the spec from GitHub…
        </p>
      ) : document.isError ? (
        <Alert variant="destructive">
          <AlertTitle>Could not read the spec</AlertTitle>
          <AlertDescription>{errorMessage(document.error)}</AlertDescription>
        </Alert>
      ) : (
        <>
          <section className="grid gap-2" aria-label="Tickets">
            <SectionLabel>Tickets · {document.data.subIssues.length}</SectionLabel>
            <div className="flex items-center justify-between gap-3">
              <label className="flex items-center gap-2 text-sm">
                <Checkbox checked={allSelected} disabled={openTickets.length === 0 || Boolean(activeQueue)} onCheckedChange={(checked) => setSelected(checked ? openTickets.map((ticket) => ticket.number) : [])} />
                Select all open tickets
              </label>
              <Button type="button" size="sm" disabled={!selectedTickets.length || Boolean(activeQueue) || !view.workspaces.length} onClick={() => void openLaunch()}>
                Implement ({selectedTickets.length})
              </Button>
            </div>
            {progressQueue && (
              <div className="flex flex-wrap items-center justify-between gap-3 rounded-md border p-3">
                <div className="grid gap-1">
                  <p className="text-sm font-medium">
                    {progressQueue.pausedReason
                      ? `Paused · ${queuePauseLabel(progressQueue.pausedReason)}`
                      : specActiveQueue
                        ? `Implementing ${Math.max(0, progressQueue.entries.findIndex((entry) => !entry.done && !entry.skipped) + 1)}/${progressQueue.entries.length}`
                        : progressQueue.entries.every((entry) => entry.done || entry.skipped)
                          ? progressQueue.entries.some((entry) => entry.skipped) ? "Implementation Queue finished with skipped tickets" : "Implementation Queue finished"
                          : "Implementation Queue cancelled"}
                  </p>
                </div>
                {specActiveQueue && (
                  <div className="flex flex-wrap gap-2">
                    {progressQueue.pausedReason && <>
                      <Button type="button" size="sm" variant="outline" onClick={() => void commands.saveItem(workActions.checkImplementationQueue(progressQueue.id))}>Check again</Button>
                      <Button type="button" size="sm" variant="outline" onClick={() => void commands.saveItem(workActions.skipImplementationQueueEntry(progressQueue.id))}>Skip ticket</Button>
                    </>}
                    <Button type="button" size="sm" variant="outline" onClick={() => void commands.saveItem(workActions.cancelImplementationQueue(progressQueue.id))}>Cancel queue</Button>
                  </div>
                )}
              </div>
            )}
            {launchOpen && (
              <section className="grid gap-3 rounded-md border p-4" aria-label="Implementation Queue launch confirmation">
                <h3 className="font-heading font-medium">Confirm Implementation Queue</h3>
                <ol className="grid gap-1 text-sm">{selectedTickets.map((ticket) => <li key={ticket.url}>#{ticket.number} · {ticket.title}</li>)}</ol>
                {view.workspaces.length > 1 && <label className="grid gap-1 text-sm">Workspace<NativeSelect value={workspaceId} onChange={(event) => void refreshPreview(Number(event.target.value))}>{view.workspaces.map((workspace) => <NativeSelectOption value={workspace.id} key={workspace.id}>Workspace #{workspace.id}</NativeSelectOption>)}</NativeSelect></label>}
                <p className="text-xs text-muted-foreground">Direct checkout · current branch: {preview?.checkoutDetails.map((checkout) => `${checkout.repositoryName} (${checkout.branch})`).join(", ") ?? "Loading checkout…"}</p>
                <div className="grid gap-2 sm:grid-cols-3">
                  <NativeSelect value={configuration.agent} onChange={(event) => { const agent = event.target.value as "claude" | "codex"; const firstModel = modelCatalog.find((catalog) => catalog.agent === agent)?.models[0]; setConfiguration({ agent, model: firstModel?.id ?? "", effort: firstModel?.efforts[0]?.id ?? "" }); }}><NativeSelectOption value="claude">Claude</NativeSelectOption><NativeSelectOption value="codex">Codex</NativeSelectOption></NativeSelect>
                  <NativeSelect aria-label="Model" value={configuration.model} onChange={(event) => { const model = agentCatalog?.models.find((candidate) => candidate.id === event.target.value); setConfiguration({ ...configuration, model: event.target.value, effort: model?.efforts[0]?.id ?? "" }); }}>{agentCatalog?.models.map((model) => <NativeSelectOption key={model.id} value={model.id}>{model.label}</NativeSelectOption>)}</NativeSelect>
                  <NativeSelect aria-label="Effort" value={configuration.effort} onChange={(event) => setConfiguration({ ...configuration, effort: event.target.value })}>{modelCatalogItem?.efforts.map((effort) => <NativeSelectOption key={effort.id} value={effort.id}>{effort.label}</NativeSelectOption>)}</NativeSelect>
                </div>
                {preview?.dirtyRepositoryIds.length ? <label className="flex items-start gap-2 text-sm"><Checkbox checked={dirtyConsent} onCheckedChange={(checked) => setDirtyConsent(checked === true)} />Allow the agent to use this dirty checkout.</label> : null}
                {preview?.sharedPaths.length ? <label className="flex items-start gap-2 text-sm"><Checkbox checked={sharedConsent} onCheckedChange={(checked) => setSharedConsent(checked === true)} />Allow sharing this checkout with active Runs.</label> : null}
                {preview?.checkoutDetails.length ? <NativeSelect value={repositoryId} onChange={(event) => setRepositoryId(Number(event.target.value))}>{preview.checkoutDetails.map((checkout) => <NativeSelectOption key={checkout.repositoryId} value={checkout.repositoryId}>{checkout.repositoryName} (current branch)</NativeSelectOption>)}</NativeSelect> : null}
                <div className="flex justify-end gap-2"><Button type="button" variant="outline" onClick={() => setLaunchOpen(false)}>Cancel</Button><Button type="button" disabled={!preview || !repositoryId || (Boolean(preview.dirtyRepositoryIds.length) && !dirtyConsent) || (Boolean(preview.sharedPaths.length) && !sharedConsent)} onClick={() => void startQueue()}>Start first ticket</Button></div>
              </section>
            )}
            {document.data.subIssues.length === 0 ? (
              <span className="text-sm text-muted-foreground">
                No tickets yet. Run to-tickets to break this spec down.
              </span>
            ) : (
              <ul className="grid gap-1.5">
                {document.data.subIssues.map((ticket) => (
                  <li
                    key={ticket.url}
                    className="flex flex-wrap items-center gap-2 rounded-md border px-3 py-2 text-sm"
                  >
                    <Checkbox aria-label={`Select ticket #${ticket.number}`} checked={selected.includes(ticket.number)} disabled={ticket.state.toLowerCase() !== "open" || Boolean(activeQueue)} onCheckedChange={(checked) => setSelected((current) => checked ? [...current, ticket.number] : current.filter((number) => number !== ticket.number))} />
                    <Badge variant={ticket.state === "open" ? "secondary" : "outline"}>
                      {ticket.state === "open" ? "Open" : "Closed"}
                    </Badge>
                    <a
                      href={ticket.url}
                      target="_blank"
                      rel="noreferrer"
                      className="min-w-0 flex-1 underline-offset-4 hover:underline"
                    >
                      <span className="text-muted-foreground">#{ticket.number}</span>{" "}
                      {ticket.title}
                    </a>
                    {!linkedUrls.has(ticket.url) && (
                      <span className="text-xs text-muted-foreground">
                        Not linked to this Item
                      </span>
                    )}
                    {progressQueue?.entries.find((entry) => entry.ticketNumber === ticket.number) && (() => { const entry = progressQueue.entries.find((candidate) => candidate.ticketNumber === ticket.number)!; const run = entry.runId ? view.runs.find((candidate) => candidate.id === entry.runId) : undefined; return entry.done ? <Badge variant="secondary">Done</Badge> : entry.skipped ? <Badge variant="outline">Skipped</Badge> : progressQueue.pausedReason && entry.runId ? <Badge variant="destructive">Paused · {queuePauseLabel(progressQueue.pausedReason)}</Badge> : entry.runId && run?.state === "finished" ? <Badge variant="outline">Waiting · Run #{entry.runId}</Badge> : entry.runId ? <Button type="button" size="sm" variant="ghost" onClick={() => onOpenRun(entry.runId!)}>Running · Run #{entry.runId}</Button> : specActiveQueue ? <Badge variant="outline">Queued</Badge> : null; })()}
                  </li>
                ))}
              </ul>
            )}
          </section>

          <section className="grid gap-2" aria-label="Description">
            <SectionLabel>Description</SectionLabel>
            {document.data.body.trim() ? (
              <div className="spec-markdown">
                <Markdown
                  remarkPlugins={[remarkGfm]}
                  components={{
                    a: ({ node: _node, ...props }) => (
                      <a {...props} target="_blank" rel="noreferrer" />
                    ),
                  }}
                >
                  {document.data.body}
                </Markdown>
              </div>
            ) : (
              <span className="text-sm text-muted-foreground">
                The spec has no description.
              </span>
            )}
          </section>
        </>
      )}
    </article>
  );
}
