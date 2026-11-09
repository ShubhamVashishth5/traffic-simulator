# Viewer protocol (version 1)

`marg serve` (crate `marg-server`) serves the web viewer, its live sessions and run folders. The
message types are Rust types in `crates/marg-server/src/protocol.rs`; their JSON Schema
(`schema/viewer_protocol.schema.json`) generates the TypeScript types in
`web/src/protocol.gen.ts` (`scripts/gen-schema.sh`). Bump `PROTOCOL_VERSION` on any change.

## HTTP

| Method and path | Body / reply |
| --- | --- |
| `GET /version` | `{protocol, marg_version}` |
| `GET /register` | `{files: RegisterFile[]}`: the register as in `params/base/`, grouped |
| `GET /scenarios` | `[{file, name, description, duration_s}]` from the scenarios directory |
| `POST /sessions` | `{scenario: "<file>" \| yaml: "<text>", seed, threads?, live_window_s?}` → `{id, ws, scenario, initial_run_hash}` |
| `GET /sessions`, `DELETE /sessions/{id}` | list and stop sessions |
| `GET /sessions/{id}/ws?protocol=1` | the session's WebSocket; other versions get HTTP 400 |
| `GET /runs` | `{root, runs: [...], experiments: [...]}`: run folders (with a `manifest.json`) and experiments (with a `runs.jsonl`) under the runs directory |
| `GET /files/{path}` | files under the runs directory, with HTTP range requests (DuckDB-WASM reads Parquet over them) |
| anything else | the built web app (`web/dist`) |

## WebSocket

**Client → server:** JSON text, tagged by `cmd`. Every command except `view` is stamped with the
step it acts before and logged to `commands.parquet` (ADR-0008).

| `cmd` | Arguments | Effect |
| --- | --- | --- |
| `play`, `pause` | | |
| `step` | `n` | advance `n` steps, then pause |
| `speed` | `x` | simulated seconds per wall second; `0` = as fast as possible |
| `set_param` | `path, value, resample?` | the `live` layer; `value` in the register unit; `live: true` entries only |
| `resample` | `path` | re-draw a per-agent parameter for agents in the network |
| `undo` | | revert the latest parameter change (logged as a `set_param` with `undo_of`) |
| `select_agent` | `id` (decimal string) or `null` | inspector messages for this agent |
| `breakpoint` | `on: {on: "event", kind} \| {on: "time", t}` | pause at the first matching event, or at `t` |
| `clear_breakpoints` | | |
| `toggle_signal_state`, `add_object`, `remove_object` | | refused until P8 / P6 (logged) |
| `save` | | write `<runs>/live/<id>/` and replay it headless (`saved` message) |
| `view` | `viewport? [x0,y0,x1,y1], fps?, colour?, ts_ribbon?` | viewer settings; not logged, no effect on the run |

**Server → client, text:** JSON `{type, data}`.

| `type` | When | `data` |
| --- | --- | --- |
| `hello` | on connect | protocol, session, seed, scenario, initial run hash, `origin`, bounds, ribbon and detector shapes (relative to `origin`), classes, colour modes, live window, dt, duration |
| `status` | on change and once a second | playing, speed, t, step, finished, agents, real-time factor, breakpoints, run hash |
| `ack` | every command | `seq` (position in the command log), `cmd`, `ok`, `message` |
| `kpi` | once a wall second while playing; after steps | KPIs if the run ended now (`kpis.parquet` computation), live detector windows closed since the last message (`detectors.parquet` rows at the live interval), speed histograms by class, time–space samples of `ts_ribbon` |
| `inspector` | with `kpi`, and on selection | the agent's `agents.parquet` row as it would be now, its state, and every neighbour its leader scan considers (gap, lateral gap, clearance, ω, IDM acceleration) |
| `history` | after parameter changes | every accepted change: seq, step, t, path, before, after, undone |
| `breakpoint` | a breakpoint fired | reason, t, step, event |
| `saved` | after `save` | folder, run hash, live and replay output hashes, whether they match |
| `error` | | message |

**Server → client, binary:** one agent frame per message, an Arrow IPC stream (schema + one
record batch). Columns: `id` UInt64, `x`, `y` Float32 (metres from `hello.origin`), `heading`
Float32 (rad), `length`, `width` Float32 (m), `class` UInt8 (index into `hello.classes`),
`colour` Float32 (the value of the selected colour mode, SI). Schema metadata: `protocol`, `t`,
`step`, `total` (agents in the network; the frame may hold fewer), `colour`.

Frames are sent at most at the viewer's `fps` (default 30), only when the state changed, and
only for agents inside the viewport (plus 25 m). That is 33 bytes per agent: 20,000 agents at
30 fps is about 20 MB/s (measured, `reports/phases/P4.md`). At 1× real time and dt = 0.1 s the
simulation produces 10 states a second; the viewer interpolates positions between the last two
frames.
