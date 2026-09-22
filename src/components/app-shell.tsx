import { type FormEvent, useEffect, useRef, useState } from "react";
import { createContext, useContext } from "react";
import { Link, Outlet, useRouterState } from "@tanstack/react-router";
import { Moon, Sun } from "lucide-react";

import { useHealthStatusQuery, useSetupStateQuery } from "../features/setup/setup-queries";
import {
  useCompleteSetupMutation,
  useHealthCheckMutation,
} from "../features/setup/setup-mutations";
import { useStructureData } from "../features/structure/structure-queries";
import {
  applyTheme,
  loadStoredTheme,
  loadTheme,
  saveTheme,
  type Theme,
} from "../theme";
import { errorMessage } from "../runtime/errors";
import type {
  DependencyState,
  HealthStatus,
  ProviderChoice,
} from "../runtime/types";
import type { PaneTab } from "../runtime/terminal-types";
import { EmbeddedTerminal } from "./terminal-runtime/EmbeddedTerminal";
import { Alert, AlertDescription } from "./ui/alert";
import { Badge } from "./ui/badge";
import { Button } from "./ui/button";
import { Card } from "./ui/card";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogTitle,
} from "./ui/dialog";
import { Input } from "./ui/input";
import { appShellLayoutClassName } from "./app-shell-layout";

type AppShellContextValue = {
  openTerminal: (runId: number, pane: PaneTab) => void;
  closeTerminal: () => void;
};

const AppShellContext = createContext<AppShellContextValue | undefined>(
  undefined,
);

export function useAppShell() {
  const value = useContext(AppShellContext);
  if (!value) throw new Error("useAppShell must be used inside AppShell");
  return value;
}

type AppTab = "work" | "settings" | "activity";

const appTabs: {
  id: AppTab;
  label: string;
  description: string;
  path: "/work" | "/settings/contexts" | "/activity";
}[] = [
  {
    id: "work",
    label: "Work",
    description: "One calm view of what needs your attention, what is moving, and what is waiting.",
    path: "/work",
  },
  {
    id: "settings",
    label: "Settings",
    description: "Contexts, Projects, Repositories, Machines, and defaults for your work.",
    path: "/settings/contexts",
  },
  {
    id: "activity",
    label: "Activity",
    description: "A record of the actions the app has taken and the changes it has observed.",
    path: "/activity",
  },
];

const outlineButtonClass =
  "border-input bg-transparent text-muted-foreground hover:border-primary hover:bg-secondary hover:text-primary";

function appTabForPath(pathname: string): AppTab {
  if (pathname.startsWith("/settings")) return "settings";
  if (pathname === "/activity") return "activity";
  return "work";
}

export function AppShell() {
  const setupQuery = useSetupStateQuery();
  const [setupContextName, setSetupContextName] = useState("Personal");
  const [setupProvider, setSetupProvider] = useState<ProviderChoice>("github");
  const [error, setError] = useState<string>();
  const [showHealthDetails, setShowHealthDetails] = useState(false);
  const [theme, setTheme] = useState<Theme>(loadTheme);
  const themePreferenceRef = useRef<Theme | undefined>(loadStoredTheme());
  const [terminalRequest, setTerminalRequest] = useState<{
    runId: number;
    pane: PaneTab;
  }>();
  const setupState = setupQuery.data;
  const structureQuery = useStructureData();
  const structure = structureQuery.data;
  const healthQuery = useHealthStatusQuery(
    setupState?.completed ? null : setupProvider,
  );
  const healthStatus = healthQuery.data;
  const queryError =
    setupQuery.error ?? structureQuery.error ?? healthQuery.error;
  const setupMutation = useCompleteSetupMutation();
  const healthMutation = useHealthCheckMutation();
  const pathname = useRouterState({
    select: (state) => state.location.pathname,
  });
  const activeTab = appTabForPath(pathname);
  const activeTabDetails = appTabs.find((tab) => tab.id === activeTab) ?? appTabs[0];
  const isDarkTheme = theme === "dark";
  const themeToggleLabel = isDarkTheme ? "Switch to light mode" : "Switch to dark mode";
  const themeModeLabel = isDarkTheme ? "Light mode" : "Dark mode";
  const runtimeState = healthStatus?.runtime.state ?? "unavailable";
  const isSaving = setupMutation.isPending;
  const isCheckingDependencies = healthMutation.isPending;

  useEffect(() => {
    applyTheme(theme);
    if (themePreferenceRef.current) saveTheme(theme);
  }, [theme]);

  useEffect(() => {
    if (structure.contexts[0] && (!setupContextName.trim() || setupContextName === "Personal")) {
      setSetupContextName(structure.contexts[0].name);
    }
  }, [setupContextName, structure.contexts]);

  useEffect(() => {
    if (setupState) setSetupProvider(setupState.completed ? setupState.provider : "github");
  }, [setupState]);

  async function handleCompleteSetup(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    try {
      await setupMutation.mutateAsync({
        contextName: setupContextName,
        provider: setupProvider,
      });
      setError(undefined);
    } catch (setupError) {
      setError(errorMessage(setupError));
    }
  }

  return (
    <main className={appShellLayoutClassName}>
      <header className="flex items-start justify-between gap-6 max-[510px]:flex-col">
        <div>
          <p className="mb-2.5 text-[0.72rem] font-bold uppercase tracking-[0.14em] text-primary">
            AI Mission Manager
          </p>
          <h1 className="font-heading text-4xl leading-[0.95] font-medium tracking-[-0.035em] sm:text-6xl">
            {activeTabDetails.label}
          </h1>
          <p className="mt-3.5 max-w-[620px] text-base leading-relaxed text-muted-foreground">
            {activeTabDetails.description}
          </p>
        </div>
        <div className="flex items-start gap-3.5 max-[510px]:w-full max-[510px]:flex-wrap">
          <Button
            type="button"
            variant="outline"
            size="sm"
            className={`h-[34px] gap-1.5 px-2.5 text-xs ${outlineButtonClass}`}
            aria-label={themeToggleLabel}
            aria-pressed={isDarkTheme}
            onClick={() => {
              const nextTheme = isDarkTheme ? "light" : "dark";
              themePreferenceRef.current = nextTheme;
              setTheme(nextTheme);
            }}
          >
            {isDarkTheme ? <Sun aria-hidden="true" /> : <Moon aria-hidden="true" />}
            <span>{themeModeLabel}</span>
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            className="h-auto min-h-8 gap-2 px-2 text-xs text-muted-foreground hover:bg-secondary hover:text-primary max-[510px]:whitespace-normal"
            aria-label="Runtime and provider health"
            aria-expanded={showHealthDetails}
            onClick={() => setShowHealthDetails((current) => !current)}
          >
            <span
              aria-hidden="true"
              className={`size-2 shrink-0 rounded-full ring-4 ${
                runtimeState === "available"
                  ? "bg-emerald-500 ring-emerald-500/15"
                  : runtimeState === "unavailable"
                    ? "bg-destructive ring-destructive/15"
                    : "bg-amber-500 ring-amber-500/15"
              }`}
            />
            <span>Runtime: {healthStatus ? dependencyStateLabel(healthStatus.runtime.state) : "Checking"}</span>
            <span className="text-muted-foreground/60">·</span>
            <span>GitHub: {healthStatus ? dependencyStateLabel(healthStatus.provider.state) : "Checking"}</span>
          </Button>
        </div>
      </header>

      <TabNavigation activeTab={activeTab} />

      {showHealthDetails && setupState?.completed && healthStatus && (
        <HealthDetails
          health={healthStatus}
          isCheckingDependencies={isCheckingDependencies}
          onCheckDependencies={() => void healthMutation.mutateAsync(null)}
        />
      )}

      {setupState && !setupState.completed && (
        <SetupWizard
          contextName={setupContextName}
          provider={setupProvider}
          health={healthStatus}
          isSaving={isSaving}
          isCheckingDependencies={isCheckingDependencies}
          onContextNameChange={setSetupContextName}
          onProviderChange={setSetupProvider}
          onCheckDependencies={() => void healthMutation.mutateAsync(setupProvider)}
          onSubmit={handleCompleteSetup}
        />
      )}

      {(error ?? (queryError ? errorMessage(queryError) : undefined)) && (
        <ErrorAlert message={error ?? errorMessage(queryError)} />
      )}

      <Dialog
        open={Boolean(terminalRequest)}
        onOpenChange={(open) => {
          if (!open) setTerminalRequest(undefined);
        }}
      >
        {terminalRequest && (
          <DialogContent
            className="h-[90vh] max-h-[900px] w-[calc(100%-1rem)] grid-cols-1 grid-rows-1 overflow-hidden p-3 sm:max-w-[1440px] sm:p-5"
            showCloseButton={false}
          >
            <DialogTitle className="sr-only">
              Embedded terminal · Run #{terminalRequest.runId}
            </DialogTitle>
            <DialogDescription className="sr-only">
              Terminal session for this Run. Closing the view leaves the Run running.
            </DialogDescription>
            <EmbeddedTerminal
              key={`${terminalRequest.runId}-${terminalRequest.pane.paneId}`}
              runId={terminalRequest.runId}
              initialPane={terminalRequest.pane}
              onClose={() => setTerminalRequest(undefined)}
            />
          </DialogContent>
        )}
      </Dialog>

      <AppShellContext.Provider
        value={{
          openTerminal: (runId, pane) => setTerminalRequest({ runId, pane }),
          closeTerminal: () => setTerminalRequest(undefined),
        }}
      >
        <Outlet />
      </AppShellContext.Provider>
    </main>
  );
}

function ErrorAlert({ message }: { message: string }) {
  return (
    <Alert className="mt-4 border-destructive/30 bg-destructive/5" variant="destructive">
      <AlertDescription>{message}</AlertDescription>
    </Alert>
  );
}

function TabNavigation({ activeTab }: { activeTab: AppTab }) {
  return (
    <nav className="mt-8 border-b border-border" aria-label="Main sections">
      <div className="flex gap-6 overflow-x-auto">
        {appTabs.map((tab) => (
          <Button
            asChild
            key={tab.id}
            variant="ghost"
            size="sm"
            className={`h-auto shrink-0 rounded-none border-b-2 px-0.5 py-3 text-xs font-bold uppercase tracking-[0.08em] hover:bg-transparent hover:text-primary ${
              activeTab === tab.id
                ? "border-primary text-foreground"
                : "border-transparent text-muted-foreground"
            }`}
          >
            <Link to={tab.path} aria-current={activeTab === tab.id ? "page" : undefined}>
              {tab.label}
            </Link>
          </Button>
        ))}
      </div>
    </nav>
  );
}

function SetupWizard({
  contextName,
  provider,
  health,
  isSaving,
  isCheckingDependencies,
  onContextNameChange,
  onProviderChange,
  onCheckDependencies,
  onSubmit,
}: {
  contextName: string;
  provider: ProviderChoice;
  health: HealthStatus | undefined;
  isSaving: boolean;
  isCheckingDependencies: boolean;
  onContextNameChange: (value: string) => void;
  onProviderChange: (value: ProviderChoice) => void;
  onCheckDependencies: () => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
}) {
  return (
    <section className="mt-8" aria-labelledby="setup-heading">
      <Card className="gap-0 rounded-2xl border-primary/30 bg-card p-6 shadow-none ring-0">
        <div className="flex items-start justify-between gap-6 max-[510px]:flex-col">
          <div>
            <p className="mb-2.5 text-[0.72rem] font-bold uppercase tracking-[0.14em] text-primary">
              First run
            </p>
            <h2 id="setup-heading" className="font-heading text-2xl font-medium tracking-tight">
              Make the app ready for your work
            </h2>
            <p className="mt-3 max-w-[720px] text-sm leading-relaxed text-muted-foreground">
              Choose your first Context, decide whether to connect GitHub, and check the tools
              already installed on this Machine. Mission Manager never installs or authenticates
              anything on your behalf.
            </p>
          </div>
          <span className="shrink-0 text-sm font-semibold text-muted-foreground">Three quick checks</span>
        </div>
        <form className="mt-6 grid gap-4" onSubmit={onSubmit}>
          <div className="grid gap-3 rounded-xl border border-border bg-background/60 p-4 sm:grid-cols-[30px_minmax(0,1fr)]">
            <span className="grid size-7 place-items-center rounded-full bg-primary text-xs font-bold text-primary-foreground">1</span>
            <div className="grid gap-3">
              <div>
                <h3 className="font-heading text-base font-medium">Create a Context</h3>
                <p className="mt-1 text-sm text-muted-foreground">
                  A Context keeps its Items, providers, and Machines together.
                </p>
              </div>
              <label className="grid gap-1.5 text-sm font-medium">
                <span>Context name</span>
                <Input
                  value={contextName}
                  onChange={(event) => onContextNameChange(event.target.value)}
                  placeholder="Personal"
                  disabled={isSaving}
                  autoFocus
                />
              </label>
            </div>
          </div>
          <div className="grid gap-3 rounded-xl border border-border bg-background/60 p-4 sm:grid-cols-[30px_minmax(0,1fr)]">
            <span className="grid size-7 place-items-center rounded-full bg-primary text-xs font-bold text-primary-foreground">2</span>
            <div className="grid gap-3">
              <div>
                <h3 className="font-heading text-base font-medium">Choose a provider</h3>
                <p className="mt-1 text-sm text-muted-foreground">
                  You can keep local work here and connect a provider later.
                </p>
              </div>
              <div className="grid gap-2">
                <label className="flex items-start gap-2 rounded-lg border border-border p-3 text-sm font-normal">
                  <input
                    type="radio"
                    name="provider"
                    value="github"
                    checked={provider === "github"}
                    onChange={() => onProviderChange("github")}
                    disabled={isSaving}
                    className="mt-0.5 size-4 accent-primary"
                  />
                  <span className="grid gap-1">
                    <strong>GitHub</strong>
                    <small className="text-muted-foreground">Use the installed `gh` CLI for Issues and pull requests.</small>
                  </span>
                </label>
                <label className="flex items-start gap-2 rounded-lg border border-border p-3 text-sm font-normal">
                  <input
                    type="radio"
                    name="provider"
                    value="none"
                    checked={provider === "none"}
                    onChange={() => onProviderChange("none")}
                    disabled={isSaving}
                    className="mt-0.5 size-4 accent-primary"
                  />
                  <span className="grid gap-1">
                    <strong>No provider yet</strong>
                    <small className="text-muted-foreground">Local Items and Runs remain available.</small>
                  </span>
                </label>
              </div>
            </div>
          </div>
          <div className="grid gap-3 rounded-xl border border-border bg-background/60 p-4 sm:grid-cols-[30px_minmax(0,1fr)]">
            <span className="grid size-7 place-items-center rounded-full bg-primary text-xs font-bold text-primary-foreground">3</span>
            <div>
              <div className="flex items-start justify-between gap-4 max-[760px]:flex-col">
                <div>
                  <h3 className="font-heading text-base font-medium">Check dependencies</h3>
                  <p className="mt-1 text-sm text-muted-foreground">
                    Missing or unauthenticated tools do not stop the rest of the app.
                  </p>
                </div>
                <Button
                  type="button"
                  variant="outline"
                  size="sm"
                  className={outlineButtonClass}
                  onClick={onCheckDependencies}
                  disabled={isCheckingDependencies || isSaving}
                >
                  {isCheckingDependencies ? "Checking…" : "Check again"}
                </Button>
              </div>
              <DependencyList health={health} />
            </div>
          </div>
          <div className="flex items-center justify-between gap-4 max-[760px]:items-stretch max-[760px]:flex-col">
            <p className="m-0 text-sm text-muted-foreground">
              You can revisit tool setup from the health indicator at any time.
            </p>
            <Button type="submit" disabled={isSaving || !contextName.trim()}>
              {isSaving ? "Saving setup…" : "Finish setup"}
            </Button>
          </div>
        </form>
      </Card>
    </section>
  );
}

function HealthDetails({
  health,
  isCheckingDependencies,
  onCheckDependencies,
}: {
  health: HealthStatus;
  isCheckingDependencies: boolean;
  onCheckDependencies: () => void;
}) {
  return (
    <section className="mt-6" aria-label="Tool health details">
      <Card className="gap-0 rounded-xl border-border bg-card p-5 shadow-none ring-0">
        <div className="flex items-start justify-between gap-4 border-b border-border pb-3 max-[760px]:flex-col max-[760px]:items-stretch">
          <div>
            <p className="mb-2.5 text-[0.72rem] font-bold uppercase tracking-[0.14em] text-primary">
              Tool health
            </p>
            <h2 className="font-heading text-xl font-medium">Runtime and provider status</h2>
          </div>
          <Button
            type="button"
            variant="outline"
            size="sm"
            className={outlineButtonClass}
            onClick={onCheckDependencies}
            disabled={isCheckingDependencies}
          >
            {isCheckingDependencies ? "Checking…" : "Check again"}
          </Button>
        </div>
        <DependencyList health={health} />
      </Card>
    </section>
  );
}

function DependencyList({ health }: { health: HealthStatus | undefined }) {
  const dependencies = health ? [health.runtime, health.provider, ...health.agents] : [];

  return (
    <ul className="mt-3 grid gap-2">
      {dependencies.length === 0 ? (
        <li className="border-t border-border pt-2 text-sm text-muted-foreground">
          Checking installed tools…
        </li>
      ) : (
        dependencies.map((dependency) => (
          <li
            className="grid items-start gap-3 border-t border-border pt-2 sm:grid-cols-[92px_minmax(0,1fr)]"
            key={dependency.key}
          >
            <Badge
              variant={dependency.state === "available" ? "secondary" : "outline"}
              className={dependency.state === "available" ? "text-emerald-700 dark:text-emerald-300" : "text-amber-700 dark:text-amber-300"}
            >
              {dependencyStateLabel(dependency.state)}
            </Badge>
            <span className="grid min-w-0 gap-0.5 text-xs leading-relaxed text-muted-foreground">
              <strong className="text-foreground">{dependency.label}</strong>
              <span>{dependency.message}</span>
              {dependency.executablePath && <code className="break-all text-[0.7rem]">{dependency.executablePath}</code>}
              {dependency.action && <small>{dependency.action}</small>}
            </span>
          </li>
        ))
      )}
    </ul>
  );
}

function dependencyStateLabel(state: DependencyState): string {
  switch (state) {
    case "available":
      return "Ready";
    case "missing":
      return "Missing";
    case "unauthenticated":
      return "Needs login";
    case "notConfigured":
      return "Not selected";
    case "unavailable":
      return "Unavailable";
  }
}
