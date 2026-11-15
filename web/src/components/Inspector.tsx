/**
 * Agent inspector (plan P4.4, P8.3): the agent's row as `agents.parquet` would hold it (exact
 * values, SI units), its persona and traits, its current state, every neighbour its leader scan
 * considers with the terms of the following model, and the logit breakdown of each open
 * violation decision (ADR-0018).
 */
import { KMH_PER_MS, exact, fmt } from "../format";
import type { Inspector as Ins } from "../protocol.gen";

const RECORD: [keyof NonNullable<Ins["record"]>, string][] = [
  ["agent_id", ""],
  ["vehicle_type", ""],
  ["class", ""],
  ["entry", ""],
  ["t_arrival", "s"],
  ["t_entry", "s"],
  ["spawn_delay", "s"],
  ["entry_y", "m"],
  ["length", "m"],
  ["width", "m"],
  ["v0", "m/s"],
  ["time_gap", "s"],
  ["standstill_gap", "m"],
  ["max_accel", "m/s²"],
  ["comfortable_decel", "m/s²"],
  ["distance", "m"],
  ["travel_time", "s"],
  ["delay", "s"],
  ["stops", ""],
];

const TRAITS: [keyof NonNullable<Ins["record"]>, string][] = [
  ["kappa", "κ at entry"],
  ["kappa_now", "κ now"],
  ["time_pressure", "π"],
  ["risk_tolerance", "ρ"],
  ["conformity", "c"],
  ["norm_sensitivity", "η"],
  ["skill", ""],
  ["distraction", "δ"],
  ["phone_user", ""],
  ["distraction_episodes", ""],
  ["violations", ""],
];

type Decision = Ins["decisions"][number];

function DecisionTable({ d }: { d: Decision }) {
  const value = (list: Decision["features"], name: string) => list.find((x) => x.name === name)?.value;
  // Which feature each term multiplies (imitation terms take ln(1 + count)).
  const feature: Record<string, string> = {
    aggressiveness: "kappa",
    time_saved: "time_saved",
    waiting_time: "waiting_time",
    imitation_violators: "n_violating",
    imitation_compliers: "n_complying",
    enforcement: "enforcement",
    risk: "risk",
  };
  return (
    <div data-testid={`decision-${d.kind}`}>
      <p className="small">
        <strong>{d.kind}</strong> · episode {d.episode} since {fmt(d.t_start, 1)} s ·{" "}
        {d.acted ? "acted" : "deciding"} · read at {fmt(d.t_read, 1)} s
      </p>
      <table className="grid small">
        <thead>
          <tr>
            <th>term</th>
            <th>coefficient</th>
            <th>feature</th>
            <th>term</th>
          </tr>
        </thead>
        <tbody>
          {d.terms.map((t) => (
            <tr key={t.name}>
              <th>{t.name}</th>
              <td>{fmt(value(d.coefficients, t.name) ?? NaN, 3)}</td>
              <td>{feature[t.name] ? fmt(value(d.features, feature[t.name]) ?? NaN, 3) : "—"}</td>
              <td data-term={t.name}>{fmt(t.value, 3)}</td>
            </tr>
          ))}
          <tr>
            <th>η (sum)</th>
            <td />
            <td />
            <td data-term="eta">{fmt(d.eta, 3)}</td>
          </tr>
        </tbody>
      </table>
      <p className="small">
        {d.hazard === null || d.hazard === undefined
          ? `P = σ(η) = ${fmt(d.probability, 3)}`
          : `λ = ${fmt(d.hazard, 4)} /s · Λ = ${fmt(d.integrated ?? NaN, 4)} of E = ${fmt(d.threshold ?? NaN, 4)} · P(acted by now) = ${fmt(d.probability, 3)}`}
      </p>
    </div>
  );
}

function ById({ onSelect }: { onSelect: (id: string) => void }) {
  return (
    <form
      className="row"
      onSubmit={(e) => {
        e.preventDefault();
        const v = (new FormData(e.currentTarget).get("id") as string).trim();
        if (v) onSelect(v);
      }}
    >
      <input name="id" placeholder="agent id" className="mono" aria-label="agent id" />
      <button type="submit">Inspect</button>
    </form>
  );
}

export function Inspector({ ins, onClose, onSelect }: { ins?: Ins; onClose: () => void; onSelect: (id: string) => void }) {
  if (!ins)
    return (
      <div>
        <p className="muted">Click a vehicle on the map, or give an agent id.</p>
        <ById onSelect={onSelect} />
      </div>
    );
  if (!ins.found)
    return (
      <div>
        <p>
          Agent {ins.id} is not in the network (not entered yet, or exited). <button onClick={onClose}>Clear</button>
        </p>
      </div>
    );
  const r = ins.record!;
  const st = ins.state!;
  return (
    <div className="inspector">
      <div className="row">
        <strong>
          {r.vehicle_type} · {r.class}
        </strong>
        <button onClick={onClose}>Clear</button>
      </div>
      <h4>Drawn parameters and trip (as in agents.parquet)</h4>
      <table className="kv" data-testid="inspector-record">
        <tbody>
          {RECORD.map(([k, unit]) => (
            <tr key={k}>
              <th>{k}</th>
              <td data-field={k}>{typeof r[k] === "number" ? exact(r[k] as number) : String(r[k])}</td>
              <td className="muted">
                {unit}
                {k === "v0" ? ` (${fmt((r.v0 as number) * KMH_PER_MS)} km/h)` : ""}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <h4>Persona and traits{r.persona ? `: ${r.persona}` : " (personas off)"}</h4>
      <table className="kv" data-testid="inspector-traits">
        <tbody>
          {TRAITS.map(([k, label]) => (
            <tr key={k}>
              <th>{k}</th>
              <td data-field={k}>{typeof r[k] === "number" ? fmt(r[k] as number, 3) : String(r[k])}</td>
              <td className="muted">{label}</td>
            </tr>
          ))}
        </tbody>
      </table>
      <h4>State at {fmt(ins.t, 1)} s</h4>
      <table className="kv">
        <tbody>
          <tr>
            <th>ribbon, s, y</th>
            <td>
              {st.ribbon}, {fmt(st.s, 2)} m, {fmt(st.y, 2)} m
            </td>
          </tr>
          <tr>
            <th>speed</th>
            <td>
              {fmt(st.v, 2)} m/s ({fmt(st.v * KMH_PER_MS)} km/h)
            </td>
          </tr>
          <tr>
            <th>acceleration</th>
            <td>
              {fmt(st.acc, 2)} m/s² (free road {fmt(ins.free_accel ?? NaN, 2)})
            </td>
          </tr>
        </tbody>
      </table>
      <h4>Neighbours considered ({ins.neighbours.length})</h4>
      {ins.neighbours.length === 0 ? (
        <p className="muted small">Nobody within the look-ahead interacts with it: it follows its free-road acceleration.</p>
      ) : (
        <table className="grid small">
          <thead>
            <tr>
              <th>agent</th>
              <th>role</th>
              <th>gap (m)</th>
              <th>lateral gap (m)</th>
              <th>clearance (m)</th>
              <th>ω</th>
              <th>a_IDM (m/s²)</th>
            </tr>
          </thead>
          <tbody>
            {ins.neighbours.map((n) => (
              <tr key={n.id}>
                <td>
                  <button className="link" onClick={() => onSelect(n.id)}>
                    {n.vehicle_type} {n.id}
                  </button>
                </td>
                <td>{n.role}</td>
                <td>{fmt(n.gap_s, 2)}</td>
                <td>{fmt(n.lateral_gap, 2)}</td>
                <td>{fmt(n.clearance, 2)}</td>
                <td>{n.weight === null || n.weight === undefined ? "—" : fmt(n.weight, 3)}</td>
                <td>{n.leader_accel === null || n.leader_accel === undefined ? "—" : fmt(n.leader_accel, 2)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
      <h4>Violation decisions ({ins.decisions.length})</h4>
      {ins.decisions.length === 0 ? (
        <p className="muted small">No open violation episode: the agent is not at a trigger.</p>
      ) : (
        ins.decisions.map((d) => <DecisionTable key={`${d.kind}-${d.episode}`} d={d} />)
      )}
      <p className="muted small">{ins.note}</p>
    </div>
  );
}
