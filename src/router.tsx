import {
  createRootRoute,
  createRoute,
  createRouter,
  redirect,
} from "@tanstack/react-router";

import { AppShell } from "./components/app-shell";
import { AppRuntimeProvider } from "./runtime/AppRuntimeProvider";
import { parseWorkSearch } from "./features/work/work-search";

function RootLayout() {
  return (
    <AppRuntimeProvider>
      <AppShell />
    </AppRuntimeProvider>
  );
}

const rootRoute = createRootRoute({
  component: RootLayout,
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
});

const structureRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/structure",
});

const activityRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: "/activity",
});

const routeTree = rootRoute.addChildren([
  indexRoute,
  workRoute,
  structureRoute,
  activityRoute,
]);

export const router = createRouter({
  routeTree,
  notFoundMode: "root",
});

declare module "@tanstack/react-router" {
  interface Register {
    router: typeof router;
  }
}
