import { useEffect, useState } from "react";
import { Archive, LoaderCircle } from "lucide-react";
import type { IsoEntry } from "../ipc/bindings";
import { appActions, useAppStore } from "../app/store";
import { ipc } from "../ipc/client";
import { formatBytes } from "../shared/lib/format";
import { isoNode, nodeMetadata } from "./store";

export function EventBrowser() {
  const sessionId = useAppStore((state) => state.sessionId);
  const [entries, setEntries] = useState<IsoEntry[]>([]);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (!sessionId) return;
    setLoading(true);
    ipc.listEventArchives(sessionId)
      .then((page) => setEntries(page.items))
      .catch(appActions.failTask)
      .finally(() => setLoading(false));
  }, [sessionId]);

  if (!sessionId) return <div className="pane-empty">尚未打开 ISO</div>;
  if (loading) return <div className="pane-loading"><LoaderCircle className="spin" size={16} />正在列出事件归档</div>;
  return (
    <div className="event-list" role="listbox" aria-label="事件 HGAR">
      <div className="event-list-header"><span>事件归档</span><span>{entries.length}</span></div>
      {entries.map((entry) => {
        const node = isoNode(sessionId, entry);
        return (
          <button
            type="button"
            className="event-row"
            key={entry.path}
            onClick={() => appActions.select({ label: entry.name, resource: node.resource, metadata: nodeMetadata(node) })}
            onDoubleClick={() => appActions.open({ label: entry.name, kind: "hgar", resource: node.resource, metadata: nodeMetadata(node) })}
          >
            <Archive size={15} />
            <span>{entry.name}</span>
            <small>{formatBytes(entry.size)}</small>
          </button>
        );
      })}
    </div>
  );
}
