import { FileQuestion, X } from "lucide-react";
import { appActions, useAppStore } from "../app/store";
import { ViewerRegistry } from "../viewers/registry";

export function EditorTabs() {
  const tabs = useAppStore((state) => state.tabs);
  const activeTabId = useAppStore((state) => state.activeTabId);
  const active = tabs.find((tab) => tab.id === activeTabId);

  return (
    <main className="editor-area">
      <div className="editor-tabs" role="tablist" aria-label="打开的资源">
        {tabs.map((tab) => (
          <div className={`editor-tab ${tab.id === activeTabId ? "active" : ""}`} role="tab" aria-selected={tab.id === activeTabId} key={tab.id}>
            <button className="tab-label" type="button" onClick={() => appActions.activateTab(tab.id)}>{tab.label}</button>
            <button className="tab-close" type="button" title="关闭" aria-label={`关闭 ${tab.label}`} onClick={() => appActions.closeTab(tab.id)}>
              <X size={13} />
            </button>
          </div>
        ))}
      </div>
      <div className="editor-content">
        {active ? (
          <ViewerRegistry tab={active} />
        ) : (
          <div className="empty-editor">
            <FileQuestion size={34} strokeWidth={1.2} />
            <strong>没有打开的资源</strong>
            <span>在左侧双击文件或 HGAR 成员</span>
          </div>
        )}
      </div>
    </main>
  );
}

