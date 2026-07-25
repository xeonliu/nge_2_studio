import { open } from "@tauri-apps/plugin-dialog";
import { convertFileSrc } from "@tauri-apps/api/core";
import { commands } from "./bindings";
import type {
  AudioPreview,
  BinaryChunk,
  EvsCommandPage,
  EvsDocument,
  EvsFramePage,
  HgarListing,
  ImagePreview,
  OpenIsoResponse,
  Page,
  ResourceRef,
  SessionId,
  SessionStatus,
  SoundEffectPreview,
  IsoEntry,
  VisualReference,
} from "./bindings";
import { mockIpc } from "./mock";

type CommandResult<T> = { status: "ok"; data: T } | { status: "error"; error: string };

function unwrap<T>(result: CommandResult<T>): T {
  if (result.status === "error") throw new Error(result.error);
  return result.data;
}

export function isTauriRuntime() {
  return "__TAURI_INTERNALS__" in window;
}

const realIpc = {
  async pickIso() {
    const selected = await open({ title: "打开 PSP ISO", multiple: false, directory: false, filters: [{ name: "PSP ISO", extensions: ["iso"] }] });
    return typeof selected === "string" ? selected : null;
  },
  async openIso(path: string) { return unwrap(await commands.openIso(path)); },
  async listDirectory(sessionId: SessionId, path: string) { return unwrap(await commands.listDirectory(sessionId, path, null)); },
  async listEventArchives(sessionId: SessionId) { return unwrap(await commands.listEventArchives(sessionId, null)); },
  async listHgarEntries(resource: ResourceRef) { return unwrap(await commands.listHgarEntries(resource, null)); },
  async openEvs(resource: ResourceRef) { return unwrap(await commands.openEvs(resource)); },
  async getEvsCommands(resource: ResourceRef) { return unwrap(await commands.getEvsCommands(resource, null)); },
  async getEvsFrames(resource: ResourceRef) { return unwrap(await commands.getEvsFrames(resource, null)); },
  async selectEvsVariant(document: ResourceRef, commandIndex: number, selected: ResourceRef) { return unwrap(await commands.selectEvsVariant(document, commandIndex, selected)); },
  async readResourceRange(resource: ResourceRef, offset: number, length: number) { return unwrap(await commands.readResourceRange(resource, offset, length)); },
  async getImagePreview(resource: ResourceRef) {
    const preview = unwrap(await commands.getImagePreview(resource));
    return { ...preview, url: convertFileSrc(preview.token, "nge2-preview") };
  },
  async getAudioPreview(document: ResourceRef, voiceId: number) {
    const preview = unwrap(await commands.getAudioPreview(document, voiceId));
    return { ...preview, url: convertFileSrc(preview.token, "nge2-preview") };
  },
  async getSoundEffectPreview(document: ResourceRef, soundId: number) {
    const preview = unwrap(await commands.getSoundEffectPreview(document, soundId));
    return { ...preview, url: convertFileSrc(preview.token, "nge2-preview") };
  },
  async getSessionStatus(sessionId: SessionId) { return unwrap(await commands.getSessionStatus(sessionId)); },
  demoEvsResource(_sessionId: SessionId): ResourceRef {
    throw new Error("演示资源仅在浏览器开发模式可用");
  },
};

export interface StudioIpc {
  pickIso(): Promise<string | null>;
  openIso(path: string): Promise<OpenIsoResponse>;
  listDirectory(sessionId: SessionId, path: string): Promise<Page<IsoEntry>>;
  listEventArchives(sessionId: SessionId): Promise<Page<IsoEntry>>;
  listHgarEntries(resource: ResourceRef): Promise<HgarListing>;
  openEvs(resource: ResourceRef): Promise<EvsDocument>;
  getEvsCommands(resource: ResourceRef): Promise<EvsCommandPage>;
  getEvsFrames(resource: ResourceRef): Promise<EvsFramePage>;
  selectEvsVariant(document: ResourceRef, commandIndex: number, selected: ResourceRef): Promise<VisualReference>;
  readResourceRange(resource: ResourceRef, offset: number, length: number): Promise<BinaryChunk>;
  getImagePreview(resource: ResourceRef): Promise<ImagePreview & { url: string }>;
  getAudioPreview(document: ResourceRef, voiceId: number): Promise<AudioPreview & { url: string }>;
  getSoundEffectPreview(document: ResourceRef, soundId: number): Promise<SoundEffectPreview & { url: string }>;
  getSessionStatus(sessionId: SessionId): Promise<SessionStatus>;
  demoEvsResource(sessionId: SessionId): ResourceRef;
}

export const ipc: StudioIpc = isTauriRuntime() ? realIpc : mockIpc;
