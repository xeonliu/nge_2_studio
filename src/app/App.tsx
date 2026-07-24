import { useEffect, useRef } from "react";
import { Workbench } from "../workbench/Workbench";
import { appActions, useAppStore } from "./store";
import { ipc, isTauriRuntime } from "../ipc/client";

export function App() {
  const theme = useAppStore((state) => state.theme);
  const booted = useRef(false);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
  }, [theme]);

  useEffect(() => {
    if (booted.current || isTauriRuntime()) return;
    booted.current = true;
    appActions.startTask("载入演示 ISO");
    ipc.openIso("__browser_demo__")
      .then((result) => {
        appActions.setSession(result.sessionId, result.metadata, result.root.items);
        const resource = ipc.demoEvsResource(result.sessionId);
        appActions.open({
          label: "a000.evs",
          kind: "evs",
          resource,
          metadata: { 类型: "EVS 脚本", 所属归档: "a000.har", 大小: 3892 },
        });
        appActions.finishTask("浏览器演示数据已就绪");
      })
      .catch(appActions.failTask);
  }, []);

  return <Workbench />;
}

