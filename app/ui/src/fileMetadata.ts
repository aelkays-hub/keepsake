// Reading every metadata tag of an item's file (ExifTool), for the
// inspector's "All metadata" and the viewer's information panel.

import { useEffect, useState } from "react";

import { api, type FileMetadata, type MemberDto } from "./api";

/** Every tag of an item's file, read by ExifTool. Nothing is read while
 * `enabled` is false or the drive is away. */
export function useFileMetadata(id: string, member: MemberDto, enabled: boolean, online: boolean) {
  const [data, setData] = useState<FileMetadata | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  useEffect(() => {
    setData(null);
    setProblem(null);
    if (!enabled || !online) return;
    let live = true;
    api
      .metadataAll(id, member)
      .then((m) => live && setData(m))
      .catch((e) => live && setProblem(String(e)));
    return () => {
      live = false;
    };
  }, [id, member, enabled, online]);
  return { data, problem };
}
