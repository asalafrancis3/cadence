export const shortAddr = (a: string) => (a.length > 12 ? `${a.slice(0, 5)}…${a.slice(-5)}` : a);

export function formatAmount(raw: bigint, decimals: number): string {
  const neg = raw < 0n;
  const abs = neg ? -raw : raw;
  const base = 10n ** BigInt(decimals);
  const whole = abs / base;
  const frac = (abs % base).toString().padStart(decimals, "0").replace(/0+$/, "");
  return `${neg ? "-" : ""}${whole.toString()}${frac ? `.${frac}` : ""}`;
}

/** Exact decimal parsing with no floating point. Throws on malformed input. */
export function parseAmount(input: string, decimals: number): bigint {
  const s = input.trim();
  if (!/^\d+(\.\d+)?$/.test(s)) throw new Error("Enter a positive number.");
  const [whole, frac = ""] = s.split(".");
  if (frac.length > decimals) throw new Error(`At most ${decimals} decimal places.`);
  return BigInt(whole) * 10n ** BigInt(decimals) + BigInt(frac.padEnd(decimals, "0") || "0");
}

const UNITS: [string, number][] = [
  ["year", 365 * 86400],
  ["month", 30 * 86400],
  ["week", 7 * 86400],
  ["day", 86400],
  ["hour", 3600],
];

export function formatPeriod(seconds: bigint): string {
  const s = Number(seconds);
  for (const [name, size] of UNITS) {
    if (s % size === 0) {
      const n = s / size;
      return n === 1 ? `every ${name}` : `every ${n} ${name}s`;
    }
  }
  return `every ${Math.round(s / 3600)} hours`;
}

export const formatDate = (unixSeconds: bigint) =>
  new Date(Number(unixSeconds) * 1000).toLocaleString(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  });

export const PERIOD_PRESETS: { label: string; seconds: number }[] = [
  { label: "Hourly", seconds: 3600 },
  { label: "Daily", seconds: 86400 },
  { label: "Weekly", seconds: 7 * 86400 },
  { label: "Monthly (30 days)", seconds: 30 * 86400 },
  { label: "Yearly", seconds: 365 * 86400 },
];
