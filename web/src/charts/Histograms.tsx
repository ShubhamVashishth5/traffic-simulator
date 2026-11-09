/**
 * Small-multiple histograms (one per vehicle class, shared x scale): single-series bars, so the
 * class is named by the panel title and colour is only a reminder. Hover shows the bin.
 */
import { useState } from "react";
import { fmt } from "../format";

export interface Hist {
  label: string;
  colour: string;
  /** Left edge of the first bin and bin width, in display units. */
  x0: number;
  bin: number;
  counts: number[];
}

export function Histograms(p: { title: string; unit: string; hists: Hist[]; share?: boolean; note?: string }) {
  const [hover, setHover] = useState<string | null>(null);
  const nBins = Math.max(1, ...p.hists.map((h) => h.counts.length));
  const x0 = Math.min(...p.hists.map((h) => h.x0), 0);
  const bin = p.hists[0]?.bin ?? 1;
  const W = 170;
  const H = 64;
  return (
    <figure className="chart">
      <figcaption>
        {p.title} <span className="muted">({p.unit}{p.share ? ", share of class" : ", vehicles"})</span>
      </figcaption>
      {p.hists.length === 0 && <p className="muted small">{p.note ?? "No data yet."}</p>}
      <div className="multiples">
        {p.hists.map((h) => {
          const total = h.counts.reduce((a, b) => a + b, 0) || 1;
          const vals = h.counts.map((c) => (p.share ? c / total : c));
          const max = Math.max(...vals, 1e-9);
          const bw = W / nBins;
          return (
            <div key={h.label} className="multiple">
              <div className="small">
                <span className="swatch" style={{ background: h.colour }} /> {h.label} <span className="muted">n = {total}</span>
              </div>
              <svg width={W} height={H + 14} role="img" aria-label={`${h.label} ${p.title}`}>
                <line x1={0} x2={W} y1={H} y2={H} className="baseline" />
                {vals.map((v, k) => {
                  const off = Math.round((h.x0 - x0) / bin);
                  const hgt = (v / max) * (H - 4);
                  const lo = h.x0 + k * bin;
                  const tip = `${h.label}: ${fmt(lo, 1)}–${fmt(lo + bin, 1)} ${p.unit}: ${p.share ? `${fmt(100 * v, 1)}%` : h.counts[k]}`;
                  return (
                    <rect
                      key={k}
                      x={(k + off) * bw + 1}
                      y={H - hgt}
                      width={Math.max(1, bw - 2)}
                      height={hgt}
                      rx={Math.min(2, bw / 4)}
                      fill={h.colour}
                      onMouseEnter={() => setHover(tip)}
                      onMouseLeave={() => setHover(null)}
                    >
                      <title>{tip}</title>
                    </rect>
                  );
                })}
                <text x={0} y={H + 12} className="tick">
                  {fmt(x0, 0)}
                </text>
                <text x={W} y={H + 12} className="tick" textAnchor="end">
                  {fmt(x0 + nBins * bin, 0)}
                </text>
              </svg>
            </div>
          );
        })}
      </div>
      <div className="small hoverline">{hover ?? " "}</div>
    </figure>
  );
}
