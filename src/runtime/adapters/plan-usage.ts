import type { PlanUsageSnapshot } from "../types";
import { command } from "./tauri";

export const planUsageAdapter = {
  list: () => command<PlanUsageSnapshot>("list_plan_usage"),
  refresh: () => command<void>("refresh_plan_usage"),
};
