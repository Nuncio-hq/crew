import { invokeTauri } from "@/shared/api/tauri";

import type {
  RecapSettings,
  RecapSettingsSnapshot,
  ThreadRecap,
  ThreadRecapGenerationRequest,
  ThreadRecapLookup,
  ThreadRecapRequest,
} from "./types";

export const RECAP_COMMANDS = {
  getSettings: "get_recap_settings",
  saveSettings: "save_recap_settings",
  getThreadRecap: "get_thread_recap",
  generateThreadRecap: "generate_thread_recap",
  cancelThreadRecap: "cancel_thread_recap",
} as const;

export type RecapCommandInvoker = <T>(
  command: string,
  args?: Record<string, unknown>,
) => Promise<T>;

export type RecapClient = {
  getSettings: () => Promise<RecapSettingsSnapshot>;
  saveSettings: (settings: RecapSettings) => Promise<RecapSettingsSnapshot>;
  getThreadRecap: (request: ThreadRecapRequest) => Promise<ThreadRecapLookup>;
  generateThreadRecap: (
    request: ThreadRecapGenerationRequest,
  ) => Promise<ThreadRecap>;
  cancelThreadRecap: (request: ThreadRecapGenerationRequest) => Promise<void>;
};

/**
 * Keep Tauri command names in one Crew-owned seam. Tests and the future native
 * service can inject the same invoker without duplicating command payloads.
 */
export function createRecapClient(
  invoke: RecapCommandInvoker = invokeTauri,
): RecapClient {
  return {
    getSettings: () =>
      invoke<RecapSettingsSnapshot>(RECAP_COMMANDS.getSettings),
    saveSettings: (settings) =>
      invoke<RecapSettingsSnapshot>(RECAP_COMMANDS.saveSettings, { settings }),
    getThreadRecap: (request) =>
      invoke<ThreadRecapLookup>(RECAP_COMMANDS.getThreadRecap, request),
    generateThreadRecap: (request) =>
      invoke<ThreadRecap>(RECAP_COMMANDS.generateThreadRecap, request),
    cancelThreadRecap: (request) =>
      invoke<void>(RECAP_COMMANDS.cancelThreadRecap, request),
  };
}

export const recapClient = createRecapClient();
