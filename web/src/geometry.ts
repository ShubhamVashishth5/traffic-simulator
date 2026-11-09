/**
 * Ribbon geometry for replay, where no server sends it: a port of `marg_net::Ribbon`
 * (`to_world`, `y_bounds`, `outline_stations`) and of the server's `hello` drawing shapes.
 * Tested against the same cases as the Rust code.
 */
import type { DetectorView, RibbonView } from "./protocol.gen";
import type { Scenario } from "./schema";

type RibbonSpec = Scenario["network"]["ribbons"][number];

export class Ribbon {
  readonly pts: [number, number][];
  readonly cum: number[];
  readonly dir: [number, number][];
  readonly width: { s: number; y_min: number; y_max: number }[];
  constructor(readonly spec: RibbonSpec) {
    this.pts = spec.polyline.map((p) => [p[0], p[1]] as [number, number]);
    this.cum = [0];
    this.dir = [];
    for (let k = 0; k + 1 < this.pts.length; k++) {
      const dx = this.pts[k + 1][0] - this.pts[k][0];
      const dy = this.pts[k + 1][1] - this.pts[k][1];
      const len = Math.hypot(dx, dy);
      this.dir.push([dx / len, dy / len]);
      this.cum.push(this.cum[k] + len);
    }
    this.width = [...spec.width].sort((a, b) => a.s - b.s);
  }
  get length(): number {
    return this.cum[this.cum.length - 1];
  }
  yBounds(s: number): [number, number] {
    const w = this.width;
    if (s <= w[0].s) return [w[0].y_min, w[0].y_max];
    const last = w[w.length - 1];
    if (s >= last.s) return [last.y_min, last.y_max];
    let i = 1;
    while (w[i].s <= s) i++;
    const a = w[i - 1];
    const b = w[i];
    const f = (s - a.s) / (b.s - a.s);
    return [a.y_min + f * (b.y_min - a.y_min), a.y_max + f * (b.y_max - a.y_max)];
  }
  private segment(s: number): number {
    let i = 0;
    while (i < this.cum.length && this.cum[i] <= s) i++;
    return Math.min(Math.max(i, 1), this.cum.length - 1) - 1;
  }
  toWorld(s: number, y: number): [number, number, number] {
    const k = this.segment(s);
    const [ux, uy] = this.dir[k];
    const base = this.pts[k];
    const ds = s - this.cum[k];
    return [base[0] + ux * ds - uy * y, base[1] + uy * ds + ux * y, Math.atan2(uy, ux)];
  }
  outlineStations(): number[] {
    const len = this.length;
    let minSeg = Infinity;
    for (let k = 1; k < this.cum.length; k++) minSeg = Math.min(minSeg, this.cum[k] - this.cum[k - 1]);
    const eps = minSeg * 1e-6;
    const st = [0, len];
    for (const c of this.cum.slice(1, -1)) st.push(c - eps, c + eps);
    for (const w of this.width) if (w.s > 0 && w.s < len) st.push(w.s);
    st.sort((a, b) => a - b);
    return st.filter((x, i) => i === 0 || x !== st[i - 1]);
  }
}

export interface Geometry {
  origin: [number, number];
  bounds: number[];
  ribbons: RibbonView[];
  detectors: DetectorView[];
  byId: Map<string, Ribbon>;
}

/** Drawing shapes for a scenario, relative to the same origin the server uses. */
export function geometry(sc: Scenario, medianBand: number): Geometry {
  let x0 = Infinity;
  let y0 = Infinity;
  for (const r of sc.network.ribbons)
    for (const p of r.polyline) {
      x0 = Math.min(x0, p[0]);
      y0 = Math.min(y0, p[1]);
    }
  const origin: [number, number] = [Math.floor(x0), Math.floor(y0)];
  const rel = (w: [number, number, number]): [number, number] => [w[0] - origin[0], w[1] - origin[1]];
  const byId = new Map<string, Ribbon>();
  const ribbons: RibbonView[] = sc.network.ribbons.map((spec) => {
    const r = new Ribbon(spec);
    byId.set(spec.id, r);
    const st = r.outlineStations();
    const right = st.map((s) => rel(r.toWorld(s, r.yBounds(s)[0])));
    const left = st.map((s) => rel(r.toWorld(s, r.yBounds(s)[1])));
    const band = st.map((s) => {
      const [a, b] = r.yBounds(s);
      return rel(r.toWorld(s, Math.min(a + medianBand, b)));
    });
    return {
      id: spec.id,
      length: r.length,
      centre: st.map((s) => rel(r.toWorld(s, 0))),
      outline: [...right, ...[...left].reverse()],
      median_band: [...right, ...[...band].reverse()],
      next: spec.next ?? null,
    };
  });
  const b = [Infinity, Infinity, -Infinity, -Infinity];
  for (const r of ribbons)
    for (const [x, y] of r.outline) {
      b[0] = Math.min(b[0], x);
      b[1] = Math.min(b[1], y);
      b[2] = Math.max(b[2], x);
      b[3] = Math.max(b[3], y);
    }
  const detectors: DetectorView[] = [];
  for (const d of sc.detectors ?? []) {
    const r = byId.get(d.ribbon);
    if (!r) continue;
    if (d.kind === "screenline") {
      const [lo, hi] = r.yBounds(d.s);
      detectors.push({ id: d.id, kind: "screenline", ribbon: d.ribbon, s_from: d.s, s_to: d.s, shape: [rel(r.toWorld(d.s, lo)), rel(r.toWorld(d.s, hi))] });
    } else {
      const st = [d.s_from, ...r.outlineStations().filter((s) => s > d.s_from && s < d.s_to), d.s_to];
      const shape = [...st.map((s) => rel(r.toWorld(s, r.yBounds(s)[0]))), ...[...st].reverse().map((s) => rel(r.toWorld(s, r.yBounds(s)[1])))];
      detectors.push({ id: d.id, kind: "area", ribbon: d.ribbon, s_from: d.s_from, s_to: d.s_to, shape });
    }
  }
  return { origin, bounds: b, ribbons, detectors, byId };
}
