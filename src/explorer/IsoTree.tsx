import { useState } from "react";
import { Archive, ChevronDown, ChevronRight, File, FileImage, FileText, Folder, FolderOpen, LoaderCircle } from "lucide-react";
import { appActions, useAppStore } from "../app/store";
import { ipc } from "../ipc/client";
import { formatBytes } from "../shared/lib/format";
import { isoNode, memberNode, nodeMetadata, viewerKind, type ExplorerNode } from "./store";

function NodeIcon({ node, expanded }: { node: ExplorerNode; expanded: boolean }) {
  if (node.isDirectory) return expanded ? <FolderOpen size={15} /> : <Folder size={15} />;
  if (node.isArchive) return <Archive size={15} />;
  if (/\.(hpt|zpt)$/i.test(node.name)) return <FileImage size={15} />;
  if (/\.evs$/i.test(node.name)) return <FileText size={15} />;
  return <File size={15} />;
}

function TreeNode({ node, depth }: { node: ExplorerNode; depth: number }) {
  const [expanded, setExpanded] = useState(false);
  const [children, setChildren] = useState<ExplorerNode[] | null>(null);
  const [loading, setLoading] = useState(false);
  const selectedPath = useAppStore((state) => state.selection?.resource);
  const selected = selectedPath?.isoPath === node.resource.isoPath
    && selectedPath.members.map((member) => member.index).join("/") === node.resource.members.map((member) => member.index).join("/");
  const expandable = node.isDirectory || node.isArchive;

  const load = async () => {
    if (children || loading) return;
    setLoading(true);
    try {
      if (node.isDirectory) {
        const result = await ipc.listDirectory(node.resource.sessionId, node.resource.isoPath);
        setChildren(result.items.map((entry) => isoNode(node.resource.sessionId, entry)));
      } else {
        const result = await ipc.listHgarEntries(node.resource);
        setChildren(result.entries.items.map((entry) => memberNode(node.resource, entry)));
      }
    } catch (error) {
      appActions.failTask(error);
      setChildren([]);
    } finally {
      setLoading(false);
    }
  };

  const toggle = async () => {
    if (!expandable) return;
    const next = !expanded;
    setExpanded(next);
    if (next) await load();
  };

  const select = () => appActions.select({ label: node.name, resource: node.resource, metadata: nodeMetadata(node) });
  const open = () => {
    if (node.isDirectory) {
      void toggle();
      return;
    }
    appActions.open({ label: node.name, kind: viewerKind(node), resource: node.resource, metadata: nodeMetadata(node) });
  };

  return (
    <li>
      <div
        className={`tree-row ${selected ? "selected" : ""}`}
        style={{ paddingLeft: 7 + depth * 16 }}
        onClick={select}
        onDoubleClick={open}
        role="treeitem"
        aria-expanded={expandable ? expanded : undefined}
      >
        <button className="tree-toggle" type="button" tabIndex={-1} disabled={!expandable} onClick={(event) => { event.stopPropagation(); void toggle(); }} aria-label={expanded ? "折叠" : "展开"}>
          {loading ? <LoaderCircle className="spin" size={13} /> : expandable ? expanded ? <ChevronDown size={13} /> : <ChevronRight size={13} /> : null}
        </button>
        <NodeIcon node={node} expanded={expanded} />
        <span className="tree-name" title={node.name}>{node.name}</span>
        {!node.isDirectory && <span className="tree-size">{formatBytes(node.size)}</span>}
      </div>
      {expanded && children && (
        <ul role="group">
          {children.map((child) => <TreeNode node={child} depth={depth + 1} key={child.id} />)}
          {children.length === 0 && <li className="tree-empty" style={{ paddingLeft: 32 + depth * 16 }}>空目录或无法解析</li>}
        </ul>
      )}
    </li>
  );
}

export function IsoTree() {
  const sessionId = useAppStore((state) => state.sessionId);
  const rootEntries = useAppStore((state) => state.rootEntries);
  const metadata = useAppStore((state) => state.isoMetadata);

  if (!sessionId) return <div className="pane-empty">尚未打开 ISO</div>;
  const nodes = rootEntries.map((entry) => isoNode(sessionId, entry));
  return (
    <div className="tree-scroll">
      <div className="tree-root-label"><span className="disc-icon">ISO</span>{metadata?.volumeId || "PSP GAME"}</div>
      <ul className="iso-tree" role="tree" aria-label="ISO 文件树">
        {nodes.map((node) => <TreeNode node={node} depth={0} key={node.id} />)}
      </ul>
    </div>
  );
}

