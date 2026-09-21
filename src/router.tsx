import { useEffect } from "react";
import {
  createRootRoute,
  createRoute,
  createRouter,
  redirect,
  useRouter,
} from "@tanstack/react-router";

import { AppShell } from "./components/app-shell";
import { appShellLayoutClassName } from "./components/app-shell-layout";
import { Empty, EmptyDescription } from "./components/ui/empty";
import { AppRuntimeProvider } from "./runtime/AppRuntimeProvider";
import { parseWorkSearch } from "./features/work/work-search";

function RootLayout() {
  return (
    <AppRuntimeProvider>
      <AppShell />
    </AppRuntimeProvider>
  );
}

function UnknownRoute() {
  const router = useRouter();

  useEffect(() => {
    void router.navigate({ to: "/work", replace: true });
  }, [router]);

  return (
    <main className={appShellLayoutClassName}>
      <Empty className="items-start border-0 p-0 py-8 text-left">
        <EmptyDescription>That route is not available. Returning to Work…</EmptyDescription>
      </Empty>
    </main>
  );
}

const rootRoute = createRootRoute({
  component: RootLayout,
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
