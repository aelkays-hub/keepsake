import { describe, expect, it } from "vitest";

import type { CloudInfo } from "../api";
import { ALL, chosen, firstChoice, names } from "../cloud";
import { suggestTags, tagName } from "../tagSearch";

const tags = [
  { value: "Beach", count: 40 },
  { value: "Places|Lake", count: 12 },
  { value: "AI|Sand Castle", count: 3 },
  { value: "Seaside", count: 50 },
  { value: "Dog", count: 7 },
];

describe("tag suggestions", () => {
  it("finds tags by any part of the name, those starting with it first", () => {
    expect(suggestTags(tags, "sea", []).map((t) => t.value)).toEqual(["Seaside"]);
    // "a" is in several: names starting with it first, then the most used.
    expect(suggestTags(tags, "A", []).map((t) => t.value)).toEqual([
      "AI|Sand Castle",
      "Seaside",
      "Beach",
      "Places|Lake",
    ]);
    expect(suggestTags(tags, "lake", []).map((t) => t.value)).toEqual(["Places|Lake"]);
  });

  it("leaves out chosen tags and suggests nothing for blank words", () => {
    expect(suggestTags(tags, "beach", ["Beach"])).toEqual([]);
    expect(suggestTags(tags, "  ", [])).toEqual([]);
  });

  it("shows paths as people read them", () => {
    expect(tagName("Places|Lake")).toBe("Places › Lake");
  });
});

const info = (def: string, keys: string[]): CloudInfo => ({
  defaultProvider: def,
  providers: [
    ["claude", "Claude"],
    ["openai", "ChatGPT"],
    ["gemini", "Gemini"],
  ].map(([id, name]) => ({
    id,
    name,
    company: "",
    hasKey: keys.includes(id),
    model: "m",
    models: [{ id: "m", name: "M", centsPerPhoto: 1 }],
  })),
});

describe("Tag with AI: All", () => {
  it("sends to every AI with a key", () => {
    expect(chosen(info("claude", ["claude", "gemini"]), ALL).map((p) => p.id)).toEqual([
      "claude",
      "gemini",
    ]);
    expect(chosen(info("claude", ["claude", "gemini"]), "gemini").map((p) => p.id)).toEqual([
      "gemini",
    ]);
  });

  it("offers All first when it was used last and more than one AI has a key", () => {
    expect(firstChoice(info(ALL, ["claude", "openai"]))).toBe(ALL);
    expect(firstChoice(info(ALL, ["openai"]))).toBe("openai");
  });

  it("names the AIs", () => {
    expect(names(["Claude"])).toBe("Claude");
    expect(names(["Claude", "Gemini"])).toBe("Claude and Gemini");
    expect(names(["Claude", "ChatGPT", "Gemini"])).toBe("Claude, ChatGPT and Gemini");
  });
});
