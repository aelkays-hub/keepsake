// Tag with Claude (ADR-036): what it costs, in words.

/** "about 2¢ a photo", "about $0.001 a photo". */
export function costPerPhoto(cents: number): string {
  return cents >= 1 ? `about ${cents}¢ a photo` : `about $${(cents / 100).toFixed(3)} a photo`;
}

/** "about $0.24" for `n` photos. */
export function costOf(n: number, cents: number): string {
  const usd = (n * cents) / 100;
  return usd < 0.01 ? "under 1¢" : `about $${usd.toFixed(2)}`;
}
