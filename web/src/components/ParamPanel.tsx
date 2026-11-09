/**
 * Schema-driven parameter panel (plan P4.5): every register entry, grouped like the register,
 * with unit, range, grade and role badges. Only `live: true` entries are editable; others say
 * why not. Distribution editors come from the register JSON Schema. A change applies to new
 * agents, or to everyone with "resample"; the change history has undo.
 */
import { useEffect, useMemo, useState } from "react";
import { api } from "../api";
import { type Param, type ParamValue, currentValue, describe, distForms, entryDescription, flatten, isDrawn, locationField, readOnlyReason } from "../paramSchema";
import type { ChangeView } from "../protocol.gen";
import type { RegisterFile } from "../register";

export interface ParamPanelProps {
  overrides?: Record<string, ParamValue>;
  history: ChangeView[];
  onApply: (path: string, value: ParamValue, resample: boolean) => void;
  onUndo: () => void;
}

function Badge({ kind, text, title }: { kind: string; text: string; title?: string }) {
  return (
    <span className={`badge badge-${kind}`} title={title}>
      {text}
    </span>
  );
}

function Editor({ p, value, onApply }: { p: Param; value: ParamValue | null; onApply: ParamPanelProps["onApply"] }) {
  const forms = useMemo(distForms, []);
  const [draft, setDraft] = useState<ParamValue | null>(value);
  useEffect(() => setDraft(value), [JSON.stringify(value)]);
  const e = p.entry;
  const [lo, hi] = e.range ?? [NaN, NaN];
  const loc = locationField(draft);
  const form = draft !== null && typeof draft === "object" ? forms.find((f) => f.key === (draft as { dist: string }).dist) : undefined;
  const set = (field: string, x: number) => setDraft((d) => (typeof d === "object" && d !== null ? ({ ...d, [field]: x } as ParamValue) : x));
  const drawn = isDrawn(draft);
  const changed = JSON.stringify(draft) !== JSON.stringify(value);
  const slider = (field: string | null) => {
    const cur = field ? ((draft as unknown as Record<string, number>)[field] as number) : (draft as number);
    if (!Number.isFinite(lo) || !Number.isFinite(hi) || cur === undefined) return null;
    return (
      <input
        type="range"
        min={lo}
        max={hi}
        step={(hi - lo) / 200}
        value={cur}
        aria-label={`${p.path} ${field ?? "value"}`}
        onChange={(ev) => set(field ?? "value", Number(ev.target.value))}
      />
    );
  };
  return (
    <div className="editor">
      {typeof draft === "number" || draft === null ? (
        <div className="row">
          {slider(null)}
          <input type="number" value={draft ?? ""} step="any" onChange={(ev) => setDraft(ev.target.value === "" ? null : Number(ev.target.value))} />
        </div>
      ) : form ? (
        <>
          {loc && <div className="row">{slider(loc)}</div>}
          <div className="fields">
            {form.fields.map((f) => (
              <label key={f}>
                {f}
                <input type="number" step="any" value={(draft as unknown as Record<string, number>)[f]} onChange={(ev) => set(f, Number(ev.target.value))} />
              </label>
            ))}
          </div>
          <div className="muted small">{form.description}</div>
        </>
      ) : (
        <div className="muted small">This distribution form is edited in the register or a scenario.</div>
      )}
      <div className="row">
        <button disabled={!changed || draft === null} onClick={() => draft !== null && onApply(p.path, draft, false)} title="New arrivals draw from the new value; agents already in the network keep theirs">
          Apply to new agents
        </button>
        {drawn && (
          <button disabled={draft === null} onClick={() => draft !== null && onApply(p.path, draft, true)} title="Also re-draw it now for every agent in the network, from each agent's own random numbers">
            Apply and resample existing
          </button>
        )}
        {changed && (
          <button className="link" onClick={() => setDraft(value)}>
            reset
          </button>
        )}
      </div>
    </div>
  );
}

export function ParamPanel(props: ParamPanelProps) {
  const [files, setFiles] = useState<RegisterFile[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [liveOnly, setLiveOnly] = useState(true);
  const [open, setOpen] = useState<string | null>("vehicles.free_flow_speed.hatchback");
  useEffect(() => {
    api
      .register()
      .then((r) => setFiles(r.files))
      .catch((e) => setError(String(e)));
  }, []);
  const params = useMemo(() => (files ? flatten(files) : []), [files]);
  const shown = params.filter((p) => (!liveOnly || p.entry.live) && (!query || p.path.includes(query)));
  const groups = [...new Set(shown.map((p) => p.group))];
  if (error) return <p className="error">Register: {error}</p>;
  if (!files) return <p className="muted">Loading the register…</p>;
  return (
    <div className="params">
      <div className="row">
        <input placeholder="filter, e.g. free_flow_speed.hatch" value={query} onChange={(e) => setQuery(e.target.value)} />
        <label className="small">
          <input type="checkbox" checked={liveOnly} onChange={(e) => setLiveOnly(e.target.checked)} /> live only
        </label>
      </div>
      <p className="muted small">
        {shown.length} of {params.length} parameters. {entryDescription("live")}
      </p>
      {groups.map((g) => {
        const sections = [...new Set(shown.filter((p) => p.group === g).map((p) => p.section))];
        return (
          <details key={g} open>
            <summary>
              <strong>{g}</strong>
              <span className="muted small"> {files.find((f) => f.group === g)?.description}</span>
            </summary>
            {sections.map((sec) => (
              <div key={sec} className="section">
                <div className="section-title">{sec}</div>
                {shown
                  .filter((p) => p.group === g && p.section === sec)
                  .map((p) => {
                    const v = currentValue(p, props.overrides, props.history);
                    const e = p.entry;
                    const isOpen = open === p.path;
                    const changedLive = props.history.some((c) => c.path === p.path);
                    return (
                      <div key={p.path} className={`param ${isOpen ? "open" : ""}`}>
                        <button className="param-head" onClick={() => setOpen(isOpen ? null : p.path)} aria-expanded={isOpen}>
                          <span className="param-name">{p.path.slice(g.length + 1)}</span>
                          <span className="param-value">
                            {describe(v)} <span className="muted">{e.unit}</span>
                            {changedLive && <Badge kind="live" text="changed" title="Changed in this session" />}
                          </span>
                          <span className="badges">
                            <Badge kind={`grade-${e.grade.replace("/", "")}`} text={e.grade} title={entryDescription("grade")} />
                            <Badge kind="role" text={e.stage !== undefined && e.stage !== null ? `fit/${e.stage}` : e.role} title={entryDescription("role")} />
                          </span>
                        </button>
                        {isOpen && (
                          <div className="param-body">
                            <div className="small">
                              {e.range && (
                                <span>
                                  range [{e.range[0]}, {e.range[1]}] {e.unit} ·{" "}
                                </span>
                              )}
                              guide {e.guide} · {e.source}
                            </div>
                            {e.notes && <div className="muted small">{e.notes}</div>}
                            {e.live ? <Editor p={p} value={v} onApply={props.onApply} /> : <div className="readonly small">{readOnlyReason(e)}</div>}
                          </div>
                        )}
                      </div>
                    );
                  })}
              </div>
            ))}
          </details>
        );
      })}
    </div>
  );
}

export function ChangeHistory({ history, onUndo }: { history: ChangeView[]; onUndo: () => void }) {
  const canUndo = history.some((c) => !c.undone);
  return (
    <div>
      <div className="row">
        <strong>Changes this session</strong>
        <button disabled={!canUndo} onClick={onUndo}>
          Undo latest
        </button>
      </div>
      {history.length === 0 ? (
        <p className="muted small">No parameter changes yet. Every change is a command in commands.parquet.</p>
      ) : (
        <table className="grid small">
          <thead>
            <tr>
              <th>#</th>
              <th>t (s)</th>
              <th>parameter</th>
              <th>before → after</th>
            </tr>
          </thead>
          <tbody>
            {[...history].reverse().map((c) => (
              <tr key={c.seq} className={c.undone ? "undone" : ""}>
                <td>{c.seq}</td>
                <td>{c.t.toFixed(1)}</td>
                <td>{c.path}</td>
                <td>
                  {describe((c.before ?? null) as ParamValue | null)} → {describe(c.after as ParamValue)}
                  {c.undone ? " (undone)" : ""}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}
