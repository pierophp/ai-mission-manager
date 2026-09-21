import type { QueryClient } from "@tanstack/react-query";
import {
  createRootRouteWithContext,
  createRoute,
  createRouter,
  redirect,
} from "@tanstack/react-router";

import { AppShell } from "./components/app-shell";
import { ActivityPage } from "./features/activity/ActivityPage";
import { StructurePage } from "./features/structure/StructurePage";
import { WorkPage } from "./features/work/WorkPage";
import {
  activityQueryOptions,
  homeQueryOptions,
  runSuggestionsQueryOptions,
  searchQueryOptions,
} from "./features/work/work-queries";
import {
  healthStatusQueryOptions,
  setupStateQueryOptions,
} from "./features/setup/setup-queries";
import { structureQueryOptions } from "./features/structure/structure-queries";
import { parseWorkSearch } from "./features/work/work-search";
import { RuntimeEventsBridge } from "./runtime/RuntimeEventsBridge";
import { workAdapter } from "./runtime/adapters";

type RouterContext = { queryClient: QueryClient };

function RootLayout() {
  return (
    <>
      <RuntimeEventsBridge />
      <AppShell />
    </>
  );
}

const rootRoute = createRootRouteWithContext<RouterContext>()({
  component: RootLayout,
  loader: ({ context }) => {
    void (async () => {
      try {
        await workAdapter.reconcileRuns();
        await Promise.all([
          context.queryClient.prefetchQuery(setupStateQueryOptions()),
          context.queryClient.prefetchQuery(healthStatusQueryOptions(null)),
          context.queryClient.prefetchQuery(structureQueryOptions.contexts()),
          context.queryClient.prefetchQuery(structureQueryOptions.projects()),
          context.queryClient.prefetchQuery(structureQueryOptions.repositories()),
          context.queryClient.prefetchQuery(structureQueryOptions.machines()),
          context.queryClient.prefetchQuery(
            structureQueryOptions.attentionDefaults(),
          ),
          context.queryClient.prefetchQuery(runSuggestionsQueryOptions()),
          context.queryClient.prefetchQuery(activityQueryOptions()),
        ]);
      } catch {
        // Mounted queries own rendering and error state. Root preload is best effort.
      }
    })();
  },
});

const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/",
  beforeLoad: () => {
    throw redirect({ to: "/work", replace: true });
  },
});

const workRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/work",
  validateSearch: parseWorkSearch,
  loaderDeps: ({ search }) => search,
  loader: ({ context, deps }) => {
    void context.queryClient.prefetchQuery(homeQueryOptions(deps.contextId));
    void context.queryClient.prefetchQuery(runSuggestionsQueryOptions());
    if (deps.q?.trim()) {
      void context.queryClient.prefetchQuery(
        searchQueryOptions(deps.q, deps.contextId),
      );
    }
  },
  component: WorkPage,
});

const structureRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/structure",
  component: StructurePage,
});

const activityRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/activity",
  loader: ({ context }) => {
    void context.queryClient.prefetchQuery(activityQueryOptions());
  },
  component: ActivityPage,
});

const routeTree = rootRoute.addChildren([
  indexRoute,
  workRoute,
  structureRoute,
  activityRoute,
]);

export const router = createRouter({
  routeTree,
  context: undefined!,
  defaultPreload: "intent",
  defaultPreloadStaleTime: 0,
  notFoundMode: "root",
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
