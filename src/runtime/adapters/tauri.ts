import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export function command<T>(name: string, args?: Record<string, unknown>): Promise<T> {
  return invoke<T>(name, args);
}

export { listen };
