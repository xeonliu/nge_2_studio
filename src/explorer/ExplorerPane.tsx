import { appActions, useAppStore } from "../app/store";
import { SegmentedControl } from "../shared/ui/SegmentedControl";
import { EventBrowser } from "./EventBrowser";
import { IsoTree } from "./IsoTree";

export function ExplorerPane() {
  const mode = useAppStore((state) => state.explorerMode);
  return (
    <aside className="explorer-pane">
      <div className="pane-title"><span>资源管理器</span></div>
      <div className="explorer-switcher">
        <SegmentedControl
          label="资源管理器视图"
          value={mode}
          options={[{ value: "files", label: "Files" }, { value: "events", label: "Events" }]}
          onChange={appActions.setExplorerMode}
        />
      </div>
      {mode === "files" ? <IsoTree /> : <EventBrowser />}
    </aside>
  );
}

