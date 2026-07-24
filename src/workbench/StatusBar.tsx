import { useEffect } from "react";
import { Circle, Database, LoaderCircle } from "lucide-react";
import { appActions, useAppStore } from "../app/store";
import { ipc } from "../ipc/client";
import { formatBytes } from "../shared/lib/format";

export function StatusBar() {
  const sessionId = useAppStore((state) => state.sessionId);
  const metadata = useAppStore((state) => state.isoMetadata);
  const selection = useAppStore((state) => state.selection);
  const busy = useAppStore((state) => state.busyCount);
  const message = useAppStore((state) => state.statusMessage);
  const cacheBytes = useAppStore((state) => state.cacheBytes);

  useEffect(() => {
    if (!sessionId) return;
    const update = () => ipc.getSessionStatus(sessionId).then((status) => appActions.setCacheBytes(status.cacheBytes)).catch(() => undefined);
    update();
    const timer = window.setInterval(update, 5000);
    return () => window.clearInterval(timer);
  }, [sessionId]);

  return (
    <footer className="statusbar">
      <span className="status-item primary">
        {busy ? <LoaderCircle className="spin" size={13} /> : <Circle className="status-dot" size={8} fill="currentColor" />}
        {message}
      </span>
      <span className="status-item path-status">{selection?.resource?.isoPath ?? "--"}</span>
      <span className="status-item">ISO9660 · {metadata?.logicalBlockSize ?? 2048} B</span>
      <span className="status-item"><Database size={13} /> 缓存 {formatBytes(cacheBytes)}</span>
    </footer>
  );
}
