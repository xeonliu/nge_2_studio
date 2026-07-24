import { Disc3, FileCog } from "lucide-react";
import type { OpenedResource } from "../../app/store";
import { useAppStore } from "../../app/store";
import { formatBytes } from "../../shared/lib/format";

export function MetadataViewer({ tab }: { tab: OpenedResource }) {
  const iso = useAppStore((state) => state.isoMetadata);
  return (
    <div className="metadata-viewer">
      <div className="viewer-toolbar"><FileCog size={15} /><strong>元数据</strong></div>
      <section className="metadata-band">
        <div className="metadata-icon"><Disc3 size={38} /></div>
        <div><h1>{tab.label}</h1><code>{tab.resource.isoPath}</code></div>
      </section>
      <section className="metadata-properties">
        <h2>ISO 卷</h2>
        <dl>
          <dt>Volume ID</dt><dd>{iso?.volumeId ?? "--"}</dd>
          <dt>镜像大小</dt><dd>{iso ? formatBytes(iso.volumeSize) : "--"}</dd>
          <dt>逻辑块</dt><dd>{iso?.logicalBlockSize ?? "--"} bytes</dd>
          {Object.entries(tab.metadata).map(([key, value]) => <span key={key}><dt>{key}</dt><dd>{String(value ?? "--")}</dd></span>)}
        </dl>
      </section>
    </div>
  );
}
