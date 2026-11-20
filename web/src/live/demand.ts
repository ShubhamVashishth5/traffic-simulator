/**
 * Live demand (sandbox track S1): the arithmetic behind the Traffic panel. The server takes
 * type shares per entry (`set_demand`); people think in classes ("more buses"), so the panel
 * edits class shares and these helpers turn them back into type shares.
 */

export type Shares = Record<string, number>;

/** Readable names for the register's vehicle classes, in display order. */
export const CLASS_LABELS: [string, string][] = [
  ["two_wheeler", "Two-wheelers"],
  ["car", "Cars"],
  ["auto_rickshaw", "Auto-rickshaws"],
  ["e_rickshaw", "E-rickshaws"],
  ["bus", "Buses"],
  ["lcv", "Light goods vehicles"],
  ["truck", "Trucks"],
  ["bicycle", "Bicycles"],
  ["cycle_rickshaw", "Cycle-rickshaws"],
  ["tractor", "Tractors"],
];

/** Share of each class in a type composition (normalised). */
export function classShares(composition: Shares, classOf: Record<string, string>): Shares {
  const total = Object.values(composition).reduce((a, w) => a + w, 0);
  const out: Shares = {};
  if (total <= 0) return out;
  for (const [t, w] of Object.entries(composition)) {
    const c = classOf[t];
    if (c === undefined || w <= 0) continue;
    out[c] = (out[c] ?? 0) + w / total;
  }
  return out;
}

/**
 * Set class `cls` to `share` (0–1) and scale the other classes so the shares still sum to 1.
 * If every other class is at 0, the result is that class alone.
 */
export function setClassShare(shares: Shares, cls: string, share: number): Shares {
  const s = Math.min(1, Math.max(0, share));
  const othersTotal = Object.entries(shares).reduce((a, [c, w]) => (c === cls ? a : a + w), 0);
  const out: Shares = {};
  for (const [c, w] of Object.entries(shares)) {
    if (c !== cls && w > 0 && othersTotal > 0) out[c] = (w / othersTotal) * (1 - s);
  }
  if (s > 0 || Object.keys(out).length === 0) out[cls] = othersTotal > 0 ? s : 1;
  return out;
}

/**
 * Type shares from class shares. Within a class the types keep the split of `current` (this
 * entry now); a class the entry lacks takes the split of `fallback` (all entries), else an
 * equal split over the class's types.
 */
export function typesFromClasses(
  classes: Shares,
  current: Shares,
  fallback: Shares,
  typesOfClass: Record<string, string[]>,
): Shares {
  const out: Shares = {};
  for (const [c, share] of Object.entries(classes)) {
    if (!(share > 0)) continue;
    const types = typesOfClass[c] ?? [];
    const pick = (src: Shares) => {
      const w = types.map((t) => [t, src[t] ?? 0] as const).filter(([, x]) => x > 0);
      const tot = w.reduce((a, [, x]) => a + x, 0);
      return tot > 0 ? w.map(([t, x]) => [t, x / tot] as const) : null;
    };
    const split = pick(current) ?? pick(fallback) ?? types.map((t) => [t, 1 / types.length] as const);
    for (const [t, x] of split) out[t] = share * x;
  }
  return out;
}

/** The flow of a piecewise-constant profile at time `t` (veh/h). */
export function flowAt(flow: { from_s: number; veh_per_h: number }[], t: number): number {
  let q = 0;
  for (const f of flow) if (f.from_s <= t) q = f.veh_per_h;
  return q;
}
