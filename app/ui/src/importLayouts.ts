// Folder layouts for card imports (Settings → When a card is inserted).

/** Folder layouts offered for card imports; any other is "Custom". */
export const LAYOUTS: { template: string; name: string }[] = [
  { template: "{yyyy}/{month}/{yyyy}-{mm}-{dd}/{orig}", name: "Year › Month › Day" },
  { template: "{yyyy}/{yyyy}-{mm}-{dd}/{orig}", name: "Year › Day" },
];

const MONTHS = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

/** Where `A7400123.ARW` taken on `d` would go, e.g. `2026\October\2026-10-30\A7400123.ARW`. */
export function layoutExample(template: string, d: Date, sep = "\\"): string {
  const two = (n: number) => String(n).padStart(2, "0");
  const v: Record<string, string> = {
    yyyy: String(d.getFullYear()),
    mm: two(d.getMonth() + 1),
    dd: two(d.getDate()),
    hh: two(d.getHours()),
    month: MONTHS[d.getMonth()],
    orig: "A7400123",
    event: "Event",
    camera: "ILCE-7M4",
    lens: "Lens",
    seq: "0001",
  };
  const path = template.trim().replace(/\{(\w+)\}/g, (_, k: string) => v[k] ?? `{${k}}`);
  return `${path.split(/[\\/]/).filter(Boolean).join(sep)}.ARW`;
}
