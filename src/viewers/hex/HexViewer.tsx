import { useQuery } from "@tanstack/react-query";
import { Binary, LoaderCircle } from "lucide-react";
import type { OpenedResource } from "../../app/store";
import { ipc } from "../../ipc/client";
import { formatBytes } from "../../shared/lib/format";

function lines(bytes: number[], base: number) {
  const output = [];
  for (let cursor = 0; cursor < bytes.length; cursor += 16) {
    const chunk = bytes.slice(cursor, cursor + 16);
    output.push({
      offset: (base + cursor).toString(16).toUpperCase().padStart(8, "0"),
      hex: chunk.map((value) => value.toString(16).toUpperCase().padStart(2, "0")).join(" ").padEnd(47, " "),
      ascii: chunk.map((value) => value >= 32 && value < 127 ? String.fromCharCode(value) : ".").join(""),
    });
  }
  return output;
}

export function HexViewer({ tab }: { tab: OpenedResource }) {
  const query = useQuery({ queryKey: ["hex", tab.id], queryFn: () => ipc.readResourceRange(tab.resource, 0, 8192) });
  if (query.isLoading) return <div className="viewer-loading"><LoaderCircle className="spin" />读取资源</div>;
  if (query.error) return <div className="viewer-error">{String(query.error)}</div>;
  const chunk = query.data!;
  return (
    <div className="hex-viewer">
      <div className="viewer-toolbar"><Binary size={15} /><strong>Hex</strong><span>{formatBytes(chunk.total)}</span><span className="toolbar-spacer" /><span>显示前 {formatBytes(chunk.bytes.length)}</span></div>
      <div className="hex-grid">
        {lines(chunk.bytes, chunk.offset).map((line) => (
          <div className="hex-line" key={line.offset}><code className="hex-offset">{line.offset}</code><code>{line.hex}</code><code className="hex-ascii">{line.ascii}</code></div>
        ))}
      </div>
    </div>
  );
}

