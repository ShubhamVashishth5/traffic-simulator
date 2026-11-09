# P4 · Web viewer v1 (live + replay + compare)

**Goal:** watch the simulation live in the browser, tweak parameters while it runs and see the effect
on the dashboards, replay any saved run, and compare twin runs side by side. Live sessions must stay
reproducible.

- **Depends on:** P2 (live), P3 (replay of experiments, compare).
- **Read first:**
  - [02-architecture.md](../02-architecture.md) §8–9;
  - [04-data-outputs-analytics.md](../04-data-outputs-analytics.md) §8;
  - [01-feature-survey.md](../01-feature-survey.md) features 8–13.

## Tasks

### P4.1 Live server (`marg-server`)
- **Do:**
  - axum + tokio, with one sim thread per session and a command channel.
  - Endpoints: `POST /sessions` (scenario, seed) and `GET /sessions/:id/ws`.
  - Commands: play, pause, step(n), speed(×), set_param(path, value), resample(path),
    select_agent(id), breakpoint(event filter), toggle_signal_state(junction, state) (stubbed until
    P8), add/remove_object (stubbed until P6).
  - Every command is stamped with sim time and logged to `commands.parquet`.
- **Done when:** a recorded live session replayed headless from scenario + seed + command log gives
  the same output hash.

### P4.2 Frame protocol
- **Do:**
  - Binary Arrow IPC frames: id, x, y, heading, length, width, class, colour value.
  - Frames are decimated to the viewer fps and culled to the viewport.
  - KPI frames go out at 1 Hz; agent-inspector frames go out on demand.
  - Protocol version in the handshake.
- **Done when:** 20,000 agents stream at 30 fps on localhost under 30 MB/s (benchmark recorded).

### P4.3 Renderer
- **Do:**
  - deck.gl with a custom instanced layer for oriented rectangles and a ribbon and strip layer.
  - Metric (orthographic) view for corridors; MapLibre basemap optional.
  - Colour by class, persona, κ, speed, violation state or zone; legend; trails.
- **Done when:** 60 fps with 20,000 agents on a mid-range laptop GPU (manual check, numbers recorded
  in the PR).

### P4.4 Controls and inspector
- **Do:**
  - Play, pause, step and time warp; breakpoints on event types.
  - Click an agent → a panel with class, persona, traits, drawn parameters, current neighbours,
    current state; after P8, also the logit breakdown of its latest decision.
- **Done when:** selecting an agent shows values identical to `agents.parquet` for that run.

### P4.5 Schema-driven parameter panel
- **Do:**
  - Build the panel from the register JSON Schema, grouped like the register.
  - Show unit, range, grade badge (A/B/C) and role badge.
  - Only `live: true` entries are editable; others are read-only with an explanation.
  - "Apply to new agents" vs "resample existing".
  - A per-session change history with undo.
- **Done when:** editing `v0` for cars changes the live fundamental-diagram scatter within seconds,
  and the change appears in the command log.

### P4.6 Live dashboards
- **Do:**
  - KPI strip: throughput, mean and p95 delay, density, violations, conflicts, gridlock.
  - uPlot time series; per-class speed and lateral histograms; fundamental-diagram scatter; a
    time–space diagram for a chosen ribbon (ECharts).
- **Done when:** the dashboard values match the observers' output for the same interval.

### P4.7 Replay mode
- **Do:** open a run folder (file picker or URL); DuckDB-WASM over the Parquet; a time scrubber; SQL
  filter box (e.g. `persona = 'delivery_rider'`); the same dashboards computed from the stored
  tables; heatmaps.
- **Done when:** a 1 h corridor run with 1 Hz trajectories opens and scrubs smoothly.

### P4.8 Twin compare
- **Do:** load two arms of an experiment (same seed); synchronised time; side-by-side or overlay;
  a KPI difference strip; highlight agents whose trip differs most.
- **Done when:** the cooperative vs observed arms from P3.3 show matching agents (the same ID in the
  same place at spawn) and diverging trajectories.

## Milestone check
A 3-minute screen recording (or a GIF in the PR): change the car desired-speed mean live and watch
the fundamental diagram respond; then open a P3 experiment run in replay and filter by class.

## Out of scope
Scenario editing (P12).
