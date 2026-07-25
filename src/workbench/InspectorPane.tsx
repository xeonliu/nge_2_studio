import { AlertCircle, CheckCircle2, CircleHelp, Copy, ExternalLink, TriangleAlert } from "lucide-react";
import { useAppStore } from "../app/store";
import { formatHex } from "../shared/lib/format";
import { IconButton } from "../shared/ui/IconButton";
import type { Resolution } from "../ipc/bindings";

function resolutionStatus(resolution: Resolution | undefined) {
  if (!resolution) return null;
  if (resolution.status === "Exact") return { label: "Exact", className: "exact", icon: CheckCircle2 };
  if (resolution.status === "Variant") return { label: `${resolution.value.length} 个 Variant`, className: "variant", icon: TriangleAlert };
  if (resolution.status === "Missing") return { label: "Missing", className: "missing", icon: AlertCircle };
  return { label: "Unsupported", className: "unsupported", icon: CircleHelp };
}

export function InspectorPane() {
  const selection = useAppStore((state) => state.selection);
  const command = useAppStore((state) => state.selectedCommand);
  const frame = useAppStore((state) => state.selectedFrame);
  const visual = useAppStore((state) => state.selectedVisual);
  const status = resolutionStatus(visual?.resolution);

  return (
    <aside className="inspector-pane">
      <div className="pane-title">
        <span>检查器</span>
        <div className="pane-actions">
          <IconButton icon={Copy} label="复制属性" disabled={!selection} />
          <IconButton icon={ExternalLink} label="跳转到关联资源" disabled={!visual} />
        </div>
      </div>
      {!selection && !command ? (
        <div className="pane-empty">选择资源或时间线命令以查看详细信息</div>
      ) : (
        <div className="inspector-scroll">
          {command && (
            <section className="property-section">
              <h2>命令</h2>
              <dl className="property-grid">
                <dt>索引</dt><dd className="mono">#{command.index}</dd>
                <dt>Opcode</dt><dd className="mono">{command.opcodeHex}</dd>
                <dt>Offset</dt><dd className="mono">{formatHex(command.offset, 8)}</dd>
                <dt>类型</dt><dd>{command.name}</dd>
                <dt>类别</dt><dd><span className={`command-category ${command.category}`}>{command.category}</span></dd>
                <dt>语义</dt><dd>{command.description}</dd>
              </dl>
              {command.parameters.length > 0 && (
                <div className="parameter-list">
                  {command.parameters.map((parameter, index) => (
                    <div key={index}><span>{command.parameterNames[index] ?? `P${index}`}</span><code>{parameter} / {formatHex(parameter, 8)}</code></div>
                  ))}
                </div>
              )}
              {command.options.length > 0 && <ol className="command-options inspector-options">{command.options.map((option, index) => <li key={`${index}-${option}`}>{option}</li>)}</ol>}
            </section>
          )}
          {visual && (
            <section className="property-section">
              <h2>资源解析</h2>
              {status && (
                <div className={`resolution-badge ${status.className}`}>
                  <status.icon size={14} /> {status.label}
                </div>
              )}
              <dl className="property-grid">
                <dt>请求</dt><dd className="mono wrap">{visual.requested}</dd>
                <dt>依据</dt><dd>{visual.evidence}</dd>
              </dl>
            </section>
          )}
          {frame?.portrait && (
            <section className="property-section">
              <h2>头像</h2>
              <dl className="property-grid">
                <dt>归档</dt><dd className="mono wrap">{frame.portrait.archivePath}</dd>
                <dt>静态图</dt><dd className="mono">{frame.portrait.staticMember}</dd>
                <dt>动画 Atlas</dt><dd className="mono">{frame.portrait.atlasMember}</dd>
                <dt>运行时</dt><dd>{frame.portrait.runtimeHidden ? "隐藏" : "显示"}</dd>
              </dl>
            </section>
          )}
          {selection && (
            <section className="property-section">
              <h2>资源</h2>
              <div className="resource-heading">{selection.label}</div>
              <dl className="property-grid">
                {Object.entries(selection.metadata).map(([key, value]) => (
                  <span className="property-pair" key={key}><dt>{key}</dt><dd>{String(value ?? "--")}</dd></span>
                ))}
              </dl>
              {selection.resource && <code className="resource-path">{selection.resource.isoPath}</code>}
            </section>
          )}
          {(command?.diagnostics ?? []).map((diagnostic, index) => (
            <div className={`diagnostic ${diagnostic.severity}`} key={`${diagnostic.offset}-${index}`}>
              <AlertCircle size={14} />
              <span>{diagnostic.message}</span>
            </div>
          ))}
        </div>
      )}
    </aside>
  );
}
