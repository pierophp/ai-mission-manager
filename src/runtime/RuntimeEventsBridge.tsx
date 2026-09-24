import { useEffect } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";

import { workAdapter } from "./adapters";
import { invalidateRunQueries, invalidateWorkQueries } from "./query-invalidation";
import { listen } from "./adapters/tauri";

export function RuntimeEventsBridge() {
  const queryClient = useQueryClient();
  const reconcileMutation = useMutation({
    mutationFn: workAdapter.reconcileRuns,
    onSuccess: async (result) => {
      if (result.changed) await invalidateRunQueries(queryClient);
    },
  });
  const pollMutation = useMutation({
    mutationFn: workAdapter.pollExternalObjects,
    onSuccess: async (result) => {
      if (result.refreshed > 0) await invalidateWorkQueries(queryClient);
    },
  });
  const reconcileRuns = reconcileMutation.mutateAsync;
  const pollExternalObjects = pollMutation.mutateAsync;

  useEffect(() => {
    let disposed = false;
    let timeout: number | undefined;
    const reconcileAndSchedule = () => {
      void reconcileRuns()
        .catch(() => undefined)
        .finally(() => {
          if (!disposed) {
            timeout = window.setTimeout(reconcileAndSchedule, 3_000);
          }
        });
    };
    reconcileAndSchedule();
    return () => {
      disposed = true;
      if (timeout !== undefined) window.clearTimeout(timeout);
    };
  }, [reconcileRuns]);

  useEffect(() => {
    const interval = window.setInterval(() => {
      void pollExternalObjects().catch(() => undefined);
    }, 5 * 60 * 1000);
    return () => window.clearInterval(interval);
  }, [pollExternalObjects]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void listen("run-state-changed", () => {
      if (!disposed) void invalidateRunQueries(queryClient);
    }).then((cleanup) => {
      if (disposed) cleanup();
      else unlisten = cleanup;
    });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [queryClient]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void listen("run-questions-changed", () => {
      if (!disposed) void invalidateRunQueries(queryClient);
    }).then((cleanup) => {
      if (disposed) cleanup();
      else unlisten = cleanup;
    });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [queryClient]);

  return null;
}

export function usePollExternalObjects() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: workAdapter.pollExternalObjects,
    onSuccess: async (result) => {
      if (result.refreshed > 0) await invalidateWorkQueries(queryClient);
    },
  });
}
