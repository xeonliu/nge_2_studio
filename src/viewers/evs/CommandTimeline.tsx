import { AlertCircle, Clock3, Flag, GitBranch, Image, ListChecks, MessageSquare, Music, Puzzle, TerminalSquare } from "lucide-react";
import { useMemo, useState } from "react";
import type { EvsCommand } from "../../ipc/bindings";
import { formatHex } from "../../shared/lib/format";

const ROW_HEIGHT = 36;
const OVERSCAN = 8;

function commandClass(command: EvsCommand) {
  return command.supported ? command.category : "unknown";
}

function CommandIcon({ command }: { command: EvsCommand }) {
  const type = commandClass(command);
  if (type === "dialogue") return <MessageSquare size={14} />;
  if (type === "flow") return <GitBranch size={14} />;
  if (type === "visual") return <Image size={14} />;
  if (type === "audio") return <Music size={14} />;
  if (type === "choice") return <ListChecks size={14} />;
  if (type === "timing") return <Clock3 size={14} />;
  if (type === "event") return <Flag size={14} />;
  if (type === "extension") return <Puzzle size={14} />;
  if (type === "unknown") return <AlertCircle size={14} />;
  return <TerminalSquare size={14} />;
}

function commandSummary(command: EvsCommand) {
  if (command.options.length) return command.options.join(" / ");
  if (command.content) return command.content;
  return command.parameters
    .map((value, index) => `${command.parameterNames[index] ?? `P${index}`}=${formatHex(value, 4)}`)
    .join("  ");
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
      <div className="timeline-header"><strong>Command Timeline</strong><span>{commands.length} entries</span><span className="timeline-legend"><i className="dialogue" />DIALOGUE<i className="visual" />VISUAL<i className="audio" />AUDIO<i className="choice" />CHOICE<i className="unknown" />UNKNOWN</span></div>
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
                <span className="timeline-summary">{commandSummary(command)}</span>
                <code className="timeline-offset">@{command.offset.toString(16).toUpperCase().padStart(8, "0")}</code>
              </button>
            );
          })}
        </div>
      </div>
    </section>
  );
}
