/**
 * Twin compare (plan P4.8): two arms of an experiment with the same seed, on one clock; side by
 * side or overlaid; a KPI difference strip; whether the same agents spawn in the same place; and
 * the agents whose trips differ most.
 */
import type { OrthographicViewState } from "@deck.gl/core";
import { useEffect, useMemo, useRef, useState } from "react";
import { api, type RunListing } from "../api";
import { classOrder } from "../components/Dashboards";
import { KpiStrip } from "../components/KpiStrip";
import { Legend } from "../components/Legend";
import { KMH_PER_MS, clock, fmt } from "../format";
import { MapView } from "../render/MapView";
import { SCALES } from "../render/scales";
import { RunPicker, Scrubber, SqlFilter } from "../replay/ReplayView";
import { RunData, type RunSource } from "../replay/runData";
import { Clock, Timeline } from "../replay/timeline";
import { useStore } from "../store";
import { type Chrome, SERIES_DARK, hexToRgba } from "../theme";

interface Divergent {
  agent_id: bigint;
  vehicle_type: string;
  travel_time_a: number;
  travel_time_b: number;
  max_gap: number;
}

interface Spawn {
  both: number;
  same_place: number;
  only_a: number;
  only_b: number;
}

function source(path: string): RunSource {
  return { kind: "url", base: `/files/${path.split("/").map(encodeURIComponent).join("/")}`, label: path };
}

/** Pick two arms of one experiment point and replication from the server's runs. */
function ArmPicker({ onOpen }: { onOpen: (a: RunSource, b: RunSource) => void }) {
  const [runs, setRuns] = useState<RunListing[]>([]);
  const [exp, setExp] = useState("");
  const [point, setPoint] = useState(0);
  const [rep, setRep] = useState(0);
  const [armA, setArmA] = useState("");
  const [armB, setArmB] = useState("");
  useEffect(() => {
    api
      .runs()
      .then((l) => {
        const rs = l.runs.filter((r) => r.experiment && r.tables.includes("trajectories"));
        setRuns(rs);
        if (rs.length) setExp(rs[0].experiment!.experiment);
      })
      .catch(() => setRuns([]));
  }, []);
  const inExp = runs.filter((r) => r.experiment!.experiment === exp);
  const points = [...new Set(inExp.map((r) => r.experiment!.point))].sort((a, b) => a - b);
  const reps = [...new Set(inExp.filter((r) => r.experiment!.point === point).map((r) => r.experiment!.rep))].sort((a, b) => a - b);
  const arms = [...new Set(inExp.filter((r) => r.experiment!.point === point && r.experiment!.rep === rep).map((r) => r.experiment!.arm))].sort();
  useEffect(() => {
    if (arms.length && !arms.includes(armA)) setArmA(arms[0]);
    if (arms.length > 1 && !arms.includes(armB)) setArmB(arms[1]);
  }, [arms.join(",")]);
  const find = (arm: string) => inExp.find((r) => r.experiment!.point === point && r.experiment!.rep === rep && r.experiment!.arm === arm);
  if (!runs.length) return <p className="muted small">No experiment runs with trajectories under the server's runs directory; pick two runs instead.</p>;
  return (
    <div className="toolbar">
      <label>
        Experiment{" "}
        <select value={exp} onChange={(e) => setExp(e.target.value)}>
          {[...new Set(runs.map((r) => r.experiment!.experiment))].map((x) => (
            <option key={x}>{x}</option>
          ))}
        </select>
      </label>
      <label>
        Point{" "}
        <select value={point} onChange={(e) => setPoint(Number(e.target.value))}>
          {points.map((x) => (
            <option key={x}>{x}</option>
          ))}
        </select>
      </label>
      <label>
        Rep{" "}
        <select value={rep} onChange={(e) => setRep(Number(e.target.value))}>
          {reps.map((x) => (
            <option key={x}>{x}</option>
          ))}
        </select>
      </label>
      <label>
        Arm A{" "}
        <select value={armA} onChange={(e) => setArmA(e.target.value)}>
          {arms.map((x) => (
            <option key={x}>{x}</option>
          ))}
        </select>
      </label>
      <label>
        Arm B{" "}
        <select value={armB} onChange={(e) => setArmB(e.target.value)}>
          {arms.map((x) => (
            <option key={x}>{x}</option>
          ))}
        </select>
      </label>
      <button
        className="primary"
        disabled={!find(armA) || !find(armB) || armA === armB}
        onClick={() => onOpen({ ...source(find(armA)!.path), label: `${armA} (${find(armA)!.path})` }, { ...source(find(armB)!.path), label: `${armB} (${find(armB)!.path})` })}
      >
        Compare
      </button>
    </div>
  );
}

export function CompareView(_props: { theme: Chrome }) {
  const [runs, setRuns] = useState<[RunData, RunData] | null>(null);
  const [pending, setPending] = useState<RunSource | null>(null);
  const [error, setError] = useState<string | null>(null);
  const open = async (a: RunSource, b: RunSource) => {
    setError(null);
    try {
      const [ra, rb] = await Promise.all([RunData.open(a, classOrder), RunData.open(b, classOrder)]);
      setRuns([ra, rb]);
    } catch (e) {
      setError(String(e));
    }
  };
  return (
    <div className="view">
      <ArmPicker onOpen={open} />
      <details>
        <summary className="small">…or any two runs</summary>
        <RunPicker
          label={pending ? "Open as B" : "Open as A"}
          onOpen={(s) => {
            if (!pending) setPending(s);
            else {
              open(pending, s);
              setPending(null);
            }
          }}
        />
        {pending && <p className="small">A: {pending.label}. Now pick B.</p>}
      </details>
      {error && <p className="error">{error}</p>}
      {runs ? <Compare key={runs[0].alias + runs[1].alias} a={runs[0]} b={runs[1]} /> : <p className="muted">Pick two arms of a twin experiment (same seed): the same agent has the same id and draws in both.</p>}
    </div>
  );
}

function Compare({ a, b }: { a: RunData; b: RunData }) {
  const clockRef = useRef<Clock | null>(null);
  if (!clockRef.current) {
    const c = new Clock();
    c.range = [Math.max(a.tRange[0], b.tRange[0]), Math.min(a.tRange[1], b.tRange[1])];
    c.t.set(c.range[0]);
    clockRef.current = c;
  }
  const clk = clockRef.current;
  useEffect(() => () => clk.dispose(), []);
  const ta = useMemo(() => new Timeline(a, clk), [a]);
  const tb = useMemo(() => new Timeline(b, clk), [b]);
  const [mode, setMode] = useState<"side" | "overlay">("side");
  const [filter, setFilter] = useState("");
  // Unset until the first pan or zoom: both maps fit the same bounds on their own.
  const [view, setView] = useState<OrthographicViewState | undefined>(undefined);
  const [spawn, setSpawn] = useState<Spawn | null>(null);
  const [top, setTop] = useState<Divergent[] | null>(null);
  const [selected, setSelected] = useState<bigint | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const t = useStore(clk.t);
  useEffect(() => {
    ta.setFilter(filter);
    tb.setFilter(filter);
  }, [filter]);
  useEffect(() => {
    const A = a.table("agents");
    const B = b.table("agents");
    a.rows(
      `SELECT count(*) FILTER (WHERE x.t_arrival = y.t_arrival AND x.entry_y = y.entry_y AND x.entry = y.entry) AS same_place, count(*) AS both,
              (SELECT count(*) FROM ${A}) - count(*) AS only_a, (SELECT count(*) FROM ${B}) - count(*) AS only_b
       FROM ${A} x JOIN ${B} y USING (agent_id)`,
    )
      .then((r) => setSpawn({ both: Number(r[0].both), same_place: Number(r[0].same_place), only_a: Number(r[0].only_a), only_b: Number(r[0].only_b) }))
      .catch((e) => setErr(String(e)));
    a.rows(
      `WITH gap AS (
         SELECT p.agent_id, max(abs(p.s - q.s)) AS max_gap
         FROM ${a.table("trajectories")} p JOIN ${b.table("trajectories")} q ON p.agent_id = q.agent_id AND p.t = q.t AND p.ribbon = q.ribbon
         GROUP BY 1)
       SELECT x.agent_id, x.vehicle_type, x.travel_time AS travel_time_a, y.travel_time AS travel_time_b, g.max_gap AS max_gap
       FROM ${A} x JOIN ${B} y USING (agent_id) LEFT JOIN gap g USING (agent_id)
       WHERE x.exited AND y.exited
       ORDER BY abs(y.travel_time - x.travel_time) DESC LIMIT 12`,
    )
      .then((r) => setTop(r.map((x) => ({ agent_id: BigInt(x.agent_id as bigint), vehicle_type: String(x.vehicle_type), travel_time_a: Number(x.travel_time_a), travel_time_b: Number(x.travel_time_b), max_gap: x.max_gap === null ? NaN : Number(x.max_gap) }))))
      .catch((e) => setErr(String(e)));
  }, [a, b]);
  const diff = useMemo(() => {
    const m = new Map<string, number | null>();
    for (const [k, v] of b.kpis) {
      const va = a.kpis.get(k);
      m.set(k, v !== null && va !== null && va !== undefined ? v - va : null);
    }
    return m;
  }, [a, b]);
  const highlighted = useMemo(() => new Set(top?.map((x) => x.agent_id) ?? []), [top]);
  const scale = SCALES.class;
  const colA = hexToRgba(SERIES_DARK[0], 230);
  const colB = hexToRgba(SERIES_DARK[1], 230);
  const label = (r: RunData) => (r.manifest as { experiment?: { arm: string } }).experiment?.arm ?? r.label;
  const hover = (run: RunData) => (_s: string, i: number, d: { cls: Uint8Array; id: BigUint64Array; value: Float32Array }) =>
    `${label(run)}: ${run.classes[d.cls[i]]?.replace(/_/g, " ")} · agent ${d.id[i]} · ${fmt(d.value[i] * KMH_PER_MS)} km/h`;

  return (
    <>
      <KpiStrip values={diff} diff note={`B − A: ${label(b)} minus ${label(a)} (seeds ${String((a.manifest as { seed?: number }).seed)} / ${String((b.manifest as { seed?: number }).seed)})`} />
      <Scrubber clockObj={clk} run={a} />
      <SqlFilter value={filter} onApply={setFilter} columns="trajectory and agent columns" error={ta.error ?? tb.error} />
      <div className="toolbar">
        <label>
          <input type="radio" checked={mode === "side"} onChange={() => setMode("side")} /> Side by side
        </label>
        <label>
          <input type="radio" checked={mode === "overlay"} onChange={() => setMode("overlay")} /> Overlay
        </label>
        <span className="muted small">Ringed: the agents whose travel times differ most.</span>
      </div>
      {mode === "side" ? (
        <div className="grid2">
          {[
            [a, ta],
            [b, tb],
          ].map(([run, tl]) => (
            <div key={(run as RunData).alias} className="map-wrap half">
              <div className="map-title small">{label(run as RunData)}</div>
              <MapView
                ribbons={(run as RunData).geometry.ribbons}
                detectors={(run as RunData).geometry.detectors}
                bounds={(run as RunData).geometry.bounds}
                classes={(run as RunData).classes}
                scale={scale}
                sources={[{ id: (run as RunData).alias, display: () => (tl as Timeline).display() }]}
                selected={selected}
                highlighted={highlighted}
                viewState={view}
                onViewState={setView}
                onPick={(id) => setSelected(id)}
                hoverLabel={hover(run as RunData)}
              />
            </div>
          ))}
          <div className="span2">
            <Legend scale={scale} label="Class" unit="" />
          </div>
        </div>
      ) : (
        <div className="map-wrap">
          <MapView
            ribbons={a.geometry.ribbons}
            detectors={a.geometry.detectors}
            bounds={a.geometry.bounds}
            classes={a.classes}
            scale={scale}
            sources={[
              { id: "a", display: () => ta.display(), fixedColour: colA, opacity: 0.8 },
              { id: "b", display: () => tb.display(), fixedColour: colB, opacity: 0.8 },
            ]}
            selected={selected}
            highlighted={highlighted}
            onPick={(id) => setSelected(id)}
            hoverLabel={(s, i, d) => hover(s === "a" ? a : b)(s, i, d)}
          />
          <Legend
            scale={scale}
            label=""
            unit=""
            extra={[
              { label: `A: ${label(a)}`, colour: SERIES_DARK[0] },
              { label: `B: ${label(b)}`, colour: SERIES_DARK[1] },
            ]}
          />
        </div>
      )}
      {err && <p className="error small">{err}</p>}
      <div className="grid2">
        <section>
          <h4>Same agents, same place at spawn</h4>
          {spawn ? (
            <p data-testid="spawn-match">
              {spawn.same_place} of {spawn.both} agents present in both arms arrive at the same time and enter at the same lateral position ({fmt((100 * spawn.same_place) / Math.max(1, spawn.both), 1)}%). Only in A: {spawn.only_a}; only in B:{" "}
              {spawn.only_b}.
            </p>
          ) : (
            <p className="muted small">Joining agents.parquet of both arms…</p>
          )}
        </section>
        <section>
          <h4>Trips that differ most (B − A)</h4>
          {top === null ? (
            <p className="muted small">Joining trajectories of both arms…</p>
          ) : (
            <table className="grid small" data-testid="divergent">
              <thead>
                <tr>
                  <th>agent</th>
                  <th>type</th>
                  <th>travel time A (s)</th>
                  <th>B (s)</th>
                  <th>B − A (s)</th>
                  <th title="Largest difference in station s at the same time; — if never in the network at the same time">max gap at equal t (m)</th>
                </tr>
              </thead>
              <tbody>
                {top.map((x) => (
                  <tr key={String(x.agent_id)} className={selected === x.agent_id ? "sel" : ""}>
                    <td>
                      <button className="link" onClick={() => setSelected(x.agent_id)}>
                        {String(x.agent_id)}
                      </button>
                    </td>
                    <td>{x.vehicle_type}</td>
                    <td>{fmt(x.travel_time_a)}</td>
                    <td>{fmt(x.travel_time_b)}</td>
                    <td>{fmt(x.travel_time_b - x.travel_time_a)}</td>
                    <td>{fmt(x.max_gap)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </section>
      </div>
      <p className="muted small">Cursor {clock(t)}.</p>
    </>
  );
}
