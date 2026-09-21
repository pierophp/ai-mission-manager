import { useEffect } from "react";
import {
  createRootRoute,
  createRoute,
  createRouter,
  redirect,
  useRouter,
} from "@tanstack/react-router";

import { AppShell } from "./App";

function UnknownRoute() {
  const router = useRouter();

  useEffect(() => {
    void router.navigate({ to: "/work", replace: true });
  }, [router]);

  return (
    <main className="app-shell">
      <p className="empty-state">That route is not available. Returning to Work…</p>
    </main>
  );
}

const rootRoute = createRootRoute({
  component: AppShell,
  notFoundComponent: UnknownRoute,
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
