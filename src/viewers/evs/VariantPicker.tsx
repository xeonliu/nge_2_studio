import { useQuery } from "@tanstack/react-query";
import { ImageOff } from "lucide-react";
import type { ResourceRef, VisualReference } from "../../ipc/bindings";
import { ipc } from "../../ipc/client";
import { resourceLabel } from "../../shared/lib/format";

function VariantThumbnail({ candidate, onClick }: { candidate: ResourceRef; onClick: () => void }) {
  const query = useQuery({
    queryKey: ["variant", candidate.isoPath, candidate.members.map((member) => member.index).join("/")],
    queryFn: () => ipc.getImagePreview(candidate),
    retry: false,
  });
  return (
    <button type="button" className="variant-option" onClick={onClick} title={resourceLabel(candidate)}>
      {query.data ? <img src={query.data.url} alt="" /> : <span><ImageOff size={18} /></span>}
      <code>{resourceLabel(candidate)}</code>
    </button>
  );
}

export function VariantPicker({ document, reference, onSelected }: { document: ResourceRef; reference: VisualReference; onSelected: () => void }) {
  const candidates = reference.resolution.status === "Variant" ? reference.resolution.value : [];
  const choose = async (candidate: ResourceRef) => {
    await ipc.selectEvsVariant(document, reference.commandIndex, candidate);
    onSelected();
  };
  return (
    <div className="variant-picker">
      <div className="variant-title"><strong>选择变量资源</strong><span>{reference.requested}</span></div>
      <div className="variant-grid">
        {candidates.map((candidate) => <VariantThumbnail candidate={candidate} onClick={() => void choose(candidate)} key={candidate.members.map((member) => member.index).join("/")} />)}
      </div>
    </div>
  );
}

