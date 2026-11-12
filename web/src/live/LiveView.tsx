/** Live view (plan P4.1–P4.6): start a session, watch it, steer it, inspect it. */
import { useEffect, useMemo, useRef, useState } from "react";
import { api, type ScenarioListing } from "../api";
import { Dashboards, classOrder } from "../components/Dashboards";
import { Inspector } from "../components/Inspector";
import { KpiStrip } from "../components/KpiStrip";
import { Legend } from "../components/Legend";
import { ChangeHistory, ParamPanel } from "../components/ParamPanel";
import { KMH_PER_MS, VEH_KM_PER_VEH_M, clock, fmt } from "../format";
import type { ParamValue } from "../paramSchema";
import type { ColourBy } from "../protocol.gen";
import { MapView } from "../render/MapView";
import { SCALES } from "../render/scales";
import { liveAlpha } from "../render/interpolate";
import { useStore } from "../store";
import { type Chrome, groupOfClass } from "../theme";
import { LiveClient } from "./client";

const SPEEDS = [0.5, 1, 2, 5, 10, 30, 0];

export function LiveView({ theme }: { theme: Chrome }) {
  const [scenarios, setScenarios] = useState<ScenarioListing[]>([]);
  const [file, setFile] = useState("corridor_2km.yaml");
  const [seed, setSeed] = useState(1);
  const [window_, setWindow] = useState(30);
  const [client, setClient] = useState<LiveClient | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    api
      .scenarios()
      .then((s) => {
        setScenarios(s);
        if (s.length && !s.some((x) => x.file === file)) setFile(s[0].file);
      })
      .catch((e) => setError(`Cannot reach marg serve: ${e}`));
  }, []);
  useEffect(() => () => client?.close(), [client]);

  const start = async () => {
    setError(null);
    try {
      client?.close();
      const s = await api.createSession({ scenario: file, seed, live_window_s: window_ });
      setClient(new LiveClient(s.id, s.ws));
    } catch (e) {
      setError(String(e));
    }
  };

  return (
    <div className="view">
      <section className="toolbar">
        <label>
          Scenario{" "}
          <select value={file} onChange={(e) => setFile(e.target.value)}>
            {scenarios.map((s) => (
              <option key={s.file} value={s.file}>
                {s.name} ({Math.round(s.duration_s / 60)} min)
              </option>
            ))}
          </select>
        </label>
        <label>
          Seed <input type="number" value={seed} min={0} onChange={(e) => setSeed(Number(e.target.value))} style={{ width: 70 }} />
        </label>
        <label title="Interval of the live detector windows behind the dashboards">
          Window (s) <input type="number" value={window_} min={1} onChange={(e) => setWindow(Number(e.target.value))} style={{ width: 60 }} />
        </label>
        <button className="primary" onClick={start}>
          {client ? "Restart" : "Start session"}
        </button>
        {error && <span className="error">{error}</span>}
      </section>
      {client ? <Session key={client.sessionId} client={client} theme={theme} /> : <p className="muted">Pick a scenario and start a session. Live sessions run on the server, one thread each.</p>}
    </div>
  );
}

function Session({ client, theme }: { client: LiveClient; theme: Chrome }) {
  const st = useStore(client.state);
  const [colour, setColour] = useState<ColourBy>("class");
  const [trails, setTrails] = useState(false);
  const [tab, setTab] = useState<"inspector" | "params" | "changes" | "log">("params");
  const [tsRibbon, setTsRibbon] = useState<string>("");
  const [renderFps, setRenderFps] = useState(0);
  const [stepN, setStepN] = useState(10);
  const [bpTime, setBpTime] = useState("");
  const [obj, setObj] = useState({ s: 500, y: 0, length: 4.5, width: 1.8 });
  const objectCount = useRef(0);
  const selected = st.inspector?.found === false ? null : st.inspector ? BigInt(st.inspector.id) : null;
  const hello = st.hello;
  const status = st.status;

  useEffect(() => {
    if (hello && !tsRibbon && hello.ribbons.length) {
      setTsRibbon(hello.ribbons[0].id);
      client.send({ cmd: "view", ts_ribbon: hello.ribbons[0].id });
    }
  }, [hello]);

  const frameStore = client.frames;
  const display = useRef(() => {
    const f = frameStore.get();
    return { prev: f.prev, cur: f.cur, alpha: liveAlpha(f.prevAt, f.curAt, performance.now(), f.prev, f.cur) };
  });

  const kpis = useMemo(() => {
    const m = new Map<string, number | null>();
    for (const k of st.kpi?.kpis ?? []) m.set(k.name, k.value ?? null);
    // Density: the latest closed window of the first area detector.
    const areaRows = st.windows.filter((w) => w.kind === "area");
    if (areaRows.length) {
      const last = Math.max(...areaRows.map((w) => w.t_start));
      const id = areaRows[0].detector_id;
      const d = areaRows.filter((w) => w.t_start === last && w.detector_id === id).reduce((a, w) => a + (w.density ?? 0), 0);
      m.set("density", d * VEH_KM_PER_VEH_M);
    }
    return m;
  }, [st.kpi, st.windows]);

  const markers = useMemo(() => st.history.map((c) => c.t), [st.history]);
  const speedHists = useMemo(
    () =>
      [...(st.kpi?.speed_hist ?? [])]
        .sort((a, b) => classOrder(a.class, b.class))
        .map((h) => ({ label: h.class.replace(/_/g, " "), colour: theme.series[groupOfClass(h.class)], x0: 0, bin: h.bin_width * KMH_PER_MS, counts: h.counts })),
    [st.kpi, theme],
  );
  const ribbonLen = hello?.ribbons.find((r) => r.id === tsRibbon)?.length ?? 0;
  const ts = st.ts.t.length ? { ribbon: st.ts.ribbon, length: ribbonLen, t: st.ts.t, s: st.ts.s, v: st.ts.v } : undefined;

  if (!hello) return <p className="muted">Connecting… {st.error && <span className="error">{st.error}</span>}</p>;
  const scale = SCALES[colour] ?? SCALES.class;
  const mode = hello.colour_modes.find((m) => m.key === colour);
  const send = client.send.bind(client);
  const apply = (path: string, value: ParamValue, resample: boolean) => send({ cmd: "set_param", path, value, resample });
  const playing = status?.playing ?? false;

  return (
    <>
      <section className="toolbar">
        <button className="primary" onClick={() => send({ cmd: playing ? "pause" : "play" })} disabled={status?.finished}>
          {playing ? "Pause" : "Play"}
        </button>
        <button onClick={() => send({ cmd: "step", n: 1 })}>Step</button>
        <span className="group">
          <input type="number" min={1} value={stepN} onChange={(e) => setStepN(Number(e.target.value))} style={{ width: 64 }} aria-label="steps" />
          <button onClick={() => send({ cmd: "step", n: stepN })}>Step n</button>
        </span>
        <label>
          Speed{" "}
          <select value={status?.speed ?? 1} onChange={(e) => send({ cmd: "speed", x: Number(e.target.value) })}>
            {SPEEDS.map((s) => (
              <option key={s} value={s}>
                {s === 0 ? "max" : `${s}×`}
              </option>
            ))}
          </select>
        </label>
        <span className="clock" title="Simulated time">
          {clock(status?.t ?? 0)} / {clock(hello.duration_s)}
        </span>
        <span className="muted small">
          {status?.agents ?? 0} agents · {fmt(status?.real_time_factor ?? 0, 1)}× real time
        </span>
        <span className="group" title="Pause automatically">
          <button onClick={() => send({ cmd: "breakpoint", on: { on: "event", kind: "collision_guard" } })}>Break on collision guard</button>
          <input placeholder="t (s)" value={bpTime} onChange={(e) => setBpTime(e.target.value)} style={{ width: 64 }} aria-label="breakpoint time" />
          <button disabled={!Number.isFinite(Number(bpTime)) || bpTime === ""} onClick={() => send({ cmd: "breakpoint", on: { on: "time", t: Number(bpTime) } })}>
            Break at t
          </button>
          {(status?.breakpoints.length ?? 0) > 0 && <button onClick={() => send({ cmd: "clear_breakpoints" })}>Clear ({status?.breakpoints.length})</button>}
        </span>
        <span className="group" title="Put a road object on the selected ribbon (station s, lateral y, length, width, in metres)">
          {(["s", "y", "length", "width"] as const).map((k) => (
            <input key={k} type="number" step="any" value={obj[k]} onChange={(e) => setObj({ ...obj, [k]: Number(e.target.value) })} style={{ width: 58 }} aria-label={`object ${k}`} title={k} />
          ))}
          <button onClick={() => send({ cmd: "add_object", object: { id: `live-${++objectCount.current}`, ribbon: tsRibbon, ...obj } })}>Add object</button>
          {st.objects.some((o) => o.present.some(([, b]) => b == null)) && (
            <button onClick={() => { const last = [...st.objects].reverse().find((o) => o.present.some(([, b]) => b == null)); if (last) send({ cmd: "remove_object", id: last.id }); }}>Remove last</button>
          )}
        </span>
        <button onClick={() => send({ cmd: "save" })} title="Write the session folder (scenario, seed, command log) and replay it headless">
          Save session
        </button>
      </section>
      {st.breakpoint && (
        <div className="notice">
          Paused at a breakpoint: {st.breakpoint.reason} at {clock(st.breakpoint.t)}.
        </div>
      )}
      {st.saved && (
        <div className={`notice ${st.saved.replay_matches ? "ok" : "error"}`}>
          Saved to runs/{st.saved.path}. Headless replay {st.saved.replay_matches ? "reproduced" : "did NOT reproduce"} the output hash ({st.saved.output_hash.slice(0, 12)}…) after{" "}
          {st.saved.steps} steps. <a href={`#replay?run=${encodeURIComponent(st.saved.path)}`}>Open in replay</a>
        </div>
      )}
      {st.error && <div className="notice error">{st.error}</div>}
      <KpiStrip values={kpis} note={`Run ${status?.run_hash ?? hello.initial_run_hash} · seed ${hello.seed} · window ${hello.live_window_s} s`} />
      <div className="main">
        <div className="map-col">
          <div className="map-wrap">
            <MapView
              ribbons={hello.ribbons}
              detectors={hello.detectors}
              objects={st.objects.filter((o) => o.present.some(([a, b]) => (a ?? -Infinity) <= (status?.t ?? 0) && (status?.t ?? 0) < (b ?? Infinity)))}
              bounds={hello.bounds}
              classes={hello.classes}
              scale={scale}
              sources={[{ id: "live", display: display.current }]}
              selected={selected}
              trails={trails}
              onPick={(id) => {
                if (id !== null) {
                  send({ cmd: "select_agent", id: id.toString() });
                  setTab("inspector");
                }
              }}
              onViewport={(vp) => send({ cmd: "view", viewport: vp })}
              onRenderRate={setRenderFps}
              hoverLabel={(_s, i, b) => `${hello.classes[b.cls[i]]?.replace(/_/g, " ")} · agent ${b.id[i]}${scale.kind !== "categorical" ? ` · ${fmt(b.value[i] * scale.factor, 1)} ${scale.unit}` : ""}`}
            />
            <Legend scale={{ kind: scale.kind, max: Math.round(scale.max * scale.factor) }} label={mode?.label ?? ""} unit={scale.unit} />
            <div className="map-stats small">
              {renderFps.toFixed(0)} fps drawn · {st.rate.fps.toFixed(0)} frames/s received · {st.rate.mbps.toFixed(1)} MB/s
            </div>
          </div>
          <div className="toolbar">
            <label>
              Colour by{" "}
              <select value={colour} onChange={(e) => {
                const c = e.target.value as ColourBy;
                setColour(c);
                send({ cmd: "view", colour: c });
              }}>
                {hello.colour_modes.map((m) => (
                  <option key={m.key} value={m.key} disabled={!m.available}>
                    {m.label}
                    {m.available ? "" : ` (${m.note})`}
                  </option>
                ))}
              </select>
            </label>
            <label>
              <input type="checkbox" checked={trails} onChange={(e) => setTrails(e.target.checked)} /> Trails
            </label>
            <label>
              Time–space ribbon{" "}
              <select
                value={tsRibbon}
                onChange={(e) => {
                  setTsRibbon(e.target.value);
                  client.resetTimeSpace(e.target.value);
                  send({ cmd: "view", ts_ribbon: e.target.value });
                }}
              >
                {hello.ribbons.map((r) => (
                  <option key={r.id}>{r.id}</option>
                ))}
              </select>
            </label>
          </div>
        </div>
        <aside className="side">
          <nav className="tabs">
            {(["params", "inspector", "changes", "log"] as const).map((t) => (
              <button key={t} className={tab === t ? "active" : ""} onClick={() => setTab(t)}>
                {{ params: "Parameters", inspector: "Inspector", changes: `Changes (${st.history.length})`, log: "Command log" }[t]}
              </button>
            ))}
          </nav>
          <div className="tab-body">
            {tab === "params" && <ParamPanel overrides={hello.scenario.overrides as Record<string, ParamValue> | undefined} history={st.history} onApply={apply} onUndo={() => send({ cmd: "undo" })} />}
            {tab === "inspector" && <Inspector ins={st.inspector} onClose={() => send({ cmd: "select_agent", id: null })} onSelect={(id) => send({ cmd: "select_agent", id })} />}
            {tab === "changes" && <ChangeHistory history={st.history} onUndo={() => send({ cmd: "undo" })} />}
            {tab === "log" && (
              <ul className="log small">
                {st.acks.map((a, k) => (
                  <li key={k} className={a.ok ? "" : "error"}>
                    #{a.seq ?? "–"} {a.cmd}: {a.message}
                  </li>
                ))}
              </ul>
            )}
          </div>
        </aside>
      </div>
      <Dashboards rows={st.windows} theme={theme} warmup={hello.warmup_s} throughputDetector={hello.scenario.kpi?.throughput_detector} markers={markers} speedHists={speedHists} ts={ts} />
    </>
  );
}
