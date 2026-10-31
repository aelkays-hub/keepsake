// The viewer's information panel: the key facts of the item shown (when,
// camera, lens, exposure, size, place), then every tag ExifTool reads from
// its file (the same list as the inspector's "All metadata"). For a RAW +
// JPEG pair it follows the viewer's JPEG | RAW choice. An item on an
// unplugged drive shows what the catalog knows.

import type { Details, FileMetadata, MemberDto } from "../api";
import * as fmt from "../format";
import { useFileMetadata } from "../fileMetadata";
import { MetadataTags } from "./AllMetadata";

/** The first of `names` found in any group, as ExifTool printed it. */
function tag(data: FileMetadata | null, ...names: string[]): string | null {
  if (!data) return null;
  for (const n of names)
    for (const g of data.groups) {
      const t = g.tags.find((x) => x.name === n);
      if (t && t.value.trim()) return t.value.trim();
    }
  return null;
}

/** `2024-06-15T14:02:11…` → `2024-06-15 14:02:11`. */
const when = (s: string | null) => (s ? s.slice(0, 19).replace("T", " ") : null);

function dimensions(data: FileMetadata | null): string | null {
  const size = tag(data, "ImageSize");
  if (size) return size.replace(/\s*x\s*/i, " × ");
  const w = tag(data, "ExifImageWidth", "ImageWidth", "SourceImageWidth");
  const h = tag(data, "ExifImageHeight", "ImageHeight", "SourceImageHeight");
  return w && h ? `${w} × ${h}` : null;
}

function exposure(data: FileMetadata | null): string | null {
  const t = tag(data, "ExposureTime", "ShutterSpeed");
  const f = tag(data, "FNumber", "Aperture");
  const parts = [t && `${t} s`, f && `f/${f}`].filter(Boolean);
  return parts.length ? parts.join(" · ") : null;
}

export function ViewerInfo({
  d,
  member,
  hasPair,
}: {
  d: Details;
  member: MemberDto;
  hasPair: boolean;
}) {
  const shown: MemberDto = hasPair ? member : "primary";
  const { data, problem } = useFileMetadata(d.id, shown, true, d.online);
  const file =
    shown === "raw"
      ? d.files.find((f) => f.role === "raw_pair")
      : (d.files.find((f) => f.role !== "raw_pair") ?? d.files[0]);
  const video = d.kind === "video";
  const gps = d.gps
    ? `${d.gps.lat.toFixed(5)}, ${d.gps.lon.toFixed(5)}`
    : tag(data, "GPSPosition", "GPSCoordinates");
  const facts: [string, string | null][] = [
    ["Date and time", when(d.captured)],
    ["Camera", [d.cameraMake, d.cameraModel].filter(Boolean).join(" ") || tag(data, "Model")],
    ["Lens", d.lens ?? tag(data, "LensModel", "LensID", "Lens")],
    ["Exposure", video ? null : exposure(data)],
    ["ISO", video ? null : tag(data, "ISO")],
    ["Focal length", tag(data, "FocalLength35efl", "FocalLength")],
    ["Duration", video ? fmt.duration(d.durationMs) : null],
    ["Frame rate", video ? tag(data, "VideoFrameRate") : null],
    ["Dimensions", dimensions(data)],
    ["File size", file ? fmt.bytes(file.size) : tag(data, "FileSize")],
    ["Location", gps],
  ];
  return (
    <aside className="viewer-info" aria-label="Information" data-testid="viewer-info">
      <h2>Information</h2>
      {hasPair && (
        <p className="muted" data-testid="viewer-info-member">
          {shown === "raw" ? "The RAW file" : "The JPEG file"} (switch with R)
        </p>
      )}
      <dl className="meta" data-testid="viewer-facts">
        {facts
          .filter(([, v]) => v)
          .map(([k, v]) => (
            <div key={k} className="meta-row">
              <dt>{k}</dt>
              <dd>{v}</dd>
            </div>
          ))}
      </dl>
      <h2>All metadata</h2>
      {d.online ? (
        <div className="all-meta">
          <MetadataTags data={data} problem={problem} />
        </div>
      ) : (
        <p className="muted" role="status">
          The drive with this item is not connected: showing what the catalog knows. Connect it to
          read every tag.
        </p>
      )}
    </aside>
  );
}
