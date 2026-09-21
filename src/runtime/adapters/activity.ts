import type { ActivityTabView } from "../types";
import { command } from "./tauri";

export const activityAdapter = {
  getTab: () => command<ActivityTabView>("get_activity_tab"),
};
