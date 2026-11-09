/**
 * The dashboard rail (plan P4.6, plan/04 §8): time series, fundamental diagram, speed and lateral
 * histograms by class, and a time–space diagram. Live and replay feed it the same detector rows.
 */
import { useMemo } from "react";
import { type DetRow, fdPoints, intervals, lateralByClass } from "../analytics";
import { EChart, type EOption } from "../charts/EChart";
import { type Hist, Histograms } from "../charts/Histograms";
import { TimeSeries } from "../charts/TimeSeries";
import { TimeSpaceCanvas } from "../charts/TimeSpaceCanvas";
import { KMH_PER_MS, VEH_H_PER_VEH_S, VEH_KM_PER_VEH_M, clock, fmt } from "../format";
import { CLASS_GROUPS, type Chrome, groupOfClass } from "../theme";

export interface TimeSpace {
  ribbon: string;
  length: number;
  t: ArrayLike<number>;
  s: ArrayLike<number>;
  v: ArrayLike<number>;
}

export interface DashboardProps {
  rows: DetRow[];
  theme: Chrome;
  warmup: number;
  throughputDetector?: string | null;
  /** Simulated times of parameter changes (vertical markers; FD splits at the latest). */
  markers: number[];
  speedHists: Hist[];
  ts?: TimeSpace;
  tsNote?: string;
}

const CLASS_LABEL = (c: string) => c.replace(/_/g, " ");

export function classOrder(a: string, b: string): number {
  return groupOfClass(a) - groupOfClass(b) || (a < b ? -1 : 1);
}

export function Dashboards(p: DashboardProps) {
  const t = p.theme;
  const iv = useMemo(() => intervals(p.rows), [p.rows]);
  const screen = p.throughputDetector ?? iv.find((x) => x.kind === "screenline")?.detector_id;
  const areas = [...new Set(iv.filter((x) => x.kind === "area").map((x) => x.detector_id))].slice(0, 3);
  const latest = p.markers.length ? Math.max(...p.markers) : null;

  const series = useMemo(() => {
    const flow = iv.filter((x) => x.detector_id === screen);
    const xs = [...new Set(iv.map((x) => x.t_end))].sort((a, b) => a - b);
    const at = (id: string, f: (x: (typeof iv)[number]) => number) => {
      const m = new Map(iv.filter((x) => x.detector_id === id).map((x) => [x.t_end, f(x)]));
      return xs.map((x) => {
        const v = m.get(x);
        return v === undefined || !Number.isFinite(v) ? null : v;
      });
    };
    return {
      xs,
      flow: screen ? [{ label: screen, values: at(screen, (x) => x.flow * VEH_H_PER_VEH_S), colour: t.series[0] }] : [],
      density: areas.map((a, k) => ({ label: a, values: at(a, (x) => x.density * VEH_KM_PER_VEH_M), colour: t.series[k] })),
      speed: areas.map((a, k) => ({ label: a, values: at(a, (x) => x.speed * KMH_PER_MS), colour: t.series[k] })),
      n: flow.length,
    };
  }, [iv, screen, areas.join(","), t]);

  const fd: EOption = useMemo(() => {
    const pts = fdPoints(iv);
    const split = latest ?? p.warmup;
    const before = pts.filter((x) => x.t_start < split);
    const after = pts.filter((x) => x.t_start >= split);
    const names = latest !== null ? ["Before the latest change", "After the latest change"] : ["Warm-up", "Measured"];
    const point = (x: (typeof pts)[number]) => [x.density * VEH_KM_PER_VEH_M, x.flow * VEH_H_PER_VEH_S, x.t_start, x.detector_id];
    return {
      backgroundColor: "transparent",
      textStyle: { color: t.ink2, fontFamily: "system-ui" },
      grid: { left: 56, right: 16, top: 30, bottom: 38 },
      legend: { top: 0, left: 0, textStyle: { color: t.ink2 }, itemWidth: 10, itemHeight: 10 },
      tooltip: {
        trigger: "item",
        formatter: (q: { value: (number | string)[]; seriesName: string }) =>
          `${q.value[3]} · ${clock(Number(q.value[2]))}<br/>density ${fmt(Number(q.value[0]))} veh/km<br/>flow ${fmt(Number(q.value[1]), 0)} veh/h<br/>${q.seriesName}`,
      },
      xAxis: { name: "Density (veh/km)", nameLocation: "middle", nameGap: 24, axisLine: { lineStyle: { color: t.axis } }, axisLabel: { color: t.ink2 }, splitLine: { lineStyle: { color: t.grid } }, min: 0 },
      yAxis: { name: "Flow (veh/h)", nameLocation: "middle", nameGap: 42, axisLine: { lineStyle: { color: t.axis } }, axisLabel: { color: t.ink2 }, splitLine: { lineStyle: { color: t.grid } }, min: 0 },
      series: [
        { name: names[0], type: "scatter", symbolSize: 8, itemStyle: { color: t.muted, borderColor: t.surface, borderWidth: 2 }, data: before.map(point) },
        { name: names[1], type: "scatter", symbolSize: 9, itemStyle: { color: t.series[0], borderColor: t.surface, borderWidth: 2 }, data: after.map(point) },
      ],
    };
  }, [iv, latest, p.warmup, t]);

  const lateral: Hist[] = useMemo(() => {
    if (!screen) return [];
    return lateralByClass(p.rows, screen)
      .sort((a, b) => classOrder(a.cls, b.cls))
      .map((h) => ({ label: CLASS_LABEL(h.cls), colour: t.series[groupOfClass(h.cls)], x0: h.y0, bin: h.bin, counts: h.counts }));
  }, [p.rows, screen, t]);

  return (
    <div className="dashboards">
      <div className="grid3">
        <TimeSeries title={`Flow at ${screen ?? "—"}`} unit="veh/h" x={series.xs} series={series.flow} markers={p.markers} theme={t} />
        <TimeSeries title="Density (Edie)" unit="veh/km" x={series.xs} series={series.density} markers={p.markers} theme={t} />
        <TimeSeries title="Space-mean speed" unit="km/h" x={series.xs} series={series.speed} markers={p.markers} theme={t} />
      </div>
      <div className="grid2">
        <figure className="chart">
          <figcaption>Fundamental diagram (area detectors, one point per window)</figcaption>
          <EChart option={fd} height={250} />
        </figure>
        <figure className="chart">
          <figcaption>Time–space diagram{p.ts ? ` (${p.ts.ribbon})` : ""}</figcaption>
          {p.ts && p.ts.t.length ? (
            <TimeSpaceCanvas t={p.ts.t} s={p.ts.s} v={p.ts.v} length={p.ts.length} ribbon={p.ts.ribbon} theme={t} />
          ) : (
            <p className="muted small">{p.tsNote ?? "Choose a ribbon to sample."}</p>
          )}
        </figure>
      </div>
      <div className="grid2">
        <Histograms title="Speed by class" unit="km/h" hists={p.speedHists} note="No vehicles in the network." />
        <Histograms title={`Lateral crossing position at ${screen ?? "—"}`} unit="m, + left" hists={lateral} share note="No crossings after warm-up yet." />
      </div>
      <p className="muted small">
        Classes: {CLASS_GROUPS.map((g) => g.label).join(", ")}. Windows: {series.n} at {screen ?? "—"}.
      </p>
    </div>
  );
}
