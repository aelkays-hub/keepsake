// Colour labels (names match the XMP/Lightroom labels).

export const LABELS: { name: string; color: string; key: string }[] = [
  { name: "Red", color: "#e5484d", key: "6" },
  { name: "Yellow", color: "#f5d90a", key: "7" },
  { name: "Green", color: "#30a46c", key: "8" },
  { name: "Blue", color: "#0090ff", key: "9" },
  { name: "Purple", color: "#8e4ec6", key: "" },
];

export function labelColor(name: string | null | undefined): string | undefined {
  return LABELS.find((l) => l.name.toLowerCase() === name?.toLowerCase())?.color;
}
