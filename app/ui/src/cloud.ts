// Tag with AI (ADR-036, ADR-038): what it costs, in words, and what each
// provider needs and does with the photos.

import type { CloudInfo, CloudProviderDto } from "./api";

/** "about 2¢ a photo", "about $0.001 a photo". */
export function costPerPhoto(cents: number): string {
  return cents >= 1 ? `about ${cents}¢ a photo` : `about $${(cents / 100).toFixed(3)} a photo`;
}

/** "about $0.24" for `n` photos. */
export function costOf(n: number, cents: number): string {
  const usd = (n * cents) / 100;
  return usd < 0.01 ? "under 1¢" : `about $${usd.toFixed(2)}`;
}

interface Help {
  /** Where to get a key. */
  site: string;
  /** What a key looks like, for the key box. */
  placeholder: string;
  /** What the provider does with the photos, when the user should know. */
  note: string | null;
}

const HELP: Record<string, Help> = {
  claude: { site: "console.anthropic.com", placeholder: "sk-ant-…", note: null },
  openai: {
    site: "platform.openai.com",
    placeholder: "sk-…",
    note: "OpenAI does not train its models on photos sent this way. It keeps them for up to 30 days to check for abuse.",
  },
  gemini: {
    site: "aistudio.google.com",
    placeholder: "Gemini API key",
    note:
      "Use a key from a Google Cloud project with billing turned on. With a free-tier key, " +
      "Google may use the photos you send, and people may review them, to improve its " +
      "products (unless you are in the European Economic Area, Switzerland or the UK).",
  },
};

export function help(providerId: string): Help {
  return HELP[providerId] ?? { site: "", placeholder: "", note: null };
}

/** The providers with a saved key. */
export function usable(info: CloudInfo): CloudProviderDto[] {
  return info.providers.filter((p) => p.hasKey);
}

/** The choice "All": every AI with a key tags the photos (ADR-039). */
export const ALL = "all";

/** The provider to offer first: the default if it has a key, else the
 * first that has one (`null`: none has). "All" when it was used last and
 * more than one AI has a key. */
export function firstChoice(info: CloudInfo): string | null {
  const ready = usable(info);
  if (info.defaultProvider === ALL && ready.length > 1) return ALL;
  return (ready.find((p) => p.id === info.defaultProvider) ?? ready[0])?.id ?? null;
}

/** The providers a choice sends to: one, or every one with a key. */
export function chosen(info: CloudInfo, picked: string | null): CloudProviderDto[] {
  const ready = usable(info);
  return picked === ALL ? ready : ready.filter((p) => p.id === picked);
}

/** "Claude", "Claude and Gemini", "Claude, ChatGPT and Gemini". */
export function names(list: string[]): string {
  return list.length <= 1
    ? (list[0] ?? "")
    : `${list.slice(0, -1).join(", ")} and ${list[list.length - 1]}`;
}
