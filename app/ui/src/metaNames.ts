// Names for people in "All metadata": ExifTool's groups and tag names.

/** Group names for people; others are shown as ExifTool names them. */
const GROUP_NAMES: Record<string, string> = {
  IFD0: "Camera and image (IFD0)",
  ExifIFD: "Exposure (EXIF)",
  GPS: "GPS",
  Composite: "Computed by ExifTool",
  IFD1: "Embedded thumbnail (IFD1)",
  File: "File",
  System: "System",
  InteropIFD: "Interoperability",
};

export function groupName(g: string): string {
  if (GROUP_NAMES[g]) return GROUP_NAMES[g];
  if (g.startsWith("XMP-")) return `XMP (${g.slice(4)})`;
  // Other standards and the file's inner structure: as ExifTool names them.
  if (/^(IPTC|ICC|SubIFD|SR2|PrintIM|MPF|JFIF|APP\d)/.test(g)) return g;
  // The camera's own notes: Sony, Canon, Nikon…
  return `${g} maker notes`;
}

/** `ExposureTime` → `Exposure Time`, `ISOSetting` → `ISO Setting`. */
export function tagName(n: string): string {
  return n.replace(/([a-z0-9])([A-Z])/g, "$1 $2").replace(/([A-Z]+)([A-Z][a-z])/g, "$1 $2");
}
