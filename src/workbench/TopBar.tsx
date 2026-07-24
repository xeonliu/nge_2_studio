import {
  ArrowLeft,
  ArrowRight,
  FolderOpen,
  Moon,
  Search,
  Sun,
} from "lucide-react";
import { appActions, useAppStore } from "../app/store";
import { ipc } from "../ipc/client";
import { IconButton } from "../shared/ui/IconButton";

export function TopBar() {
  const metadata = useAppStore((state) => state.isoMetadata);
  const selection = useAppStore((state) => state.selection);
  const theme = useAppStore((state) => state.theme);

  const openIso = async () => {
    try {
      const path = await ipc.pickIso();
      if (!path) return;
      appActions.startTask("正在读取 ISO 卷描述符");
      const result = await ipc.openIso(path);
      appActions.setSession(result.sessionId, result.metadata, result.root.items);
      appActions.finishTask(`已读取根目录，共 ${result.root.total} 项`);
    } catch (error) {
      appActions.failTask(error);
    }
  };

  return (
    <header className="topbar">
      <div className="brand-mark" aria-label="NGE2 ISO Studio">N2</div>
      <IconButton icon={FolderOpen} label="打开 ISO" onClick={openIso} />
      <div className="toolbar-divider" />
      <IconButton icon={ArrowLeft} label="后退" disabled />
      <IconButton icon={ArrowRight} label="前进" disabled />
      <div className="path-field" title={selection?.resource?.isoPath ?? metadata?.sourcePath ?? ""}>
        <span className="path-volume">{metadata?.volumeId || "NGE2 ISO Studio"}</span>
        <span className="path-separator">/</span>
        <span className="path-current">{selection?.label ?? "打开 ISO 开始检视"}</span>
      </div>
      <label className="search-field">
        <Search size={15} aria-hidden="true" />
        <input type="search" placeholder="筛选当前视图" aria-label="筛选当前视图" />
      </label>
      <IconButton
        icon={theme === "dark" ? Sun : Moon}
        label={`切换主题，当前：${theme}`}
        onClick={appActions.cycleTheme}
      />
    </header>
  );
}

