import type { StudioIpc } from "./client";
import type {
  DialogueFrame,
  EvsCommand,
  FormatDiagnostic,
  HgarEntry,
  IsoEntry,
  Page,
  Resolution,
  ResourceRef,
  SessionId,
  VisualReference,
} from "./bindings";

const SESSION = "browser-demo-session";

function page<T>(items: T[]): Page<T> {
  return { items, offset: 0, total: items.length, hasMore: false };
}

function isoEntry(name: string, path: string, isDirectory: boolean, size: number, extent: number): IsoEntry {
  return { name, path, isDirectory, size, extent, modified: "2006-02-03 09:29:23" };
}

function resource(isoPath: string, members: { index: number; name: string }[] = []): ResourceRef {
  return { sessionId: SESSION, isoPath, members };
}

const directories: Record<string, IsoEntry[]> = {
  "/": [
    isoEntry("PSP_GAME", "/PSP_GAME", true, 2048, 38912),
    isoEntry("UMD_DATA.BIN", "/UMD_DATA.BIN", false, 48, 43008),
  ],
  "/PSP_GAME": [
    isoEntry("SYSDIR", "/PSP_GAME/SYSDIR", true, 2048, 47104),
    isoEntry("USRDIR", "/PSP_GAME/USRDIR", true, 4096, 51200),
    isoEntry("PARAM.SFO", "/PSP_GAME/PARAM.SFO", false, 912, 55296),
    isoEntry("ICON0.PNG", "/PSP_GAME/ICON0.PNG", false, 18432, 57344),
  ],
  "/PSP_GAME/USRDIR": [
    isoEntry("event", "/PSP_GAME/USRDIR/event", true, 16384, 65536),
    isoEntry("face", "/PSP_GAME/USRDIR/face", true, 12288, 81920),
    isoEntry("chara", "/PSP_GAME/USRDIR/chara", true, 8192, 94208),
    isoEntry("map", "/PSP_GAME/USRDIR/map", true, 8192, 102400),
  ],
  "/PSP_GAME/USRDIR/event": [
    isoEntry("a000.har", "/PSP_GAME/USRDIR/event/a000.har", false, 774472, 143360),
    isoEntry("a001.har", "/PSP_GAME/USRDIR/event/a001.har", false, 479288, 917504),
    isoEntry("a002.har", "/PSP_GAME/USRDIR/event/a002.har", false, 843440, 1398784),
    isoEntry("cev101.har", "/PSP_GAME/USRDIR/event/cev101.har", false, 238904, 2242560),
  ],
};

const a000Members: HgarEntry[] = [
  { index: 0, identifier: 3, encodedIdentifier: 381492, shortName: "a000.evs", longName: "a000.evs", displayName: "a000.evs", compressed: true, size: 3892, contentOffset: 288, unknownFirst: 30441, unknownLast: 0 },
  { index: 1, identifier: 12, encodedIdentifier: 50812, shortName: "scn_1.zpt", longName: "scn_1.hpt", displayName: "scn_1.hpt", compressed: true, size: 189244, contentOffset: 4200, unknownFirst: 28411, unknownLast: 24 },
  { index: 2, identifier: 7, encodedIdentifier: 10928, shortName: "scn_2.zpt", longName: "scn_2.hpt", displayName: "scn_2.hpt", compressed: true, size: 176520, contentOffset: 193464, unknownFirst: 28412, unknownLast: 24 },
  { index: 3, identifier: 22, encodedIdentifier: 99312, shortName: "shinji_1.zpt", longName: "shinji_1.hpt", displayName: "shinji_1.hpt", compressed: true, size: 82940, contentOffset: 370004, unknownFirst: 28413, unknownLast: 8 },
  { index: 4, identifier: 29, encodedIdentifier: 77182, shortName: "station.zpt", longName: "station.hpt", displayName: "station.hpt", compressed: true, size: 231104, contentOffset: 452964, unknownFirst: 28414, unknownLast: 32 },
];

function member(index: number) {
  const entry = a000Members[index];
  return resource("/PSP_GAME/USRDIR/event/a000.har", [{ index, name: entry.displayName }]);
}

function diagnostic(message: string, offset: number, severity: "info" | "warning" | "error" = "warning"): FormatDiagnostic {
  return { message, offset, severity };
}

function command(index: number, opcode: number, name: string, parameters: number[] = [], content: string | null = null): EvsCommand {
  const offset = 64 + index * 24;
  return {
    index,
    offset,
    opcode,
    opcodeHex: `0x${opcode.toString(16).toUpperCase().padStart(2, "0")}`,
    name,
    parameters,
    content,
    contentBytes: content ? new TextEncoder().encode(content).length : 0,
    rawPayload: [...parameters.flatMap((value) => [value & 255, (value >> 8) & 255, (value >> 16) & 255, (value >> 24) & 255]), ...(content ? [...new TextEncoder().encode(content), 0] : [])],
    supported: opcode !== 0xfe,
    diagnostics: opcode === 0xfe ? [diagnostic("未知 opcode，已保留原始 payload 并继续解析", offset)] : [],
  };
}

const commands: EvsCommand[] = [
  command(0, 0x8c, "VISUAL 0x8C", [0], "station"),
  command(1, 0x90, "WAIT", [300]),
  command(2, 0x01, "SAY", [1, 5, 1042], "……这里是第三新东京市。"),
  command(3, 0x8d, "VISUAL 0x8D", [1], "shinji_1"),
  command(4, 0x01, "SAY", [1, 7, 1043], "父亲为什么要叫我来？▽已经三年没见了。"),
  command(5, 0x95, "AUDIO", [2201], "se_station"),
  command(6, 0x87, "EXTENSION", [637]),
  command(7, 0x8e, "VISUAL 0x8E", [0], "scn_$w"),
  command(8, 0x01, "SAY", [0x1001, 3, 0x4000], "风越来越大了……"),
  command(9, 0x69, "CONTROL", []),
  command(10, 0x20, "COMMAND", [1, 0, 12, 4]),
  command(11, 0x90, "WAIT", [800]),
  command(12, 0x01, "SAY", [4, 2, 1050], "真嗣君，听得到吗？"),
  command(13, 0x8c, "VISUAL 0x8C", [0], "missing_cut"),
  command(14, 0xfe, "UNKNOWN", [], null),
  command(15, 0x01, "SAY", [1, 8, 1051], "这个声音是……葛城上尉？"),
  command(16, 0x78, "CONTROL", [12, 0]),
  command(17, 0x90, "WAIT", [240]),
  command(18, 0x8d, "VISUAL 0x8D", [0], "shinji_1"),
  command(19, 0x01, "SAY", [1, 5, 1052], "我明白了。"),
];

const exactStation: VisualReference = { commandIndex: 0, opcode: 0x8c, requested: "station", resolution: { status: "Exact", value: member(4) }, evidence: "在 EVS 所属 HGAR 内精确匹配文件名" };
const exactPortrait: VisualReference = { commandIndex: 3, opcode: 0x8d, requested: "shinji_1", resolution: { status: "Exact", value: member(3) }, evidence: "在 EVS 所属 HGAR 内精确匹配文件名" };
const variants: VisualReference = { commandIndex: 7, opcode: 0x8e, requested: "scn_$w", resolution: { status: "Variant", value: [member(1), member(2)] }, evidence: "仅在 EVS 所属 HGAR 内展开 $w/$x/$y/$d/$e/$f 候选" };
const missing: VisualReference = { commandIndex: 13, opcode: 0x8c, requested: "missing_cut", resolution: { status: "Missing" }, evidence: "所属 HGAR 内没有精确文件名或合法变量候选" };

function portrait(hidden = false): DialogueFrame["portrait"] {
  return {
    archivePath: "/PSP_GAME/USRDIR/face/f01_05.har",
    staticMember: "f01_05_1.hpt",
    atlasMember: "f01_05_2.hpt",
    resolution: { status: "Exact", value: resource("/PSP_GAME/USRDIR/face/f01_05.har", [{ index: 0, name: "f01_05_1.hpt" }]) },
    runtimeHidden: hidden,
  };
}

function frame(commandIndex: number, text: string, expression: number, visuals: VisualReference[], hidden = false): DialogueFrame {
  return {
    commandIndex,
    text,
    textBytes: new TextEncoder().encode(text).length,
    pages: text.split("▽"),
    speakerId: 1,
    speakerName: "碇真嗣",
    expressionId: expression,
    expressionName: `表情 ${expression}`,
    audioId: hidden ? null : 1040 + commandIndex,
    portrait: portrait(hidden),
    visuals,
    diagnostics: hidden ? [diagnostic("保留头像关联，但运行时隐藏头像", commands[commandIndex].offset, "info")] : [],
  };
}

const frames: DialogueFrame[] = [
  frame(2, "……这里是第三新东京市。", 5, [exactStation]),
  frame(4, "父亲为什么要叫我来？▽已经三年没见了。", 7, [exactStation, exactPortrait]),
  frame(8, "风越来越大了……", 3, [exactStation, exactPortrait, variants], true),
  frame(12, "真嗣君，听得到吗？", 2, [exactStation, variants]),
  frame(15, "这个声音是……葛城上尉？", 8, [missing, variants]),
  frame(19, "我明白了。", 5, [missing, exactPortrait]),
];

let selectedVariant: ResourceRef | null = null;

function applyVariant(reference: VisualReference): VisualReference {
  return selectedVariant && reference.commandIndex === 7
    ? { ...reference, resolution: { status: "Exact", value: selectedVariant }, evidence: "用户从同一 HGAR 的变量候选中选择" }
    : reference;
}

export const mockIpc: StudioIpc = {
  async pickIso() { return "__browser_demo__"; },
  async openIso() {
    return {
      sessionId: SESSION,
      metadata: { sourcePath: "/Users/demo/ULJS00064.iso", volumeId: "PSP GAME", logicalBlockSize: 2048, volumeSize: 890306560 },
      root: page(directories["/"]),
    };
  },
  async listDirectory(_sessionId, path) { return page(directories[path] ?? []); },
  async listEventArchives() { return page(directories["/PSP_GAME/USRDIR/event"]); },
  async listHgarEntries() { return { resource: resource("/PSP_GAME/USRDIR/event/a000.har"), version: 3, entries: page(a000Members) }; },
  async openEvs(evsResource) { return { resource: evsResource, commandCount: commands.length, frameCount: frames.length, diagnosticCount: 2, diagnostics: [diagnostic("1 条未知命令已保留", commands[14].offset)] }; },
  async getEvsCommands() { return { page: page(commands) }; },
  async getEvsFrames() {
    const visualReferences = [exactStation, exactPortrait, applyVariant(variants), missing];
    return { page: page(frames.map((item) => ({ ...item, visuals: item.visuals.map(applyVariant) }))), visualReferences, diagnostics: [diagnostic("missing_cut 未在所属 HGAR 内找到", commands[13].offset)] };
  },
  async selectEvsVariant(_document, commandIndex, selected) {
    if (commandIndex !== 7) throw new Error("所选资源不在候选列表中");
    selectedVariant = selected;
    return applyVariant(variants);
  },
  async readResourceRange(_resource, offset, length) {
    const seed = [...new TextEncoder().encode("HGAR NGE2 ISO Studio read-only preview ")];
    const bytes = Array.from({ length: Math.min(length, 8192) }, (_, index) => seed[(index + offset) % seed.length] ^ ((index * 13) & 0xff));
    return { offset, total: 774472, bytes };
  },
  async getImagePreview(imageResource) {
    const name = imageResource.members.at(-1)?.name ?? "";
    const isPortrait = name.includes("f01_") || name.includes("shinji");
    return { url: isPortrait ? "/demo-portrait.png" : "/demo-stage.png", mime: "image/png", width: isPortrait ? 220 : 480, height: 272, pixelFormat: "Indexed8", divisions: [], approximate: false };
  },
  async getSessionStatus() { return { cacheBytes: 12_746_208 }; },
  demoEvsResource(sessionId: SessionId) { return { sessionId, isoPath: "/PSP_GAME/USRDIR/event/a000.har", members: [{ index: 0, name: "a000.evs" }] }; },
};

const _resolutionTypeCheck: Resolution = { status: "Missing" };
void _resolutionTypeCheck;

