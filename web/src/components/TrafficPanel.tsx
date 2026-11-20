/**
 * Traffic panel (sandbox track S1): how much traffic comes in and what it is made of, changed
 * while the run plays. Each change is a `set_demand` command: logged, and replayed exactly
 * when the session is saved. Sliders send on release, not while dragging.
 */
import { useMemo, useState } from "react";
import type { ClientMessage, DemandView, Hello } from "../protocol.gen";
import { CLASS_LABELS, type Shares, classShares, flowAt, setClassShare, typesFromClasses } from "../live/demand";

export interface TrafficPanelProps {
  hello: Hello;
  demand: DemandView[];
  t: number;
  send: (m: ClientMessage) => void;
}

/** A slider plus a number box; `onCommit` fires on release, Enter or blur. */
function Knob({ value, max, step, unit, label, onCommit }: { value: number; max: number; step: number; unit: string; label: string; onCommit: (x: number) => void }) {
  const [draft, setDraft] = useState<number | null>(null);
  const shown = draft ?? value;
  const commit = () => {
    if (draft !== null && Number.isFinite(draft) && draft !== value) onCommit(draft);
    setDraft(null);
  };
  return (
    <div className="knob">
      <span className="knob-label">{label}</span>
      <input
        type="range"
        min={0}
        max={max}
        step={step}
        value={Math.min(shown, max)}
        aria-label={label}
        onChange={(e) => setDraft(Number(e.target.value))}
        onPointerUp={commit}
        onKeyUp={commit}
        onBlur={commit}
      />
      <input
        type="number"
        min={0}
        step={step}
        value={Math.round(shown * 10) / 10}
        aria-label={`${label} value`}
        onChange={(e) => setDraft(Number(e.target.value))}
        onKeyDown={(e) => e.key === "Enter" && commit()}
        onBlur={commit}
        style={{ width: 70 }}
      />
      <span className="muted small">{unit}</span>
    </div>
  );
}

export function TrafficPanel({ hello, demand, t, send }: TrafficPanelProps) {
  const [target, setTarget] = useState<string>("*");
  const classOf = useMemo(() => Object.fromEntries(hello.vehicle_types.map((v) => [v.key, v.class])), [hello]);
  const typesOfClass = useMemo(() => {
    const m: Record<string, string[]> = {};
    for (const v of hello.vehicle_types) (m[v.class] ??= []).push(v.key);
    return m;
  }, [hello]);
  const entries = hello.scenario.demand.entries;
  const base = (id: string) => {
    const e = entries.find((x) => x.id === id);
    return e ? flowAt(e.flow, t) : 0;
  };
  const comp = (d: DemandView) => d.composition as Shares;
  const total = demand.reduce((a, d) => a + d.veh_per_h, 0);
  const baseTotal = demand.reduce((a, d) => a + base(d.entry), 0);
  const pct = baseTotal > 0 ? (100 * total) / baseTotal : 100;

  // All entries together, as one mix weighted by flow: the fallback split and the "all" view.
  const pooled = useMemo(() => {
    const out: Shares = {};
    for (const d of demand) for (const [k, w] of Object.entries(comp(d))) out[k] = (out[k] ?? 0) + w * Math.max(d.veh_per_h, 1e-9);
    return out;
  }, [demand]);
  const targets = target === "*" ? demand : demand.filter((d) => d.entry === target);
  const shownMix = classShares(target === "*" ? pooled : comp(targets[0] ?? demand[0]), classOf);

  const setFlow = (entry: string, q: number) => send({ cmd: "set_demand", entry, veh_per_h: Math.max(0, Math.round(q)) });
  const setAll = (percent: number) => {
    for (const d of demand) setFlow(d.entry, (base(d.entry) * percent) / 100);
  };
  const setClass = (cls: string, percent: number) => {
    for (const d of targets) {
      const classes = setClassShare(classShares(comp(d), classOf), cls, percent / 100);
      send({ cmd: "set_demand", entry: d.entry, composition: typesFromClasses(classes, comp(d), pooled, typesOfClass) });
    }
  };

  return (
    <div className="traffic">
      <p className="muted small">
        Changes act from the next step. They go into the session log, so a saved session replays exactly. A flow change holds for the rest of the run (it replaces the
        scenario&apos;s later flow steps).
      </p>
      <div className="section-title">Traffic volume</div>
      {demand.length > 1 && (
        <>
          <Knob label="All entries" unit="% of scenario" value={pct} max={300} step={5} onCommit={setAll} />
          <p className="muted small">{Math.round(total)} veh/h in all now, against {Math.round(baseTotal)} in the scenario at this time.</p>
        </>
      )}
      {demand.map((d) => (
        <Knob
          key={d.entry}
          label={demand.length > 1 ? `Entry ${d.entry}` : "Flow"}
          unit="veh/h"
          value={d.veh_per_h}
          max={Math.max(3000, 3 * base(d.entry), d.veh_per_h)}
          step={50}
          onCommit={(q) => setFlow(d.entry, q)}
        />
      ))}
      <div className="section-title">Vehicle mix</div>
      {demand.length > 1 && (
        <label className="small">
          Apply to{" "}
          <select value={target} onChange={(e) => setTarget(e.target.value)}>
            <option value="*">all entries</option>
            {demand.map((d) => (
              <option key={d.entry} value={d.entry}>
                entry {d.entry}
              </option>
            ))}
          </select>
        </label>
      )}
      {CLASS_LABELS.filter(([c]) => typesOfClass[c]).map(([c, label]) => (
        <Knob key={`${target}-${c}`} label={label} unit="% of vehicles" value={100 * (shownMix[c] ?? 0)} max={100} step={1} onCommit={(x) => setClass(c, x)} />
      ))}
      <p className="muted small">Raising one class scales the others down in proportion. Within a class the vehicle types keep the entry&apos;s split.</p>
    </div>
  );
}
