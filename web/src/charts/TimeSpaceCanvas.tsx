/**
 * Time–space diagram: every sample a 2 px dot at (t, s), coloured by speed on a one-hue ramp.
 * Drawn on a canvas: ECharts' large-scatter mode, the only one fast enough for 10⁵–10⁶ points,
 * cannot colour points individually. Hover shows the time and station under the cursor.
 */
import { useEffect, useRef, useState } from "react";
import { KMH_PER_MS, clock, fmt } from "../format";
import type { Chrome } from "../theme";

const PAD = { left: 56, right: 70, top: 14, bottom: 38 };

function ticks(lo: number, hi: number, n: number): number[] {
  const span = hi - lo || 1;
  const step0 = span / n;
  const mag = 10 ** Math.floor(Math.log10(step0));
  const step = [1, 2, 2.5, 5, 10].map((m) => m * mag).find((s) => s >= step0) ?? step0;
  const out: number[] = [];
  for (let v = Math.ceil(lo / step) * step; v <= hi + 1e-9; v += step) out.push(v);
  return out;
}

function parse(hex: string): [number, number, number] {
  return [parseInt(hex.slice(1, 3), 16), parseInt(hex.slice(3, 5), 16), parseInt(hex.slice(5, 7), 16)];
}

export function TimeSpaceCanvas(p: { t: ArrayLike<number>; s: ArrayLike<number>; v: ArrayLike<number>; length: number; ribbon: string; theme: Chrome; height?: number; vmaxKmh?: number }) {
  const ref = useRef<HTMLCanvasElement>(null);
  const [hover, setHover] = useState<string | null>(null);
  const height = p.height ?? 250;
  const vmax = (p.vmaxKmh ?? 90) / KMH_PER_MS;
  const ramp = (p.theme.dark ? [...p.theme.sequential].reverse() : p.theme.sequential.slice(3)).map(parse);
  let t0 = Infinity;
  let t1 = -Infinity;
  for (let i = 0; i < p.t.length; i++) {
    if (p.t[i] < t0) t0 = p.t[i];
    if (p.t[i] > t1) t1 = p.t[i];
  }
  if (!(t1 > t0)) t1 = t0 + 1;

  useEffect(() => {
    const c = ref.current!;
    const dpr = window.devicePixelRatio || 1;
    const w = c.clientWidth;
    c.width = w * dpr;
    c.height = height * dpr;
    const ctx = c.getContext("2d")!;
    ctx.scale(dpr, dpr);
    ctx.clearRect(0, 0, w, height);
    const pw = w - PAD.left - PAD.right;
    const ph = height - PAD.top - PAD.bottom;
    const X = (t: number) => PAD.left + ((t - t0) / (t1 - t0)) * pw;
    const Y = (s: number) => PAD.top + ph - (s / Math.max(1, p.length)) * ph;
    ctx.font = "11px system-ui";
    ctx.strokeStyle = p.theme.grid;
    ctx.fillStyle = p.theme.ink2;
    for (const tv of ticks(t0, t1, 6)) {
      const x = X(tv);
      ctx.beginPath();
      ctx.moveTo(x, PAD.top);
      ctx.lineTo(x, PAD.top + ph);
      ctx.stroke();
      ctx.textAlign = "center";
      ctx.fillText(clock(tv).slice(0, -2), x, PAD.top + ph + 14);
    }
    for (const sv of ticks(0, p.length, 4)) {
      const y = Y(sv);
      ctx.beginPath();
      ctx.moveTo(PAD.left, y);
      ctx.lineTo(PAD.left + pw, y);
      ctx.stroke();
      ctx.textAlign = "right";
      ctx.fillText(fmt(sv, 0), PAD.left - 6, y + 4);
    }
    ctx.textAlign = "center";
    ctx.fillText("Simulated time", PAD.left + pw / 2, height - 6);
    ctx.save();
    ctx.translate(14, PAD.top + ph / 2);
    ctx.rotate(-Math.PI / 2);
    ctx.fillText(`s on ${p.ribbon} (m)`, 0, 0);
    ctx.restore();
    // Points, coloured into an image buffer for speed.
    const img = ctx.getImageData(0, 0, w * dpr, height * dpr);
    const data = img.data;
    const W = w * dpr;
    for (let i = 0; i < p.t.length; i++) {
      const f = Math.min(1, Math.max(0, p.v[i] / vmax)) * (ramp.length - 1);
      const k = Math.min(ramp.length - 2, Math.floor(f));
      const a = ramp[k];
      const b = ramp[k + 1];
      const u = f - k;
      const x0 = Math.round(X(p.t[i]) * dpr);
      const y0 = Math.round(Y(p.s[i]) * dpr);
      for (let dx = 0; dx < 2 * dpr; dx++)
        for (let dy = 0; dy < 2 * dpr; dy++) {
          const x = x0 + dx;
          const y = y0 + dy;
          if (x < 0 || y < 0 || x >= W || y >= height * dpr) continue;
          const o = 4 * (y * W + x);
          data[o] = a[0] + (b[0] - a[0]) * u;
          data[o + 1] = a[1] + (b[1] - a[1]) * u;
          data[o + 2] = a[2] + (b[2] - a[2]) * u;
          data[o + 3] = 255;
        }
    }
    ctx.putImageData(img, 0, 0);
    // Colour scale.
    const gx = w - PAD.right + 24;
    for (let k = 0; k < ph; k++) {
      const f = (1 - k / ph) * (ramp.length - 1);
      const j = Math.min(ramp.length - 2, Math.floor(f));
      const u = f - j;
      const col = ramp[j].map((x, q) => Math.round(x + (ramp[j + 1][q] - x) * u));
      ctx.fillStyle = `rgb(${col.join(",")})`;
      ctx.fillRect(gx, PAD.top + k, 10, 1);
    }
    ctx.fillStyle = p.theme.ink2;
    ctx.textAlign = "left";
    ctx.fillText(`${p.vmaxKmh ?? 90} km/h`, gx - 2, PAD.top - 1 < 8 ? 8 : PAD.top - 1);
    ctx.fillText("0", gx + 14, PAD.top + ph);
  }, [p.t, p.s, p.v, p.length, p.theme, height]);

  return (
    <div>
      <canvas
        ref={ref}
        style={{ width: "100%", height }}
        role="img"
        aria-label={`Time–space diagram of ${p.ribbon}: ${p.t.length} samples coloured by speed`}
        onMouseMove={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          const pw = r.width - PAD.left - PAD.right;
          const ph = height - PAD.top - PAD.bottom;
          const x = e.clientX - r.left - PAD.left;
          const y = e.clientY - r.top - PAD.top;
          if (x < 0 || x > pw || y < 0 || y > ph) return setHover(null);
          setHover(`${clock(t0 + (x / pw) * (t1 - t0))} · s ${fmt((1 - y / ph) * p.length, 0)} m`);
        }}
        onMouseLeave={() => setHover(null)}
      />
      <div className="small hoverline">{hover ?? `${p.t.length.toLocaleString("en")} samples`}</div>
    </div>
  );
}
