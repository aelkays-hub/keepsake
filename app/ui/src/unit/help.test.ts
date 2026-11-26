import { describe, expect, it } from "vitest";

import { HELP_TOPICS, SHORTCUTS, findTopics } from "../help";

describe("help", () => {
  it("lists every topic when nothing is typed", () => {
    expect(findTopics(HELP_TOPICS, "")).toEqual(HELP_TOPICS);
    expect(findTopics(HELP_TOPICS, "   ")).toEqual(HELP_TOPICS);
  });

  it("finds topics by every word typed, in the title or the steps, any case", () => {
    const ids = (q: string) => findTopics(HELP_TOPICS, q).map((t) => t.id);
    expect(ids("IMPORT card")).toContain("import-card");
    expect(ids("ctrl+t")).toEqual(["time-shift"]);
    expect(ids("flickr")).toEqual(["flickr"]);
    expect(ids("zzz nothing")).toEqual([]);
  });

  it("has unique topic ids and shortcut keys, and no empty topic", () => {
    expect(new Set(HELP_TOPICS.map((t) => t.id)).size).toBe(HELP_TOPICS.length);
    expect(new Set(SHORTCUTS.map((k) => k.keys)).size).toBe(SHORTCUTS.length);
    for (const t of HELP_TOPICS) expect(t.steps.length).toBeGreaterThan(0);
  });
});
