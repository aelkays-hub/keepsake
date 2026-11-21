import { describe, expect, it } from "vitest";

import {
  addTags,
  cleanTag,
  dedupe,
  DEFAULT_PRIVACY,
  finalTags,
  isMachineTag,
  MAX_TAGS,
  noEdits,
  parseTags,
  PRIVACY,
  privacyLabel,
  removeTag,
} from "../flickr";

describe("Upload to Flickr: privacy", () => {
  it("offers public, friends and family, friends, family and private; private first chosen", () => {
    expect(PRIVACY.map((p) => p.id)).toEqual([
      "public",
      "friends_family",
      "friends",
      "family",
      "private",
    ]);
    expect(DEFAULT_PRIVACY).toBe("private");
    expect(privacyLabel("friends_family")).toBe("Friends and family");
    expect(privacyLabel("odd")).toBe("odd");
  });
});

describe("Upload to Flickr: tags", () => {
  it("typed tags are separated by commas and cleaned", () => {
    expect(parseTags(' Air  Canada , C-FEJC,, "YYZ" ')).toEqual(["Air Canada", "C-FEJC", "YYZ"]);
    expect(cleanTag('  a "b"  c ')).toBe("a b c");
  });

  it("each tag is kept once, at most 75", () => {
    expect(dedupe(["Sony", "sony", "SONY ", ""])).toEqual(["Sony"]);
    expect(dedupe(Array.from({ length: 100 }, (_, i) => `t${i}`))).toHaveLength(MAX_TAGS);
  });

  it("removing and adding adjust the suggested tags", () => {
    const suggested = ["C-FEJC", "Sony", "Sony ILCE-1"];
    let e = removeTag(noEdits, "Sony");
    expect(finalTags(suggested, e)).toEqual(["C-FEJC", "Sony ILCE-1"]);
    e = addTags(e, ["Air Canada", "sony"]);
    expect(finalTags(suggested, e)).toEqual(["C-FEJC", "Sony", "Sony ILCE-1", "Air Canada"]);
    e = removeTag(e, "air canada");
    expect(finalTags(suggested, e)).toEqual(["C-FEJC", "Sony", "Sony ILCE-1"]);
  });

  it("machine tags are told apart", () => {
    expect(isMachineTag("aircraft:registration=C-FEJC")).toBe(true);
    expect(isMachineTag("aircraft:from_country=Canada")).toBe(true);
    expect(isMachineTag("C-FEJC")).toBe(false);
    expect(isMachineTag("12:30=x")).toBe(false);
  });
});
