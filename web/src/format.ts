/**
 * Display units. Everything arriving from the core is SI (m, s, m/s); the viewer converts only
 * for display, here. Parameter values stay in their register unit.
 */
export const KMH_PER_MS = 3.6;
export const VEH_H_PER_VEH_S = 3600;
export const VEH_KM_PER_VEH_M = 1000;

export function fmt(x: number | null | undefined, digits = 1): string {
  if (x === null || x === undefined || !Number.isFinite(x)) return "—";
  const a = Math.abs(x);
  if (a >= 10000) return x.toLocaleString("en", { maximumFractionDigits: 0 });
  return x.toLocaleString("en", { minimumFractionDigits: digits, maximumFractionDigits: digits });
}

/** Simulated time as h:mm:ss. */
export function clock(t: number): string {
  if (!Number.isFinite(t)) return "—";
  const s = Math.floor(t);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const tenth = Math.floor((t - s) * 10);
  return `${h}:${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}.${tenth}`;
}

/** Full-precision number, as stored (for the inspector). */
export function exact(x: number | null | undefined): string {
  if (x === null || x === undefined) return "null";
  return String(x);
}
