import type { OpenedResource } from "../app/store";
import { HgarViewer } from "./archive/HgarViewer";
import { EvsViewer } from "./evs/EvsViewer";
import { HexViewer } from "./hex/HexViewer";
import { ImageViewer } from "./image/ImageViewer";
import { MetadataViewer } from "./metadata/MetadataViewer";

export function ViewerRegistry({ tab }: { tab: OpenedResource }) {
  switch (tab.kind) {
    case "evs": return <EvsViewer tab={tab} />;
    case "hgar": return <HgarViewer tab={tab} />;
    case "image": return <ImageViewer tab={tab} />;
    case "metadata": return <MetadataViewer tab={tab} />;
    default: return <HexViewer tab={tab} />;
  }
}

