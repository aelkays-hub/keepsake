// Edit suggestions (ADR-042): which Open in choice is Photoshop or Topaz
// Photo, what asking an AI costs, and what the window says while it asks.

import type { AskingDto, CloudInfo, OpenChoice } from "./api";
import { costPerPhoto } from "./cloud";

export type Program = "photoshop" | "topaz";

export const PROGRAM_NAME: Record<Program, string> = {
  photoshop: "Photoshop",
  topaz: "Topaz Photo",
};

/** The Open in choice that is `program` (by its name or path), if the
 * user has set it up in Settings for this file type. */
export function choiceFor(choices: OpenChoice[], program: Program): OpenChoice | null {
  const want = program === "photoshop" ? /photoshop/i : /topaz/i;
  return choices.find((c) => c.app !== "" && (want.test(c.name) || want.test(c.app))) ?? null;
}

/** What asking `providerId` sends and costs, in a sentence. */
export function askLine(info: CloudInfo, providerId: string): string {
  const p = info.providers.find((x) => x.id === providerId);
  if (!p) return "";
  const model = p.models.find((m) => m.id === p.model) ?? p.models[0];
  const cost = model ? ` (${costPerPhoto(model.centsPerPhoto)})` : "";
  return (
    `Sends a 1024-pixel preview of this photo and what Keepsake measured to ` +
    `${p.name}, by ${p.company}${cost}. The original is not sent.`
  );
}

/** The provider to offer: the default for Tag with AI if it has a key,
 * else the first that has one (`null`: none has). */
export function askChoice(info: CloudInfo): string | null {
  const ready = info.providers.filter((p) => p.hasKey);
  return (ready.find((p) => p.id === info.defaultProvider) ?? ready[0])?.id ?? null;
}

/** The line about a question on its way, or one that failed. */
export function askingLine(a: AskingDto): string {
  if (a.pending)
    return a.problem
      ? `Asking ${a.providerName}… (${a.problem}; trying again)`
      : `Asking ${a.providerName}…`;
  return `${a.providerName} could not answer: ${a.problem ?? "it failed"}.`;
}
