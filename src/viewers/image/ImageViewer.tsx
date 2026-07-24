import { useQuery } from "@tanstack/react-query";
import { Grid3X3, LoaderCircle, Maximize, Minus, Plus } from "lucide-react";
import { useState } from "react";
import type { OpenedResource } from "../../app/store";
import { ipc } from "../../ipc/client";
import { IconButton } from "../../shared/ui/IconButton";

export function ImageViewer({ tab }: { tab: OpenedResource }) {
  const [zoom, setZoom] = useState(1);
  const [checker, setChecker] = useState(true);
  const query = useQuery({ queryKey: ["preview", tab.id], queryFn: () => ipc.getImagePreview(tab.resource) });
  if (query.isLoading) return <div className="viewer-loading"><LoaderCircle className="spin" />解码 HGPT</div>;
  if (query.error) return <div className="viewer-error">{String(query.error)}</div>;
  const image = query.data!;
  return (
    <div className="image-viewer">
      <div className="viewer-toolbar">
        <strong>{image.width} × {image.height}</strong>
        <span>{image.pixelFormat}</span>
        <span>{image.divisions.length} divisions</span>
        <span className="toolbar-spacer" />
        <IconButton icon={Minus} label="缩小" onClick={() => setZoom((value) => Math.max(0.25, value - 0.25))} />
        <span className="zoom-value">{Math.round(zoom * 100)}%</span>
        <IconButton icon={Plus} label="放大" onClick={() => setZoom((value) => Math.min(8, value + 0.25))} />
        <IconButton icon={Maximize} label="实际大小" onClick={() => setZoom(1)} />
        <IconButton icon={Grid3X3} label="切换透明背景" className={checker ? "active" : ""} onClick={() => setChecker((value) => !value)} />
      </div>
      <div className={`image-canvas ${checker ? "checker" : ""}`}>
        <img src={image.url} width={image.width * zoom} height={image.height * zoom} alt={tab.label} draggable={false} />
      </div>
    </div>
  );
}

