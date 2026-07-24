import type { HgarEntry, IsoEntry, ResourceRef, SessionId } from "../ipc/bindings";
import type { ViewerKind } from "../app/store";

export interface ExplorerNode {
  id: string;
  name: string;
  kind: "iso" | "member";
  isDirectory: boolean;
  isArchive: boolean;
  size: number;
  compressed: boolean;
  resource: ResourceRef;
  isoEntry?: IsoEntry;
  hgarEntry?: HgarEntry;
}

export function isoNode(sessionId: SessionId, entry: IsoEntry): ExplorerNode {
  const resource: ResourceRef = { sessionId, isoPath: entry.path, members: [] };
  return {
    id: entry.path,
    name: entry.name,
    kind: "iso",
    isDirectory: entry.isDirectory,
    isArchive: !entry.isDirectory && entry.name.toLowerCase().endsWith(".har"),
    size: entry.size,
    compressed: false,
    resource,
    isoEntry: entry,
  };
}

export function memberNode(parent: ResourceRef, entry: HgarEntry): ExplorerNode {
  const resource: ResourceRef = {
    ...parent,
    members: [...parent.members, { index: entry.index, name: entry.displayName }],
  };
  return {
    id: `${parent.isoPath}#${resource.members.map((member) => member.index).join("/")}`,
    name: entry.displayName,
    kind: "member",
    isDirectory: false,
    isArchive: entry.displayName.toLowerCase().endsWith(".har"),
    size: entry.size,
    compressed: entry.compressed,
    resource,
    hgarEntry: entry,
  };
}

export function viewerKind(node: ExplorerNode): ViewerKind {
  const name = node.name.toLowerCase();
  if (node.isArchive) return "hgar";
  if (name.endsWith(".evs")) return "evs";
  if (name.endsWith(".hpt") || name.endsWith(".zpt")) return "image";
  if (name === "param.sfo" || name.endsWith(".json")) return "metadata";
  return "hex";
}

export function nodeMetadata(node: ExplorerNode): Record<string, string | number | boolean | null> {
  return {
    类型: node.isDirectory ? "ISO 目录" : node.isArchive ? "HGAR 归档" : "文件",
    大小: node.size,
    压缩: node.compressed,
    Offset: node.hgarEntry?.contentOffset ?? node.isoEntry?.extent ?? null,
    标识符: node.hgarEntry?.identifier ?? null,
  };
}

