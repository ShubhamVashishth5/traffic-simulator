/**
 * Replay (plan P4.7): open a run folder from the server's runs directory, a URL or a local
 * directory; DuckDB-WASM over its Parquet tables; a time scrubber; a SQL filter; the live
 * dashboards computed from the stored tables; heatmaps.
 */
import { useEffect, useMemo, useRef, useState } from "react";
import { api, type RunListing } from "../api";
import { EChart, type EOption } from "../charts/EChart";
import { histogram } from "../analytics";
import { Dashboards, classOrder, type TimeSpace } from "../components/Dashboards";
import { KpiStrip } from "../components/KpiStrip";
import { Legend } from "../components/Legend";
import { SCALES } from "../render/scales";
import { KMH_PER_MS, clock, fmt } from "../format";
import { MapView } from "../render/MapView";
import { useStore } from "../store";
import { type Chrome, groupOfClass } from "../theme";
import { RunData, type RunSource, runFiles } from "./runData";
import { Clock, Timeline } from "./timeline";

export function RunPicker({ onOpen, label = "Open" }: { onOpen: (s: RunSource) => void; label?: string }) {
  const [runs, setRuns] = useState<RunListing[]>([]);
  const [pick, setPick] = useState("");
  const [url, setUrl] = useState("");
  useEffect(() => {
    api
      .runs()
      .then((l) => {
        setRuns(l.runs.filter((r) => r.tables.includes("agents")));
        const want = new URLSearchParams(window.location.hash.split("?")[1] ?? "").get("run");
        if (want) {
          setPick(want);
          onOpen({ kind: "url", base: `/files/${want.split("/").map(encodeURIComponent).join("/")}`, label: want });
        }
      })
      .catch(() => setRuns([]));
  }, []);
  return (
    <div className="toolbar">
      <label>
        Run{" "}
        <select value={pick} onChange={(e) => setPick(e.target.value)} style={{ maxWidth: 420 }}>
          <option value="">— runs under the server's runs directory —</option>
          {runs.map((r) => (
            <option key={r.path} value={r.path}>
              {r.path} {r.tables.includes("trajectories") ? "" : "(no trajectories)"}
            </option>
          ))}
        </select>
      </label>
      <button disabled={!pick} onClick={() => onOpen({ kind: "url", base: `/files/${pick.split("/").map(encodeURIComponent).join("/")}`, label: pick })}>
        {label}
      </button>
      <span className="muted">or</span>
      <input placeholder="URL of a run folder" value={url} onChange={(e) => setUrl(e.target.value)} style={{ width: 220 }} />
      <button disabled={!url} onClick={() => onOpen({ kind: "url", base: url.replace(/\/$/, ""), label: url })}>
        {label} URL
      </button>
      <span className="muted">or</span>
      <label className="filebutton">
        Local folder…
        <input
          type="file"
          // @ts-expect-error: non-standard directory picker attribute
          webkitdirectory=""
          multiple
          onChange={(e) => {
            if (e.target.files?.length) {
              const r = runFiles(e.target.files);
              onOpen({ kind: "files", files: r.files, label: r.label });
            }
          }}
        />
      </label>
    </div>
  );
}

export function Scrubber({ clockObj, run }: { clockObj: Clock; run: RunData }) {
  const t = useStore(clockObj.t);
  const playing = useStore(clockObj.playing);
  const [rate, setRate] = useState(clockObj.rate);
  const [lo, hi] = clockObj.range;
  return (
    <div className="toolbar scrubber">
      <button className="primary" onClick={() => clockObj.playing.set(!playing)}>
        {playing ? "Pause" : "Play"}
      </button>
      <input type="range" min={lo} max={hi} step={run.trajEvery / 10} value={t} onChange={(e) => clockObj.seek(Number(e.target.value))} aria-label="time" style={{ flex: 1 }} />
      <span className="clock">{clock(t)}</span>
      <label>
        Rate{" "}
        <select
          value={rate}
          onChange={(e) => {
            clockObj.rate = Number(e.target.value);
            setRate(clockObj.rate);
          }}
        >
          {[1, 5, 10, 30, 60, 120].map((r) => (
            <option key={r} value={r}>
              {r}×
            </option>
          ))}
        </select>
      </label>
    </div>
  );
}

export function SqlFilter({ value, onApply, columns, error }: { value: string; onApply: (s: string) => void; columns: string; error?: string | null }) {
  const [draft, setDraft] = useState(value);
  return (
    <div className="toolbar">
      <label style={{ flex: 1, display: "flex", gap: 6, alignItems: "center" }}>
        <span>SQL filter</span>
        <input
          className="mono"
          style={{ flex: 1 }}
          placeholder="e.g. class = 'two_wheeler' AND v0 > 15"
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && onApply(draft)}
        />
      </label>
      <button onClick={() => onApply(draft)}>Apply</button>
      {value && (
        <button
          className="link"
          onClick={() => {
            setDraft("");
            onApply("");
          }}
        >
          clear
        </button>
      )}
      <span className="muted small" title={columns}>
        columns: trajectory (t, s, y, v, ribbon…) and agent (class, vehicle_type, v0, …)
      </span>
      {error && <span className="error small">{error}</span>}
    </div>
  );
}

function Heatmaps({ run, ribbon, filter, theme }: { run: RunData; ribbon: string; filter: string; theme: Chrome }) {
  const [data, setData] = useState<Awaited<ReturnType<RunData["heatmaps"]>> | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    setData(null);
    run
      .heatmaps(ribbon, filter, 30, 50, 0.5)
      .then(setData)
      .catch((e) => setError(String(e)));
  }, [run, ribbon, filter]);
  const opts = useMemo(() => {
    if (!data) return null;
    // Ordinal floor: the lightest step still reads against the surface.
    const ramp = theme.dark ? [...theme.sequential].reverse() : theme.sequential.slice(3);
    const heat = (cells: number[][], xName: string, yName: string, label: string, max: number, xFmt: (v: number) => string, vFmt: (v: number) => string): EOption => {
      const xs = [...new Set(cells.map((c) => c[0]))].sort((a, b) => a - b);
      const ys = [...new Set(cells.map((c) => c[1]))].sort((a, b) => a - b);
      const xi = new Map(xs.map((x, i) => [x, i]));
      const yi = new Map(ys.map((y, i) => [y, i]));
      return {
        backgroundColor: "transparent",
        animation: false,
        textStyle: { color: theme.ink2, fontFamily: "system-ui" },
        grid: { left: 56, right: 80, top: 10, bottom: 38 },
        tooltip: { formatter: (q: { value: number[] }) => `${xFmt(xs[q.value[0]])} · ${fmt(ys[q.value[1]], 1)} · ${vFmt(q.value[2])}` },
        xAxis: { type: "category", data: xs.map(xFmt), name: xName, nameLocation: "middle", nameGap: 24, axisLabel: { color: theme.ink2 }, splitArea: { show: false } },
        yAxis: { type: "category", data: ys.map((y) => fmt(y, 1)), name: yName, nameLocation: "middle", nameGap: 42, axisLabel: { color: theme.ink2 } },
        visualMap: { min: 0, max, dimension: 2, right: 0, top: 10, itemHeight: 120, text: [label, ""], textStyle: { color: theme.ink2 }, inRange: { color: ramp }, calculable: false },
        series: [{ type: "heatmap", progressive: 0, data: cells.map((c) => [xi.get(c[0]), yi.get(c[1]), c[2]]) }],
      };
    };
    const speed = heat(
      data.speed.map(([t, s, v]) => [t, s, v * KMH_PER_MS]),
      "Simulated time",
      "s (m)",
      "km/h",
      90,
      (v) => clock(v).slice(0, -2),
      (v) => `${fmt(v)} km/h`,
    );
    const maxN = Math.max(1, ...data.lateral.map((r) => r[2]));
    const lateral = heat(data.lateral, "s (m)", "y (m, + left)", "samples", maxN, (v) => fmt(v, 0), (v) => `${v} samples`);
    return { speed, lateral };
  }, [data, theme]);
  if (error) return <p className="error small">{error}</p>;
  if (!opts) return <p className="muted small">Computing heatmaps…</p>;
  return (
    <div className="grid2">
      <figure className="chart">
        <figcaption>Mean speed, 30 s × 50 m cells ({ribbon})</figcaption>
        <EChart option={opts.speed} height={240} />
      </figure>
      <figure className="chart">
        <figcaption>Where vehicles drive: trajectory samples per 50 m × 0.5 m cell</figcaption>
        <EChart option={opts.lateral} height={240} />
      </figure>
    </div>
  );
}

export function ReplayView({ theme }: { theme: Chrome }) {
  const [run, setRun] = useState<RunData | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const open = async (s: RunSource) => {
    setError(null);
    setLoading(true);
    try {
      const r = await RunData.open(s, classOrder);
      setRun(r);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  };
  return (
    <div className="view">
      <RunPicker onOpen={open} />
      {loading && <p className="muted">Opening with DuckDB-WASM…</p>}
      {error && <p className="error">{error}</p>}
      {run ? <Replay key={run.alias} run={run} theme={theme} /> : !loading && <p className="muted">Open a run folder. Runs with trajectories can be played back; others show their tables only.</p>}
    </div>
  );
}

function Replay({ run, theme }: { run: RunData; theme: Chrome }) {
  const clockRef = useRef<Clock | null>(null);
  if (!clockRef.current) {
    clockRef.current = new Clock();
    clockRef.current.range = [...run.tRange];
    clockRef.current.t.set(run.tRange[0]);
  }
  const clk = clockRef.current;
  const timeline = useMemo(() => new Timeline(run, clk), [run]);
  useEffect(() => () => clk.dispose(), []);
  const [filter, setFilter] = useState("");
  const [colour, setColour] = useState<"class" | "speed">("class");
  const [ribbon, setRibbon] = useState(run.scenario.network.ribbons[0]?.id ?? "");
  const [ts, setTs] = useState<TimeSpace | undefined>();
  const [tsError, setTsError] = useState<string | null>(null);
  const [selected, setSelected] = useState<bigint | null>(null);
  const loaded = useStore(timeline.loadedVersion);
  const t = useStore(clk.t);
  const hasTraj = run.tables.has("trajectories");
  useEffect(() => timeline.setFilter(filter), [filter]);
  useEffect(() => {
    if (!hasTraj) return;
    setTs(undefined);
    setTsError(null);
    const len = run.geometry.byId.get(ribbon)?.length ?? 0;
    run
      .timeSpace(ribbon, filter)
      .then((x) => setTs({ ribbon, length: len, ...x }))
      .catch((e) => setTsError(String(e)));
  }, [ribbon, filter]);

  // Speed histograms of the agents at the cursor (at most ~10 updates per second of scrubbing).
  const tick = Math.floor(t / Math.max(run.trajEvery, 1));
  const speedHists = useMemo(() => {
    const f = timeline.frameAt();
    if (!f) return [];
    return run.classes
      .map((c, k) => {
        const vs: number[] = [];
        for (let i = 0; i < f.n; i++) if (f.cls[i] === k) vs.push(f.value[i]);
        return { label: c.replace(/_/g, " "), colour: theme.series[groupOfClass(c)], x0: 0, bin: KMH_PER_MS, counts: histogram(vs, 1, 45) };
      })
      .filter((h) => h.counts.length);
  }, [tick, loaded, theme]);
  const scale = SCALES[colour];
  const m = run.manifest as { run_hash?: string; seed?: number; scenario?: string; experiment?: { arm: string; rep: number } };

  return (
    <>
      <KpiStrip
        values={run.kpis}
        note={`${run.label} · run ${m.run_hash} · seed ${m.seed}${m.experiment ? ` · arm ${m.experiment.arm}, rep ${m.experiment.rep}` : ""} · ${run.agentsCount} agents · tables: ${[...run.tables].join(", ")}`}
      />
      {hasTraj ? (
        <>
          <Scrubber clockObj={clk} run={run} />
          <SqlFilter value={filter} onApply={setFilter} columns="t, agent_id, ribbon, s, y, x_world, y_world, heading, v, a; agents.*" error={timeline.error} />
          <div className="map-wrap">
            <MapView
              ribbons={run.geometry.ribbons}
              detectors={run.geometry.detectors}
              bounds={run.geometry.bounds}
              classes={run.classes}
              scale={scale}
              sources={[{ id: "replay", display: () => timeline.display() }]}
              selected={selected}
              onPick={(id) => setSelected(id)}
              hoverLabel={(_s, i, b) => `${run.classes[b.cls[i]]?.replace(/_/g, " ")} · agent ${b.id[i]} · ${fmt(b.value[i] * KMH_PER_MS, 1)} km/h`}
            />
            <Legend scale={{ kind: scale.kind, max: Math.round(scale.max * scale.factor) }} label={colour === "class" ? "Class" : "Speed"} unit={scale.unit} />
            <div className="map-stats small">
              {timeline.frameAt()?.n ?? 0} agents at {clock(t)} {filter && `(filter: ${filter})`}
            </div>
          </div>
          <div className="toolbar">
            <label>
              Colour by{" "}
              <select value={colour} onChange={(e) => setColour(e.target.value as "class" | "speed")}>
                <option value="class">Class</option>
                <option value="speed">Speed</option>
              </select>
            </label>
            <label>
              Ribbon{" "}
              <select value={ribbon} onChange={(e) => setRibbon(e.target.value)}>
                {run.scenario.network.ribbons.map((r) => (
                  <option key={r.id}>{r.id}</option>
                ))}
              </select>
            </label>
            {selected !== null && <AgentRow run={run} id={selected} onClear={() => setSelected(null)} />}
          </div>
        </>
      ) : (
        <p className="notice">This run has no trajectories.parquet (set outputs.trajectories in the scenario to play it back).</p>
      )}
      <Dashboards
        rows={run.detectors}
        theme={theme}
        warmup={run.scenario.warmup_s ?? 0}
        throughputDetector={run.scenario.kpi?.throughput_detector}
        markers={[]}
        speedHists={speedHists}
        ts={ts}
        tsNote={tsError ?? (hasTraj ? "Loading…" : "No trajectories in this run.")}
      />
      {hasTraj && <Heatmaps run={run} ribbon={ribbon} filter={filter} theme={theme} />}
    </>
  );
}

function AgentRow({ run, id, onClear }: { run: RunData; id: bigint; onClear: () => void }) {
  const [row, setRow] = useState<Record<string, unknown> | null>(null);
  useEffect(() => {
    run.rows(`SELECT * FROM ${run.table("agents")} WHERE agent_id = ${id}`).then((r) => setRow(r[0] ?? null));
  }, [id]);
  if (!row) return null;
  return (
    <span className="small">
      agent {String(id)}: {String(row.vehicle_type)}, v0 {fmt(Number(row.v0) * KMH_PER_MS)} km/h, travel time {fmt(Number(row.travel_time))} s, delay {fmt(Number(row.delay))} s{" "}
      <button className="link" onClick={onClear}>
        clear
      </button>
    </span>
  );
}
