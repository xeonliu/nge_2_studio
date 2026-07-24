import { EyeOff, MessageSquareText, Volume2, VolumeX } from "lucide-react";
import type { DialogueFrame } from "../../ipc/bindings";

export function DialoguePanel({ frame }: { frame: DialogueFrame | null }) {
  if (!frame) return <section className="dialogue-panel empty"><MessageSquareText size={20} />选择 SAY 命令查看台词</section>;
  return (
    <section className="dialogue-panel">
      <div className="speaker-block">
        <strong>{frame.speakerName}</strong>
        <span>{frame.expressionName}</span>
        {frame.portrait?.runtimeHidden && <em><EyeOff size={12} />NO_AVATAR</em>}
      </div>
      <div className="dialogue-text">
        {frame.pages.map((page, index) => (
          <span key={index}>{page}{index < frame.pages.length - 1 && <i className="page-break">▽</i>}</span>
        ))}
      </div>
      <div className="dialogue-stats">
        <span>{frame.audioId === null ? <VolumeX size={13} /> : <Volume2 size={13} />}{frame.audioId === null ? "NO AUDIO" : `Audio ${frame.audioId}`}</span>
        <span>{frame.textBytes} bytes</span>
        <span>{frame.pages.length} page{frame.pages.length > 1 ? "s" : ""}</span>
      </div>
    </section>
  );
}

