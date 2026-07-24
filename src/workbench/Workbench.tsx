import { appActions, useAppStore } from "../app/store";
import { ExplorerPane } from "../explorer/ExplorerPane";
import { useSplitter } from "../shared/hooks/useSplitter";
import { EditorTabs } from "./EditorTabs";
import { InspectorPane } from "./InspectorPane";
import { StatusBar } from "./StatusBar";
import { TopBar } from "./TopBar";

export function Workbench() {
  const explorerWidth = useAppStore((state) => state.explorerWidth);
  const inspectorWidth = useAppStore((state) => state.inspectorWidth);
  const error = useAppStore((state) => state.error);
  const explorerSplitter = useSplitter(
    (delta) => appActions.setExplorerWidth(explorerWidth + delta),
    1,
  );
  const inspectorSplitter = useSplitter(
    (delta) => appActions.setInspectorWidth(inspectorWidth + delta),
    -1,
  );

  return (
    <div
      className="workbench"
      style={{
        gridTemplateColumns: `${explorerWidth}px 4px minmax(0, 1fr) 4px ${inspectorWidth}px`,
      }}
    >
      <TopBar />
      <ExplorerPane />
      <div className="splitter" role="separator" aria-label="调整资源管理器宽度" {...explorerSplitter} />
      <EditorTabs />
      <div className="splitter" role="separator" aria-label="调整检查器宽度" {...inspectorSplitter} />
      <InspectorPane />
      <StatusBar />
      {error && (
        <div className="error-toast" role="alert">
          <span>{error}</span>
          <button type="button" onClick={appActions.dismissError} aria-label="关闭错误">×</button>
        </div>
      )}
    </div>
  );
}

