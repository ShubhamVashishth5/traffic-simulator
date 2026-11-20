/**
 * Behaviour panel (sandbox track S2): driver behaviour in plain words, on top of the register.
 * Each slider is one `live` register entry, changed with `set_param` (logged, undoable, replayed).
 * The full register stays in the Parameters tab.
 */
import { useEffect, useMemo, useState } from "react";
import { api } from "../api";
import { CLASS_LABELS } from "../live/demand";
import { type Param, type ParamValue, currentValue, flatten } from "../paramSchema";
import type { ChangeView } from "../protocol.gen";
import type { RegisterFile } from "../register";
import { Knob } from "./Knob";

export interface BehaviourPanelProps {
  overrides?: Record<string, ParamValue>;
  history: ChangeView[];
  /** Vehicle classes this scenario can send (`Hello.vehicle_types`); others are hidden. */
  classes: string[];
  onApply: (path: string, value: ParamValue) => void;
  onColourBySelfishness: () => void;
}

/** Plain-language rows: [register path, label, unit, explanation]. */
const POPULATION: [string, string, string, string][] = [
  ["personas.traits.aggressiveness.mean", "Everyone", "SD", "Selfishness of the whole population, in standard deviations from the guide's average driver."],
];
const LINKS: [string, string, string, string][] = [
  ["personas.links.clearance", "Squeezes past", "γ", "How much selfishness shrinks the side gap a driver keeps from others (clearance × e^(−γκ))."],
  ["personas.links.critical_gap", "Forces into gaps", "r", "How much selfishness shortens the gap a driver accepts at a junction (correlation with κ)."],
];

export function BehaviourPanel(props: BehaviourPanelProps) {
  const [files, setFiles] = useState<RegisterFile[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    api
      .register()
      .then((r) => setFiles(r.files))
      .catch((e) => setError(String(e)));
  }, []);
  const params = useMemo(() => new Map((files ? flatten(files) : []).map((p) => [p.path, p])), [files]);
  if (error) return <p className="error">Register: {error}</p>;
  if (!files) return <p className="muted">Loading the register…</p>;

  const row = ([path, label, unit, help]: [string, string, string, string]) => {
    const p: Param | undefined = params.get(path);
    if (!p || !p.entry.live) return null;
    const v = currentValue(p, props.overrides, props.history);
    if (typeof v !== "number") return null;
    const [lo, hi] = p.entry.range ?? [-3, 3];
    return <Knob key={path} label={label} unit={unit} title={`${help}\n${path}`} value={v} min={lo} max={hi} step={(hi - lo) / 120} onCommit={(x) => props.onApply(path, Math.round(x * 100) / 100)} />;
  };
  const classRows = CLASS_LABELS.filter(([c]) => props.classes.includes(c)).map(([c, label]): [string, string, string, string] => [
    `personas.class_shift.${c}.aggressiveness`,
    label,
    "SD",
    `Selfishness of ${label.toLowerCase()} on top of everyone's, in standard deviations.`,
  ]);

  return (
    <div className="traffic">
      <p className="muted small">
        Selfishness is the driver&apos;s aggressiveness κ. A selfish driver keeps smaller side gaps, follows closer, accelerates harder, accepts shorter gaps at junctions
        and, when rule-breaking is on, breaks rules more. 0 is the guide&apos;s average; ±1 is one standard deviation. Changes apply to vehicles that enter from now on.
      </p>
      <button onClick={props.onColourBySelfishness}>Colour vehicles by selfishness</button>
      <div className="section-title">Selfishness</div>
      {POPULATION.map(row)}
      <div className="section-title">Selfishness by vehicle class</div>
      {classRows.map(row)}
      <div className="section-title">What selfishness does</div>
      {LINKS.map(row)}
      <p className="muted small">Hover a slider for its register entry. Every change is in the Changes tab, with undo.</p>
    </div>
  );
}
