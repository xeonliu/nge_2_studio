import { useQuery } from "@tanstack/react-query";
import { EyeOff, ImageOff, Layers3 } from "lucide-react";
import { useState } from "react";
import type { DialogueFrame, ResourceRef, VisualReference } from "../../ipc/bindings";
import { ipc } from "../../ipc/client";
import { VariantPicker } from "./VariantPicker";

function exactResource(reference: VisualReference | null): ResourceRef | null {
  return reference?.resolution.status === "Exact" ? reference.resolution.value : null;
}

function PreviewImage({ resource, alt, className = "" }: { resource: ResourceRef; alt: string; className?: string }) {
  const query = useQuery({
    queryKey: ["image-preview", resource.isoPath, resource.members.map((member) => member.index).join("/")],
    queryFn: () => ipc.getImagePreview(resource),
  });
  if (query.isLoading) return <div className={`asset-loading ${className}`} />;
  if (!query.data) return <div className={`asset-missing ${className}`}><ImageOff size={22} /></div>;
  return <img className={className} src={query.data.url} alt={alt} draggable={false} />;
}

export function StoryboardStage({
  document,
  frame,
  visual,
  onVariantSelected,
}: {
  document: ResourceRef;
  frame: DialogueFrame | null;
  visual: VisualReference | null;
  onVariantSelected: () => void;
}) {
  const [composeLayers, setComposeLayers] = useState(false);
  const main = exactResource(visual);
  const portrait = frame?.portrait?.resolution.status === "Exact" ? frame.portrait.resolution.value : null;
  return (
    <section className="storyboard-stage" aria-label="分镜画面">
      <div className="stage-main">
        {main ? (
          <PreviewImage resource={main} alt={visual?.requested ?? "视觉资源"} className="stage-main-image" />
        ) : visual?.resolution.status === "Variant" ? (
          <VariantPicker document={document} reference={visual} onSelected={onVariantSelected} />
        ) : (
          <div className="stage-placeholder"><ImageOff size={30} /><span>{visual ? "关联资源缺失" : "当前尚无视觉命令"}</span></div>
        )}
        <div className="stage-overlay">
          <span className={`resolution-dot ${visual?.resolution.status.toLowerCase() ?? "none"}`} />
          <code>{visual?.requested ?? "NO VISUAL"}</code>
        </div>
        {composeLayers && <div className="approximate-label"><Layers3 size={13} />实验性图层合成 · 近似结果</div>}
      </div>
      <aside className="portrait-stage">
        <div className="portrait-label">STATIC PORTRAIT · _1.hpt</div>
        {portrait ? <PreviewImage resource={portrait} alt={frame?.speakerName ?? "头像"} className="portrait-image" /> : <div className="portrait-placeholder"><EyeOff size={24} /><span>{frame?.portrait?.runtimeHidden ? "运行时隐藏" : "无精确头像"}</span></div>}
        {frame?.portrait && <code>{frame.portrait.staticMember}</code>}
      </aside>
      <label className="layer-toggle" title="合成结果不是游戏运行时还原">
        <input type="checkbox" checked={composeLayers} onChange={(event) => setComposeLayers(event.target.checked)} />
        <span>近似合成</span>
      </label>
    </section>
  );
}

