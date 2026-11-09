/** Headline KPI tiles (plan P4.6). Values not modelled yet say so instead of showing zero. */
import { KPI_TILES } from "../analytics";
import { fmt } from "../format";

export function KpiStrip(p: { values: Map<string, number | null>; title?: string; diff?: boolean; note?: string }) {
  return (
    <section className="kpis" aria-label={p.title ?? "KPIs"}>
      {KPI_TILES.map((k) => {
        const raw = p.values.get(k.key);
        const v = raw === undefined || raw === null ? null : raw * (k.scale ?? 1);
        const missing = k.note && raw === undefined;
        return (
          <div key={k.key} className="tile" title={k.note}>
            <div className="tile-label">{k.label}</div>
            <div className={`tile-value ${missing ? "muted" : ""}`}>
              {missing ? "n/a" : p.diff && v !== null && v > 0 ? `+${fmt(v)}` : fmt(v, k.unit === "veh/h" ? 0 : 1)}
              {!missing && <span className="tile-unit"> {k.unit}</span>}
            </div>
            {missing && <div className="tile-note">{k.note}</div>}
          </div>
        );
      })}
      {p.note && <div className="tile-foot muted small">{p.note}</div>}
    </section>
  );
}
