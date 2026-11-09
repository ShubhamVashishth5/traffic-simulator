/**
 * A time series (uPlot): one measure per chart (never two y-axes), simulated time on x, a
 * crosshair whose values show in the legend, and vertical markers at parameter changes.
 */
import { useEffect, useRef } from "react";
import uPlot from "uplot";
import "uplot/dist/uPlot.min.css";
import { clock } from "../format";
import type { Chrome } from "../theme";

export interface Series {
  label: string;
  values: (number | null)[];
  colour: string;
}

export function TimeSeries(p: { title: string; unit: string; x: number[]; series: Series[]; markers?: number[]; theme: Chrome; height?: number; warmup?: number }) {
  const el = useRef<HTMLDivElement>(null);
  const plot = useRef<uPlot | null>(null);
  const markers = useRef<number[]>([]);
  markers.current = p.markers ?? [];
  const key = p.series.map((s) => s.label + s.colour).join("|") + p.theme.dark;

  useEffect(() => {
    const t = p.theme;
    const axis = { stroke: t.ink2, grid: { stroke: t.grid, width: 1 }, ticks: { stroke: t.axis, width: 1 }, font: "11px system-ui" };
    const opts: uPlot.Options = {
      width: el.current!.clientWidth || 300,
      height: p.height ?? 150,
      legend: { show: true, live: true },
      cursor: { drag: { x: true, y: false }, points: { size: 8 } },
      scales: { x: { time: false } },
      axes: [
        { ...axis, values: (_u, vals) => vals.map((v) => clock(v).slice(0, -2)) },
        { ...axis, size: 48, label: p.unit, labelFont: "11px system-ui", labelSize: 14 },
      ],
      series: [
        { label: "t", value: (_u, v) => (v == null ? "—" : clock(v)) },
        ...p.series.map((s) => ({ label: s.label, stroke: s.colour, width: 2, points: { show: false }, value: (_u: uPlot, v: number | null) => (v == null ? "—" : v.toFixed(1)) })),
      ],
      hooks: {
        draw: [
          (u) => {
            const ctx = u.ctx;
            ctx.save();
            ctx.strokeStyle = t.muted;
            ctx.setLineDash([4, 4]);
            ctx.lineWidth = 1;
            for (const m of markers.current) {
              const x = u.valToPos(m, "x", true);
              if (x < u.bbox.left || x > u.bbox.left + u.bbox.width) continue;
              ctx.beginPath();
              ctx.moveTo(x, u.bbox.top);
              ctx.lineTo(x, u.bbox.top + u.bbox.height);
              ctx.stroke();
            }
            ctx.restore();
          },
        ],
      },
    };
    const u = new uPlot(opts, [p.x, ...p.series.map((s) => s.values)] as uPlot.AlignedData, el.current!);
    plot.current = u;
    const ro = new ResizeObserver(() => u.setSize({ width: el.current!.clientWidth, height: p.height ?? 150 }));
    ro.observe(el.current!);
    return () => {
      ro.disconnect();
      u.destroy();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  useEffect(() => {
    plot.current?.setData([p.x, ...p.series.map((s) => s.values)] as uPlot.AlignedData);
  }, [p.x, p.series]);

  return (
    <figure className="chart">
      <figcaption>{p.title}</figcaption>
      <div ref={el} />
    </figure>
  );
}
