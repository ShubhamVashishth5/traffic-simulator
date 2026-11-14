/**
 * Dashboard aggregates from detector rows (`detectors.parquet` columns, or the live windows,
 * which are the same rows for a shorter interval). Live and replay use these same functions, so
 * a chart seen live can be regenerated from the saved run (plan/04 §8).
 */

/** The `detectors.parquet` columns the dashboards use (SI). */
export interface DetRow {
  detector_id: string;
  kind: string;
  t_start: number;
  t_end: number;
  in_warmup: boolean;
  vehicle_type: string;
  class: string;
  count: number;
  flow?: number | null;
  density?: number | null;
  space_mean_speed?: number | null;
  tts?: number | null;
  ttd?: number | null;
  lateral_y0?: number | null;
  lateral_bin?: number | null;
  lateral_hist?: number[] | null;
}

/** One detector interval, summed over vehicle types (SI). */
export interface Interval {
  detector_id: string;
  kind: string;
  t_start: number;
  t_end: number;
  in_warmup: boolean;
  count: number;
  /** veh/s: screenline count/Δt, area Edie TTD/(LΔt). */
  flow: number;
  /** veh/m (areas only). */
  density: number;
  /** m/s: screenline harmonic mean of spot speeds, area TTD/TTS. */
  speed: number;
}

const val = (x: number | null | undefined) => (x === null || x === undefined || !Number.isFinite(x) ? 0 : x);

/** Sum rows of the same detector and interval over vehicle types. */
export function intervals(rows: DetRow[]): Interval[] {
  const by = new Map<string, { r: DetRow; count: number; flow: number; density: number; tts: number; ttd: number; invSum: number }>();
  for (const r of rows) {
    const key = `${r.detector_id}\u0000${r.t_start}`;
    let a = by.get(key);
    if (!a) {
      a = { r, count: 0, flow: 0, density: 0, tts: 0, ttd: 0, invSum: 0 };
      by.set(key, a);
    }
    a.count += r.count;
    a.flow += val(r.flow);
    a.density += val(r.density);
    a.tts += val(r.tts);
    a.ttd += val(r.ttd);
    // Harmonic mean across types: Σn / Σ(n / v_harm).
    if (r.kind === "screenline" && r.count > 0 && val(r.space_mean_speed) > 0) a.invSum += r.count / val(r.space_mean_speed);
  }
  const out: Interval[] = [];
  for (const a of by.values()) {
    const screen = a.r.kind === "screenline";
    out.push({
      detector_id: a.r.detector_id,
      kind: a.r.kind,
      t_start: a.r.t_start,
      t_end: a.r.t_end,
      in_warmup: a.r.in_warmup,
      count: a.count,
      flow: a.flow,
      density: screen ? NaN : a.density,
      speed: screen ? (a.invSum > 0 ? a.count / a.invSum : NaN) : a.tts > 0 ? a.ttd / a.tts : NaN,
    });
  }
  out.sort((x, y) => (x.detector_id < y.detector_id ? -1 : x.detector_id > y.detector_id ? 1 : x.t_start - y.t_start));
  return out;
}

/** Fundamental-diagram points (one per area-detector interval with traffic). */
export function fdPoints(iv: Interval[]): { density: number; flow: number; t_start: number; in_warmup: boolean; detector_id: string }[] {
  return iv
    .filter((x) => x.kind === "area" && x.density > 0)
    .map((x) => ({ density: x.density, flow: x.flow, t_start: x.t_start, in_warmup: x.in_warmup, detector_id: x.detector_id }));
}

export interface ClassHistogram {
  cls: string;
  y0: number;
  bin: number;
  counts: number[];
  total: number;
}

/** Lateral crossing histograms by class at one screenline, over intervals after warm-up. */
export function lateralByClass(rows: DetRow[], detector: string, includeWarmup = false): ClassHistogram[] {
  const by = new Map<string, ClassHistogram>();
  for (const r of rows) {
    if (r.detector_id !== detector || r.kind !== "screenline" || !r.lateral_hist || (!includeWarmup && r.in_warmup)) continue;
    let h = by.get(r.class);
    if (!h) {
      h = { cls: r.class, y0: val(r.lateral_y0), bin: val(r.lateral_bin), counts: new Array(r.lateral_hist.length).fill(0), total: 0 };
      by.set(r.class, h);
    }
    r.lateral_hist.forEach((c, k) => {
      if (k < h!.counts.length) h!.counts[k] += c;
    });
    h.total += r.count;
  }
  return [...by.values()];
}

/** Histogram of values with fixed bin width starting at 0. */
export function histogram(values: ArrayLike<number>, bin: number, maxBins: number): number[] {
  const out: number[] = [];
  for (let i = 0; i < values.length; i++) {
    const v = values[i];
    if (!Number.isFinite(v)) continue;
    const b = Math.min(maxBins - 1, Math.max(0, Math.floor(v / bin)));
    while (out.length <= b) out.push(0);
    out[b]++;
  }
  return out;
}

/** Headline KPI names and labels, in strip order. */
export const KPI_TILES: { key: string; label: string; unit: string; scale?: number; note?: string }[] = [
  { key: "throughput_veh_h", label: "Throughput", unit: "veh/h" },
  { key: "delay_mean_s", label: "Delay, mean", unit: "s" },
  { key: "delay_p95_s", label: "Delay, p95", unit: "s" },
  { key: "density", label: "Density", unit: "veh/km" },
  { key: "network_mean_speed_m_s", label: "Mean speed", unit: "km/h", scale: 3.6 },
  { key: "violations", label: "Violations", unit: "", note: "not modelled before P8" },
  { key: "conflicts", label: "Conflicts", unit: "" },
  { key: "gridlock_share", label: "Gridlock", unit: "%", scale: 100, note: "needs an area detector" },
];
