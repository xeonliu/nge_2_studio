import { useSyncExternalStore } from "react";
import type {
  DialogueFrame,
  EvsCommand,
  IsoEntry,
  IsoMetadata,
  ResourceRef,
  SessionId,
  VisualReference,
} from "../ipc/bindings";

export type Theme = "system" | "light" | "dark";
export type ExplorerMode = "files" | "events";
export type ViewerKind = "evs" | "hgar" | "image" | "metadata" | "hex";

export interface OpenedResource {
  id: string;
  label: string;
  kind: ViewerKind;
  resource: ResourceRef;
  metadata: Record<string, string | number | boolean | null>;
}

export interface Selection {
  label: string;
  resource?: ResourceRef;
  metadata: Record<string, string | number | boolean | null>;
}

interface PersistedSettings {
  explorerWidth: number;
  inspectorWidth: number;
  theme: Theme;
  recentIsos: string[];
}

interface AppState extends PersistedSettings {
  sessionId: SessionId | null;
  isoMetadata: IsoMetadata | null;
  rootEntries: IsoEntry[];
  explorerMode: ExplorerMode;
  tabs: OpenedResource[];
  activeTabId: string | null;
  selection: Selection | null;
  selectedCommand: EvsCommand | null;
  selectedFrame: DialogueFrame | null;
  selectedVisual: VisualReference | null;
  statusMessage: string;
  cacheBytes: number;
  busyCount: number;
  error: string | null;
}

const defaults: PersistedSettings = {
  explorerWidth: 280,
  inspectorWidth: 320,
  theme: "system",
  recentIsos: [],
};

function readSettings(): PersistedSettings {
  try {
    return { ...defaults, ...JSON.parse(localStorage.getItem("nge2-studio.settings") ?? "{}") };
  } catch {
    return defaults;
  }
}

let state: AppState = {
  ...readSettings(),
  sessionId: null,
  isoMetadata: null,
  rootEntries: [],
  explorerMode: "files",
  tabs: [],
  activeTabId: null,
  selection: null,
  selectedCommand: null,
  selectedFrame: null,
  selectedVisual: null,
  statusMessage: "未打开 ISO",
  cacheBytes: 0,
  busyCount: 0,
  error: null,
};

const listeners = new Set<() => void>();

function emit() {
  for (const listener of listeners) listener();
}

function update(patch: Partial<AppState> | ((current: AppState) => Partial<AppState>)) {
  state = { ...state, ...(typeof patch === "function" ? patch(state) : patch) };
  emit();
}

function persist() {
  const value: PersistedSettings = {
    explorerWidth: state.explorerWidth,
    inspectorWidth: state.inspectorWidth,
    theme: state.theme,
    recentIsos: state.recentIsos,
  };
  localStorage.setItem("nge2-studio.settings", JSON.stringify(value));
}

function resourceId(resource: ResourceRef) {
  return `${resource.isoPath}#${resource.members.map((member) => member.index).join("/")}`;
}

export const appActions = {
  startTask(message: string) {
    update((current) => ({ busyCount: current.busyCount + 1, statusMessage: message, error: null }));
  },
  finishTask(message: string) {
    update((current) => ({ busyCount: Math.max(0, current.busyCount - 1), statusMessage: message }));
  },
  failTask(error: unknown) {
    const message = error instanceof Error ? error.message : String(error);
    update((current) => ({ busyCount: Math.max(0, current.busyCount - 1), error: message, statusMessage: "操作失败" }));
  },
  setSession(sessionId: SessionId, metadata: IsoMetadata, rootEntries: IsoEntry[]) {
    const recentIsos = [metadata.sourcePath, ...state.recentIsos.filter((path) => path !== metadata.sourcePath)].slice(0, 8);
    update({
      sessionId,
      isoMetadata: metadata,
      rootEntries,
      recentIsos,
      tabs: [],
      activeTabId: null,
      selection: null,
      statusMessage: `已打开 ${metadata.volumeId || "ISO"}`,
      error: null,
    });
    persist();
  },
  setExplorerMode(explorerMode: ExplorerMode) {
    update({ explorerMode });
  },
  select(selection: Selection | null) {
    update({ selection, selectedCommand: null, selectedFrame: null, selectedVisual: null });
  },
  open(resource: Omit<OpenedResource, "id">) {
    const id = resourceId(resource.resource);
    update((current) => ({
      tabs: current.tabs.some((tab) => tab.id === id)
        ? current.tabs
        : [...current.tabs, { ...resource, id }],
      activeTabId: id,
      selection: { label: resource.label, resource: resource.resource, metadata: resource.metadata },
      selectedCommand: null,
      selectedFrame: null,
      selectedVisual: null,
    }));
  },
  closeTab(id: string) {
    update((current) => {
      const index = current.tabs.findIndex((tab) => tab.id === id);
      const tabs = current.tabs.filter((tab) => tab.id !== id);
      const next = current.activeTabId === id ? tabs[Math.max(0, index - 1)]?.id ?? null : current.activeTabId;
      return { tabs, activeTabId: next };
    });
  },
  activateTab(activeTabId: string) {
    update({ activeTabId, selectedCommand: null, selectedFrame: null, selectedVisual: null });
  },
  inspectCommand(command: EvsCommand | null, frame: DialogueFrame | null, visual: VisualReference | null = null) {
    update({ selectedCommand: command, selectedFrame: frame, selectedVisual: visual });
  },
  setCacheBytes(cacheBytes: number) {
    update({ cacheBytes });
  },
  setExplorerWidth(explorerWidth: number) {
    update({ explorerWidth: Math.min(460, Math.max(220, explorerWidth)) });
    persist();
  },
  setInspectorWidth(inspectorWidth: number) {
    update({ inspectorWidth: Math.min(480, Math.max(260, inspectorWidth)) });
    persist();
  },
  cycleTheme() {
    const theme: Theme = state.theme === "system" ? "light" : state.theme === "light" ? "dark" : "system";
    update({ theme });
    persist();
  },
  dismissError() {
    update({ error: null });
  },
};

export function getAppState() {
  return state;
}

export function useAppStore<T>(selector: (state: AppState) => T): T {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => selector(state),
    () => selector(state),
  );
}

