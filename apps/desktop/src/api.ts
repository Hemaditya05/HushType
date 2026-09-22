// Typed wrappers around the Rust commands and events.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type HotkeyMode = "hold" | "toggle" | "hybrid";
export type InsertMethod = "auto" | "type" | "paste";
export type AppContext = "general" | "code" | "terminal" | "chat" | "document" | "browser";

export interface Settings {
  launchAtStartup: boolean;
  startMinimized: boolean;
  showIndicator: boolean;
  indicatorTop: boolean;
  playSounds: boolean;
  theme: "system" | "light" | "dark";
  model: string;
  language: string;
  microphone: string;
  silenceTimeoutMs: number;
  vadSensitivity: number;
  livePreview: boolean;
  unloadAfterMin: number;
  loadModelAtStartup: boolean;
  maxRecordingSec: number;
  removeFillers: boolean;
  smartPunctuation: boolean;
  autoCapitalize: boolean;
  spokenPunctuation: boolean;
  aggressiveCleanup: boolean;
  contextAware: boolean;
  insertMethod: InsertMethod;
  restoreClipboard: boolean;
  hotkey: string;
  hotkeyMode: HotkeyMode;
  saveHistory: boolean;
  storeAudio: boolean;
  historyLimit: number;
  onboarded: boolean;
}

export type ModelStatus =
  | { state: "unloaded" }
  | { state: "loading"; model: string }
  | { state: "loaded"; model: string; loadMs: number; backend: string }
  | { state: "error"; model: string; message: string };

export interface Status {
  phase: "idle" | "recording" | "processing";
  hotkey: string;
  hotkeyError: string | null;
  model: ModelStatus;
  modelId: string;
  modelInstalled: boolean;
  cpuSupported: boolean;
}

export interface Snapshot {
  settings: Settings;
  status: Status;
  version: string;
}

export interface InputDevice {
  name: string;
  is_default: boolean;
}

export interface ModelEntry {
  id: string;
  name: string;
  file: string;
  size_bytes: number;
  english_only: boolean;
  ram_mb: number;
  speed: number;
  accuracy: number;
  description: string;
  installed: boolean;
  selected: boolean;
  recommended: boolean;
}

export interface Hardware {
  cpu: { logical_cores: number; avx2: boolean; avx512: boolean; fma: boolean };
  threads: number;
  ramTotalMb: number;
  ramAvailableMb: number;
  gpus: { name: string; dedicated_vram_mb: number; integrated: boolean }[];
  gpuBackends: string[];
}

export interface ModelsPayload {
  models: ModelEntry[];
  hardware: Hardware;
  status: ModelStatus;
  modelsDir: string;
  downloading: boolean;
}

export interface DownloadEvent {
  id: string;
  downloaded: number;
  total: number;
  bytesPerSec: number;
  done: boolean;
  error: string | null;
}

export interface Term {
  term: string;
  aliases: string[];
}

export interface HistoryEntry {
  id: number;
  timestamp: number;
  app: string;
  raw: string;
  text: string;
  language: string;
  durationMs: number;
  audioFile?: string;
}

export interface Diagnostics {
  workingSetMb: number;
  privateMb: number;
  whisper: string;
  paths: { config: string; data: string; models: string; logs: string; recordings: string };
  elevated: boolean;
}

export interface DictationResult {
  text: string;
  app: string;
  outcome: "inserted" | "copied" | "failed";
  audioMs: number;
  finalLatencyMs: number;
}

export const api = {
  getState: () => invoke<Snapshot>("get_state"),
  saveSettings: (settings: Settings) => invoke<{ warnings: string[] }>("save_settings", { settings }),
  checkHotkey: (label: string) => invoke<{ label: string; warning: string | null }>("check_hotkey", { label }),
  suspendHotkey: (suspended: boolean) => invoke<void>("suspend_hotkey", { suspended }),
  toggleDictation: () => invoke<void>("toggle_dictation"),
  listMicrophones: () => invoke<InputDevice[]>("list_microphones"),
  startMicTest: (device: string) => invoke<void>("start_mic_test", { device }),
  stopMicTest: () => invoke<void>("stop_mic_test"),
  listModels: () => invoke<ModelsPayload>("list_models"),
  downloadModel: (id: string) => invoke<void>("download_model", { id }),
  cancelDownload: () => invoke<void>("cancel_download"),
  deleteModel: (id: string) => invoke<void>("delete_model", { id }),
  loadModel: () => invoke<void>("load_model"),
  unloadModel: () => invoke<void>("unload_model"),
  getDictionary: () => invoke<Term[]>("get_dictionary"),
  saveDictionary: (terms: Term[]) => invoke<void>("save_dictionary", { terms }),
  resetDictionary: () => invoke<Term[]>("reset_dictionary"),
  previewText: (raw: string, context: AppContext) => invoke<string>("preview_text", { raw, context }),
  getHistory: (offset: number, limit: number, query: string) =>
    invoke<{ entries: HistoryEntry[]; total: number }>("get_history", { offset, limit, query }),
  deleteHistory: (id: number) => invoke<void>("delete_history", { id }),
  clearHistory: () => invoke<void>("clear_history"),
  copyText: (text: string) => invoke<void>("copy_text", { text }),
  openExternal: (target: string) => invoke<void>("open_external", { target }),
  diagnostics: () => invoke<Diagnostics>("diagnostics"),
  finishOnboarding: () => invoke<void>("finish_onboarding"),
};

export function on<T>(event: string, cb: (payload: T) => void): () => void {
  let unlisten: UnlistenFn | null = null;
  let cancelled = false;
  listen<T>(event, (e) => cb(e.payload)).then((u) => {
    if (cancelled) u();
    else unlisten = u;
  });
  return () => {
    cancelled = true;
    if (unlisten) unlisten();
  };
}

export function formatBytes(n: number): string {
  if (n >= 1 << 30) return `${(n / (1 << 30)).toFixed(1)} GB`;
  if (n >= 1 << 20) return `${Math.round(n / (1 << 20))} MB`;
  return `${Math.round(n / 1024)} KB`;
}

export function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : JSON.stringify(e);
}
