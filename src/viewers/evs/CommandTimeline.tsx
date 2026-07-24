import { AlertCircle, GitBranch, Image, MessageSquare, Music, TerminalSquare } from "lucide-react";
import { useMemo, useState } from "react";
import type { EvsCommand } from "../../ipc/bindings";
import { formatHex } from "../../shared/lib/format";

const ROW_HEIGHT = 36;
const OVERSCAN = 8;

function commandClass(command: EvsCommand) {
  if (command.opcode === 1) return "say";
  if ([0x8c, 0x8d, 0x8e].includes(command.opcode)) return "visual";
  if (command.opcode === 0x95) return "audio";
  if (command.name.includes("CONTROL")) return "control";
  if (!command.supported) return "unknown";
  return "command";
}

function CommandIcon({ command }: { command: EvsCommand }) {
  const type = commandClass(command);
  if (type === "say") return <MessageSquare size={14} />;
  if (type === "visual") return <Image size={14} />;
  if (type === "audio") return <Music size={14} />;
  if (type === "control") return <GitBranch size={14} />;
  if (type === "unknown") return <AlertCircle size={14} />;
  return <TerminalSquare size={14} />;
}

export function CommandTimeline({ commands, selectedIndex, onSelect }: { commands: EvsCommand[]; selectedIndex: number | null; onSelect: (index: number) => void }) {
  const [scrollTop, setScrollTop] = useState(0);
  const viewportHeight = 250;
  const range = useMemo(() => {
    const start = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN);
    const end = Math.min(commands.length, Math.ceil((scrollTop + viewportHeight) / ROW_HEIGHT) + OVERSCAN);
    return { start, end };
  }, [commands.length, scrollTop]);
  return (
    <section className="command-timeline">
      <div className="timeline-header"><strong>Command Timeline</strong><span>{commands.length} entries</span><span className="timeline-legend"><i className="say" />SAY<i className="visual" />VISUAL<i className="audio" />AUDIO<i className="unknown" />UNKNOWN</span></div>
      <div className="timeline-scroll" style={{ height: viewportHeight }} onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}>
        <div className="timeline-spacer" style={{ height: commands.length * ROW_HEIGHT }}>
          {commands.slice(range.start, range.end).map((command, localIndex) => {
            const index = range.start + localIndex;
            return (
              <button
                type="button"
                className={`timeline-row ${commandClass(command)} ${selectedIndex === command.index ? "selected" : ""}`}
                style={{ transform: `translateY(${index * ROW_HEIGHT}px)` }}
                onClick={() => onSelect(command.index)}
                key={command.index}
              >
                <span className="timeline-index">{String(command.index).padStart(4, "0")}</span>
                <span className="timeline-opcode"><CommandIcon command={command} /><code>{command.opcodeHex}</code></span>
                <strong>{command.name}</strong>
                <span className="timeline-summary">{command.content ?? command.parameters.map((value) => formatHex(value, 4)).join("  ")}</span>
                <code className="timeline-offset">@{command.offset.toString(16).toUpperCase().padStart(8, "0")}</code>
              </button>
            );
          })}
        </div>
      </div>
    </section>
  );
}

