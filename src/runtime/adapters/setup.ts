import type {
  HealthStatus,
  ProviderChoice,
  SetupState,
} from "../types";
import { command } from "./tauri";

export const setupAdapter = {
  getState: () => command<SetupState>("get_setup_state"),
  complete: (contextName: string, provider: ProviderChoice) =>
    command<SetupState>("complete_setup", { contextName, provider }),
  getHealth: (provider: ProviderChoice | null) =>
    command<HealthStatus>("get_health_status", { provider }),
};
