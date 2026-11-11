import { describe, expect, it } from "vitest";

import type { CloudInfo, OpenChoice } from "../api";
import { askChoice, askingLine, askLine, choiceFor } from "../editAdvice";
import { initial, reduce } from "../uiModel";

const PS = "C:\\Program Files\\Adobe\\Adobe Photoshop 2026\\Photoshop.exe";
const TOPAZ = "C:\\Program Files\\Topaz Labs LLC\\Topaz Photo\\Topaz Photo.exe";

const choices: OpenChoice[] = [
  { app: PS, name: "Photoshop", default: true },
  { app: TOPAZ, name: "Topaz Photo", default: false },
  { app: "", name: "System default", default: false },
];

const info = (keys: string[], def = "claude"): CloudInfo => ({
  defaultProvider: def,
  providers: [
    {
      id: "claude",
      name: "Claude",
      company: "Anthropic",
      hasKey: keys.includes("claude"),
      model: "claude-opus-5-5",
      models: [{ id: "claude-opus-5-5", name: "Claude Opus 5.5", centsPerPhoto: 2 }],
    },
    {
      id: "gemini",
      name: "Gemini",
      company: "Google",
      hasKey: keys.includes("gemini"),
      model: "gemini-3.6-flash",
      models: [{ id: "gemini-3.6-flash", name: "Gemini 3.6 Flash", centsPerPhoto: 0.8 }],
    },
  ],
});

describe("Edit suggestions", () => {
  it("finds Photoshop and Topaz Photo among the Open in choices", () => {
    expect(choiceFor(choices, "photoshop")?.app).toBe(PS);
    expect(choiceFor(choices, "topaz")?.app).toBe(TOPAZ);
    // By path, when the name is something else.
    expect(
      choiceFor([{ app: "D:\\Tools\\photoshop.exe", name: "PS", default: true }], "photoshop"),
    ).not.toBeNull();
    // Not set up: the system default does not count.
    expect(choiceFor(choices.slice(2), "topaz")).toBeNull();
  });

  it("says what asking sends and costs, and offers an AI with a key", () => {
    expect(askLine(info(["claude"]), "claude")).toBe(
      "Sends a 1024-pixel preview of this photo and what Keepsake measured to Claude, " +
        "by Anthropic (about 2¢ a photo). The original is not sent.",
    );
    expect(askLine(info([]), "nobody")).toBe("");
    expect(askChoice(info(["claude", "gemini"]))).toBe("claude");
    expect(askChoice(info(["gemini"]))).toBe("gemini");
    // "All" (Tag with AI) is not a choice here: the first with a key.
    expect(askChoice(info(["claude", "gemini"], "all"))).toBe("claude");
    expect(askChoice(info([]))).toBeNull();
  });

  it("says how the question is getting on", () => {
    const a = { provider: "claude", providerName: "Claude", pending: true, problem: null };
    expect(askingLine(a)).toBe("Asking Claude…");
    expect(askingLine({ ...a, problem: "Claude is busy" })).toBe(
      "Asking Claude… (Claude is busy; trying again)",
    );
    expect(
      askingLine({ ...a, pending: false, problem: "Claude declined to look at this photo" }),
    ).toBe("Claude could not answer: Claude declined to look at this photo.");
  });

  it("opens and closes the window for one photo", () => {
    const open = reduce(initial, { t: "editAdvice", photo: { photo: "a1", title: "DSC1.JPG" } });
    expect(open.editAdvice).toEqual({ photo: "a1", title: "DSC1.JPG" });
    expect(reduce(open, { t: "editAdvice", photo: null }).editAdvice).toBeNull();
  });
});
