import { describe, expect, it } from "vitest";

import aboutRs from "../../src-tauri/src/about.rs?raw";
import realNotices from "../../src-tauri/licenses/THIRD-PARTY-NOTICES.txt?raw";
import { CREDITS, findPackages, parseNotices } from "../licences";

const rule = `\n\n${"=".repeat(76)}\n\n`;
const sub = `\n\n${"-".repeat(76)}\n\n`;
const sample =
  [
    "THIRD-PARTY NOTICES FOR KEEPSAKE\n\nHeader.",
    "1. PROGRAMS SHIPPED BESIDE KEEPSAKE\n\nFFmpeg (separate)\n  Licence: LGPL.",
    "2. RUST CRATES BUILT INTO KEEPSAKE (2)\n\n" +
      "anyhow 1.0.0  —  MIT OR Apache-2.0  https://github.com/dtolnay/anyhow\n" +
      "    Licence text: [111111111111] [222222222222]\n" +
      "objc2 0.6.5  —  MIT  https://github.com/madsmtm/objc2\n" +
      "    Licence text: [333333333333] (standard text of its licence; the package carries no licence file)",
    "3. JAVASCRIPT PACKAGES IN KEEPSAKE'S WINDOW (1)\n\n" +
      "@tauri-apps/api 2.1.0  —  Apache-2.0 OR MIT\n    Licence text: [111111111111]",
    [
      "4. LICENCE TEXTS\n\nIntro.",
      "[111111111111] used by: anyhow 1.0.0, @tauri-apps/api 2.1.0\n\nApache License\nVersion 2.0",
      "[222222222222] used by: anyhow 1.0.0\n\nMIT License\n\nCopyright (c) dtolnay",
      "[333333333333] used by: objc2 0.6.5\n\nMIT License\n\nCopyright (c) the authors of objc2",
    ].join(sub),
  ].join(rule) + "\n";

describe("third-party notices", () => {
  it("reads the programs, the packages and their texts", () => {
    const n = parseNotices(sample);
    expect(n.programs).toBe("FFmpeg (separate)\n  Licence: LGPL.");
    expect(n.packages.map((p) => [p.kind, p.name, p.version, p.licence, p.repo])).toEqual([
      ["rust", "anyhow", "1.0.0", "MIT OR Apache-2.0", "https://github.com/dtolnay/anyhow"],
      ["rust", "objc2", "0.6.5", "MIT", "https://github.com/madsmtm/objc2"],
      ["js", "@tauri-apps/api", "2.1.0", "Apache-2.0 OR MIT", ""],
    ]);
    expect(n.packages[0].refs).toEqual(["111111111111", "222222222222"]);
    expect(n.packages[0].standard).toBe(false);
    expect(n.packages[1].standard).toBe(true);
    expect(n.texts.get("111111111111")).toBe("Apache License\nVersion 2.0");
    expect(n.texts.get("333333333333")).toContain("the authors of objc2");
    // Windows line endings read the same.
    expect(parseNotices(sample.replace(/\n/g, "\r\n")).packages).toHaveLength(3);
  });

  it("finds by name or licence, in any case", () => {
    const p = parseNotices(sample).packages;
    expect(findPackages(p, "OBJC").map((x) => x.name)).toEqual(["objc2"]);
    expect(findPackages(p, "apache").map((x) => x.name)).toEqual(["anyhow", "@tauri-apps/api"]);
    expect(findPackages(p, "  ")).toHaveLength(3);
    expect(findPackages(p, "gpl")).toEqual([]);
  });

  it("reads the real file: every package has its licence text", () => {
    const n = parseNotices(realNotices);
    const rust = n.packages.filter((p) => p.kind === "rust");
    const js = n.packages.filter((p) => p.kind === "js");
    // The counts the file states in its headings.
    expect(realNotices).toContain(`RUST CRATES BUILT INTO KEEPSAKE (${rust.length})`);
    expect(realNotices).toContain(`JAVASCRIPT PACKAGES IN KEEPSAKE'S WINDOW (${js.length})`);
    expect(rust.length).toBeGreaterThan(100);
    expect(js.map((p) => p.name)).toContain("react");
    expect(n.programs).toContain("ExifTool");
    for (const p of n.packages) {
      expect(p.refs.length, p.name).toBeGreaterThan(0);
      for (const r of p.refs) expect(n.texts.get(r), `${p.name} ${r}`).toBeTruthy();
    }
  });

  it("every credit's page is one the app agrees to open", () => {
    for (const c of CREDITS) expect(aboutRs).toContain(`"${c.url}"`);
  });
});
