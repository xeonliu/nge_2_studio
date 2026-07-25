import { Braces, CircleHelp } from "lucide-react";
import type { EvsCommand, VisualReference } from "../../ipc/bindings";

export function EvsStateInspector({ command, visual }: { command: EvsCommand | null; visual: VisualReference | null }) {
  if (!command) return <div className="raw-command-empty"><CircleHelp size={24} />选择一条命令</div>;
  return (
    <section className="raw-command-view">
      <header><Braces size={17} /><strong>{command.name}</strong><span>#{command.index}</span><code>{command.opcodeHex}</code></header>
      <div className="raw-columns">
        <div>
          <h3>语义</h3>
          <span className={`command-category ${command.category}`}>{command.category}</span>
          <p className="command-description">{command.description}</p>
          {command.options.length > 0 && <ol className="command-options">{command.options.map((option, index) => <li key={`${index}-${option}`}>{option}</li>)}</ol>}
        </div>
        <div><h3>参数</h3>{command.parameters.length ? command.parameters.map((value, index) => <code key={index}>{command.parameterNames[index] ?? `P${index}`}: {value} / 0x{value.toString(16).toUpperCase()}</code>) : <span>无参数</span>}</div>
        <div><h3>Payload 与解析</h3><code className="raw-payload">{command.rawPayload.map((value) => value.toString(16).padStart(2, "0")).join(" ") || "--"}</code><span>{command.supported ? "Supported" : "Unsupported"}</span>{visual && <><span>{visual.resolution.status}</span><small>{visual.evidence}</small></>}</div>
      </div>
    </section>
  );
}
