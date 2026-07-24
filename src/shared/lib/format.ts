export function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KiB", "MiB", "GiB"];
  let value = bytes / 1024;
  let unit = units[0];
  for (let index = 1; index < units.length && value >= 1024; index += 1) {
    value /= 1024;
    unit = units[index];
  }
  return `${value >= 10 ? value.toFixed(1) : value.toFixed(2)} ${unit}`;
}

export function formatHex(value: number, width = 2) {
  return `0x${value.toString(16).toUpperCase().padStart(width, "0")}`;
}

export function basename(path: string) {
  return path.split("/").filter(Boolean).at(-1) ?? path;
}

export function resourceLabel(resource: { isoPath: string; members: { name: string }[] }) {
  return resource.members.at(-1)?.name ?? basename(resource.isoPath);
}
