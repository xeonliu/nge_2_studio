import { useQuery } from "@tanstack/react-query";
import { Archive, Box, FileImage, FileText, LoaderCircle } from "lucide-react";
import type { OpenedResource } from "../../app/store";
import { appActions } from "../../app/store";
import { memberNode, nodeMetadata, viewerKind } from "../../explorer/store";
import { ipc } from "../../ipc/client";
import { formatBytes, formatHex } from "../../shared/lib/format";

export function HgarViewer({ tab }: { tab: OpenedResource }) {
  const query = useQuery({
    queryKey: ["hgar", tab.id],
    queryFn: () => ipc.listHgarEntries(tab.resource),
  });
  if (query.isLoading) return <div className="viewer-loading"><LoaderCircle className="spin" />解析 HGAR 目录</div>;
  if (query.error) return <div className="viewer-error">{String(query.error)}</div>;
  const listing = query.data!;
  return (
    <div className="table-viewer">
      <div className="viewer-toolbar">
        <Archive size={15} />
        <strong>HGAR v{listing.version}</strong>
        <span>{listing.entries.total} 个成员</span>
        <span className="toolbar-spacer" />
        <span>双击打开成员</span>
      </div>
      <div className="data-table" role="table" aria-label="HGAR 成员">
        <div className="data-row data-header" role="row">
          <span>名称</span><span>ID</span><span>Offset</span><span>大小</span><span>状态</span>
        </div>
        {listing.entries.items.map((entry) => {
          const node = memberNode(tab.resource, entry);
          const Icon = entry.displayName.endsWith(".evs") ? FileText : /\.(hpt|zpt)$/i.test(entry.displayName) ? FileImage : Box;
          return (
            <button
              className="data-row"
              role="row"
              type="button"
              key={entry.index}
              onClick={() => appActions.select({ label: node.name, resource: node.resource, metadata: nodeMetadata(node) })}
              onDoubleClick={() => appActions.open({ label: node.name, kind: viewerKind(node), resource: node.resource, metadata: nodeMetadata(node) })}
            >
              <span className="file-cell"><Icon size={14} />{entry.displayName}</span>
              <code>{entry.identifier}</code>
              <code>{formatHex(entry.contentOffset, 8)}</code>
              <span>{formatBytes(entry.size)}</span>
              <span>{entry.compressed ? <em className="compression-tag">DEFLATE</em> : "Raw"}</span>
            </button>
          );
        })}
      </div>
    </div>
  );
}

