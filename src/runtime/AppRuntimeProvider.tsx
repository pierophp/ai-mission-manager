import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import type {
  ActivityTabView,
  Context,
  ContextAttentionDefault,
  HealthStatus,
  HomeView,
  ItemView,
  Machine,
  Project,
  ProviderChoice,
  Repository,
  RunSuggestion,
  SetupState,
} from "./types";
import { activityAdapter, setupAdapter, structureAdapter, workAdapter } from "./adapters";
import { listen } from "./adapters/tauri";
import { errorMessage } from "./errors";
import { currentMinute } from "./time";

type StructureData = {
  contexts: Context[];
  projects: Project[];
  repositories: Repository[];
  machines: Machine[];
  attentionDefaults: ContextAttentionDefault[];
};

export type AppRuntimeValue = {
  setupState: SetupState | undefined;
  healthStatus: HealthStatus | undefined;
  structure: StructureData;
  home: HomeView | undefined;
  runSuggestions: RunSuggestion[];
  activity: ActivityTabView;
  searchResults: ItemView[];
  contextFilterId: number | undefined;
  searchQuery: string;
  error: string | undefined;
  isLoading: boolean;
  isCheckingDependencies: boolean;
  initialStateLoaded: boolean;
  setError: (message: string | undefined) => void;
  setSearchQuery: (query: string) => void;
  setContextFilter: (contextId: number | undefined) => Promise<void>;
  refreshHome: (contextId?: number) => Promise<void>;
  refreshSearch: (query?: string) => Promise<void>;
  refreshRunSuggestions: () => Promise<void>;
  refreshActivity: () => Promise<void>;
  refreshStructure: () => Promise<StructureData>;
  refreshAll: () => Promise<StructureData>;
  pollExternalObjects: (showErrors?: boolean) => Promise<void>;
  refreshHealthStatus: (provider?: ProviderChoice | null) => Promise<void>;
  completeSetup: (contextName: string, provider: ProviderChoice) => Promise<void>;
};

const AppRuntimeContext = createContext<AppRuntimeValue | undefined>(undefined);

export function AppRuntimeProvider({ children }: { children: ReactNode }) {
  const [setupState, setSetupState] = useState<SetupState>();
  const [healthStatus, setHealthStatus] = useState<HealthStatus>();
  const [structure, setStructure] = useState<StructureData>({
    contexts: [],
    projects: [],
    repositories: [],
    machines: [],
    attentionDefaults: [],
  });
  const [home, setHome] = useState<HomeView>();
  const [runSuggestions, setRunSuggestions] = useState<RunSuggestion[]>([]);
  const [activity, setActivity] = useState<ActivityTabView>({
    audit_entries: [],
    activities: [],
  });
  const [searchResults, setSearchResults] = useState<ItemView[]>([]);
  const [contextFilterId, setContextFilterId] = useState<number>();
  const [searchQuery, setSearchQueryState] = useState("");
  const [error, setError] = useState<string>();
  const [isLoading, setIsLoading] = useState(true);
  const [isCheckingDependencies, setIsCheckingDependencies] = useState(false);
  const [initialStateLoaded, setInitialStateLoaded] = useState(false);

  const refreshStructure = useCallback(async (): Promise<StructureData> => {
    const [contexts, projects, attentionDefaults, repositories, machines] = await Promise.all([
      structureAdapter.listContexts(),
      structureAdapter.listProjects(),
      structureAdapter.listAttentionDefaults(),
      structureAdapter.listRepositories(),
      structureAdapter.listMachines(),
    ]);
    const nextStructure = { contexts, projects, attentionDefaults, repositories, machines };
    setStructure(nextStructure);
    return nextStructure;
  }, []);

  const refreshHome = useCallback(async (nextContextFilterId = contextFilterId) => {
    setHome(await workAdapter.getHome(nextContextFilterId, currentMinute()));
  }, [contextFilterId]);

  const refreshRunSuggestions = useCallback(async () => {
    setRunSuggestions(await workAdapter.listRunSuggestions());
  }, []);

  const refreshActivity = useCallback(async () => {
    setActivity(await activityAdapter.getTab());
  }, []);

  const refreshSearch = useCallback(async (query = searchQuery) => {
    if (!query.trim()) {
      setSearchResults([]);
      return;
    }
    setSearchResults(await workAdapter.searchItems(query));
  }, [searchQuery]);

  const refreshAll = useCallback(async (): Promise<StructureData> => {
    const [nextStructure] = await Promise.all([
      refreshStructure(),
      refreshHome(),
      refreshRunSuggestions(),
      refreshActivity(),
      refreshSearch(),
    ]);
    return nextStructure;
  }, [refreshActivity, refreshHome, refreshRunSuggestions, refreshSearch, refreshStructure]);

  const pollExternalObjects = useCallback(async (showErrors = false) => {
    try {
      const result = await workAdapter.pollExternalObjects();
      if (result.refreshed > 0) {
        await Promise.all([refreshHome(), refreshSearch(), refreshActivity()]);
      }
      if (showErrors && result.failures.length > 0) {
        setError(`Some External Objects could not be refreshed (${result.failures.length}).`);
      }
    } catch (pollError) {
      if (showErrors) setError(errorMessage(pollError));
    }
  }, [refreshActivity, refreshHome, refreshSearch]);

  const refreshHealthStatus = useCallback(async (provider?: ProviderChoice | null) => {
    setIsCheckingDependencies(true);
    try {
      const loadedHealthStatus = await setupAdapter.getHealth(
        provider === undefined ? (setupState?.completed ? null : "github") : provider,
      );
      setHealthStatus(loadedHealthStatus);
      setError(undefined);
    } catch (healthError) {
      setError(errorMessage(healthError));
    } finally {
      setIsCheckingDependencies(false);
    }
  }, [setupState]);

  const completeSetup = useCallback(async (contextName: string, provider: ProviderChoice) => {
    const completed = await setupAdapter.complete(contextName, provider);
    setSetupState(completed);
    await refreshHealthStatus(null);
    await refreshAll();
  }, [refreshAll, refreshHealthStatus]);

  const setContextFilter = useCallback(async (nextContextFilterId: number | undefined) => {
    setContextFilterId(nextContextFilterId);
    setIsLoading(true);
    try {
      await refreshHome(nextContextFilterId);
      setError(undefined);
    } catch (loadError) {
      setError(errorMessage(loadError));
    } finally {
      setIsLoading(false);
    }
  }, [refreshHome]);

  useEffect(() => {
    let disposed = false;
    async function loadInitialState() {
      setIsLoading(true);
      try {
        const [nextStructure, nextSetupState, nextHealthStatus, nextHome, nextSuggestions, nextActivity] =
          await Promise.all([
            refreshStructure(),
            setupAdapter.getState(),
            setupAdapter.getHealth(null),
            workAdapter.getHome(undefined, currentMinute()),
            workAdapter.listRunSuggestions(),
            activityAdapter.getTab(),
          ]);
        if (disposed) return;
        setStructure(nextStructure);
        setSetupState(nextSetupState);
        setHealthStatus(nextHealthStatus);
        setHome(nextHome);
        setRunSuggestions(nextSuggestions);
        setActivity(nextActivity);
        setError(undefined);
        setInitialStateLoaded(true);
        await workAdapter.reconcileRuns();
        if (!disposed) {
          await Promise.all([
            refreshHome(),
            refreshRunSuggestions(),
            refreshActivity(),
            searchQuery.trim() ? refreshSearch() : Promise.resolve(),
          ]);
        }
        if (!disposed) await pollExternalObjects(false);
      } catch (loadError) {
        if (!disposed) setError(errorMessage(loadError));
      } finally {
        if (!disposed) setIsLoading(false);
      }
    }
    void loadInitialState();
    return () => {
      disposed = true;
    };
  }, []);

  useEffect(() => {
    void refreshSearch();
  }, [contextFilterId, initialStateLoaded, refreshSearch, searchQuery]);

  useEffect(() => {
    const interval = window.setInterval(() => {
      void pollExternalObjects(false);
    }, 5 * 60 * 1000);
    return () => window.clearInterval(interval);
  }, [pollExternalObjects]);

  useEffect(() => {
    const interval = window.setInterval(() => {
      void refreshHome().catch(() => undefined);
      if (searchQuery.trim()) void refreshSearch().catch(() => undefined);
    }, 3_000);
    return () => window.clearInterval(interval);
  }, [refreshHome, refreshSearch, searchQuery]);

  useEffect(() => {
    if (!initialStateLoaded) return;
    const interval = window.setInterval(() => {
      void refreshRunSuggestions().catch(() => undefined);
    }, 10_000);
    return () => window.clearInterval(interval);
  }, [initialStateLoaded, refreshRunSuggestions]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen("run-state-changed", () => {
      if (disposed) return;
      void refreshHome();
      void refreshActivity();
      if (searchQuery.trim()) void refreshSearch();
    }).then((cleanup) => {
      if (disposed) cleanup();
      else unlisten = cleanup;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [refreshActivity, refreshHome, refreshSearch, searchQuery]);

  const value = useMemo<AppRuntimeValue>(() => ({
    setupState,
    healthStatus,
    structure,
    home,
    runSuggestions,
    activity,
    searchResults,
    contextFilterId,
    searchQuery,
    error,
    isLoading,
    isCheckingDependencies,
    initialStateLoaded,
    setError,
    setSearchQuery: setSearchQueryState,
    setContextFilter,
    refreshHome,
    refreshSearch,
    refreshRunSuggestions,
    refreshActivity,
    refreshStructure,
    refreshAll,
    pollExternalObjects,
    refreshHealthStatus,
    completeSetup,
  }), [
    activity,
    completeSetup,
    contextFilterId,
    error,
    healthStatus,
    home,
    initialStateLoaded,
    isCheckingDependencies,
    isLoading,
    pollExternalObjects,
    refreshActivity,
    refreshAll,
    refreshHealthStatus,
    refreshHome,
    refreshRunSuggestions,
    refreshSearch,
    refreshStructure,
    runSuggestions,
    searchQuery,
    setContextFilter,
    setupState,
    structure,
    searchResults,
  ]);

  return <AppRuntimeContext.Provider value={value}>{children}</AppRuntimeContext.Provider>;
}

export function useAppRuntime(): AppRuntimeValue {
  const runtime = useContext(AppRuntimeContext);
  if (!runtime) {
    throw new Error("useAppRuntime must be used inside AppRuntimeProvider");
  }
  return runtime;
}
