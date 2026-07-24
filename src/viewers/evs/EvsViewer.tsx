import { useQuery } from "@tanstack/react-query";
import { AlertTriangle, LoaderCircle } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { OpenedResource } from "../../app/store";
import { appActions } from "../../app/store";
import { ipc } from "../../ipc/client";
import { SegmentedControl } from "../../shared/ui/SegmentedControl";
import { CommandTimeline } from "./CommandTimeline";
import { DialoguePanel } from "./DialoguePanel";
import { EvsStateInspector } from "./EvsStateInspector";
import { StoryboardStage } from "./StoryboardStage";

type ViewMode = "storyboard" | "raw";

export function EvsViewer({ tab }: { tab: OpenedResource }) {
  const [mode, setMode] = useState<ViewMode>("storyboard");
  const [selectedIndex, setSelectedIndex] = useState<number | null>(null);
  const document = useQuery({ queryKey: ["evs-document", tab.id], queryFn: () => ipc.openEvs(tab.resource) });
  const commands = useQuery({ queryKey: ["evs-commands", tab.id], queryFn: () => ipc.getEvsCommands(tab.resource) });
  const frames = useQuery({ queryKey: ["evs-frames", tab.id], queryFn: () => ipc.getEvsFrames(tab.resource) });
  const commandItems = useMemo(() => commands.data?.page.items ?? [], [commands.data]);
  const frameItems = useMemo(() => frames.data?.page.items ?? [], [frames.data]);

  useEffect(() => {
    if (selectedIndex !== null || commandItems.length === 0) return;
    const firstSay = commandItems.find((command) => command.opcode === 1) ?? commandItems[0];
    setSelectedIndex(firstSay.index);
  }, [commandItems, selectedIndex]);

  const selectedCommand = commandItems.find((command) => command.index === selectedIndex) ?? null;
  const selectedFrame = frameItems.find((frame) => frame.commandIndex === selectedIndex)
    ?? [...frameItems].reverse().find((frame) => frame.commandIndex <= (selectedIndex ?? -1))
    ?? null;
  const selectedVisual = frames.data?.visualReferences.find((visual) => visual.commandIndex === selectedIndex)
    ?? selectedFrame?.visuals.at(-1)
    ?? null;

  useEffect(() => {
    appActions.inspectCommand(selectedCommand, selectedFrame, selectedVisual);
  }, [selectedCommand, selectedFrame, selectedVisual]);

  if (document.isLoading || commands.isLoading || frames.isLoading) {
    return <div className="viewer-loading"><LoaderCircle className="spin" />按需解析 EVS 与关联资源</div>;
  }
  const error = document.error || commands.error || frames.error;
  if (error) return <div className="viewer-error">{String(error)}</div>;

  return (
    <div className="evs-viewer">
      <div className="evs-toolbar">
        <div className="evs-document-title"><strong>{tab.label}</strong><span>{document.data!.commandCount} commands</span><span>{document.data!.frameCount} frames</span></div>
        {document.data!.diagnosticCount > 0 && <span className="warning-count"><AlertTriangle size={13} />{document.data!.diagnosticCount}</span>}
        <span className="toolbar-spacer" />
        <SegmentedControl
          label="EVS 显示模式"
          value={mode}
          options={[{ value: "storyboard", label: "分镜" }, { value: "raw", label: "原始命令" }]}
          onChange={setMode}
        />
      </div>
      {mode === "storyboard" ? (
        <>
          <StoryboardStage document={tab.resource} frame={selectedFrame} visual={selectedVisual} onVariantSelected={() => void frames.refetch()} />
          <DialoguePanel frame={selectedFrame} />
        </>
      ) : (
        <EvsStateInspector command={selectedCommand} visual={selectedVisual} />
      )}
      <CommandTimeline
        commands={commandItems}
        selectedIndex={selectedIndex}
        onSelect={setSelectedIndex}
      />
    </div>
  );
}

