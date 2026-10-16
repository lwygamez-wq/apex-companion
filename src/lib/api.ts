// Couche d'accès IPC : l'UI n'appelle jamais `invoke` directement.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppSettings,
  Comparison,
  ConnectorInfo,
  Dashboard,
  ImportReport,
  InsertOutcome,
  SessionDetail,
  SessionFilter,
  SessionSummary,
  StatusPayload,
} from "./types";

export const isTauri = (): boolean => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!isTauri()) {
    throw new Error("Apex Companion doit être lancé via Tauri (npm run tauri dev).");
  }
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    // Les erreurs Rust arrivent sous forme de chaîne lisible (ServiceError).
    throw new Error(typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e));
  }
}

export const api = {
  status: () => call<StatusPayload>("get_status"),
  connectors: () => call<ConnectorInfo[]>("list_connectors"),
  sessions: (filter?: SessionFilter) => call<SessionSummary[]>("list_sessions", { filter: filter ?? null }),
  session: (id: number) => call<SessionDetail>("get_session", { id }),
  importFiles: (paths: string[]) => call<ImportReport[]>("import_files", { paths }),
  scanIracing: () => call<ImportReport[]>("scan_iracing"),
  loadDemo: () => call<InsertOutcome>("load_demo"),
  compare: (referenceLapId: number, comparedLapId: number, stepM = 1) =>
    call<Comparison>("compare_laps", { referenceLapId, comparedLapId, stepM }),
  setReference: (lapId: number) => call<void>("set_reference", { lapId }),
  deleteSession: (id: number) => call<void>("delete_session", { id }),
  deleteAll: () => call<void>("delete_all_data"),
  exportData: (path: string) => call<number>("export_data", { path }),
  diagnostics: () => call<unknown>("get_diagnostics"),
  dashboard: () => call<Dashboard>("get_dashboard"),
  settings: () => call<AppSettings>("get_settings"),
  saveSettings: (settings: AppSettings) => call<AppSettings>("save_settings", { settings }),
};

export function onStatus(cb: (s: StatusPayload) => void): Promise<UnlistenFn> {
  if (!isTauri()) return Promise.resolve(() => undefined);
  return listen<StatusPayload>("apex://status", (e) => cb(e.payload));
}

export function onSessionsChanged(cb: () => void): Promise<UnlistenFn> {
  if (!isTauri()) return Promise.resolve(() => undefined);
  return listen("apex://sessions-changed", () => cb());
}
