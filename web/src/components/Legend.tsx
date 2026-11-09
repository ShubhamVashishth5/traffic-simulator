/** Map legend: class groups (categorical) or a gradient with its domain. */
import { CLASS_GROUPS, SERIES_DARK, divergingOnRoad, sequentialOnRoad } from "../theme";
import type { ColourScale } from "../render/MapView";

const css = (c: number[]) => `rgb(${c[0]},${c[1]},${c[2]})`;

export function Legend(p: { scale: ColourScale; label: string; unit: string; present?: Set<number>; extra?: { label: string; colour: string }[] }) {
  if (p.extra) {
    return (
      <div className="legend">
        {p.extra.map((e) => (
          <span key={e.label} className="legend-item">
            <span className="swatch" style={{ background: e.colour }} /> {e.label}
          </span>
        ))}
      </div>
    );
  }
  if (p.scale.kind === "categorical") {
    return (
      <div className="legend">
        {CLASS_GROUPS.map((g, k) => (
          <span key={g.label} className={`legend-item ${p.present && !p.present.has(k) ? "faded" : ""}`}>
            <span className="swatch" style={{ background: SERIES_DARK[k] }} /> {g.label}
          </span>
        ))}
      </div>
    );
  }
  const stops = Array.from({ length: 11 }, (_, i) => i / 10);
  const grad = stops.map((f) => css(p.scale.kind === "sequential" ? sequentialOnRoad(f) : divergingOnRoad(2 * f - 1))).join(",");
  const lo = p.scale.kind === "sequential" ? 0 : -p.scale.max;
  return (
    <div className="legend">
      <span className="legend-item">{p.label}</span>
      <span className="legend-item">
        {lo}
        <span className="gradient" style={{ background: `linear-gradient(90deg, ${grad})` }} />
        {p.scale.max} {p.unit}
      </span>
    </div>
  );
}
