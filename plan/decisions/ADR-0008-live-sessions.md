# ADR-0008: Live sessions: when commands act, what they change, and run identity

- **Status:** proposed (implemented in P4.1, P4.5)
- **Date:** 2026-11-09
- **Phase / task:** P4.1, P4.5

## Context
The live viewer changes parameters while a run is going (plan/02 §8–9). Plan/02 §9 fixes the
broad rule: population-level parameters change immediately; per-agent drawn parameters apply to
new arrivals, or to everyone with "resample". Four details were not settled:

1. **When** a command acts, so that a session replays bitwise (ADR-0003).
2. Which agents count as **"new"**: arrivals already waiting in an entry queue have drawn their
   parameters at their first insertion attempt.
3. What **"resample"** draws: a fresh number, or the agent's own random number through the new
   distribution.
4. The **run identity** of a session whose parameters changed mid-run (ADR-0004).

None of this changes the traffic model; it decides how the tool behaves, which is why it is an
ADR and not a modelling choice.

## Options
1. **Timing.** (a) Stamp commands with wall time and replay by time: not reproducible. (b) Stamp
   with the **number of steps completed**, apply between steps. Chosen.
2. **New agents.** (a) Agents whose draws are made after the change, keeping queued arrivals'
   cached draws: depends on when insertion was first attempted, invisible to the user. (b) **Every
   agent that has not entered yet**: queued arrivals drop their cached draws and draw again.
   Chosen.
3. **Resample.** (a) Fresh random numbers (a new counter block): the agent jumps to an unrelated
   value. (b) **The agent's own block, through the new distribution:** each agent keeps its
   quantile, so raising a mean moves every agent up, and the result equals what that agent would
   have drawn had the new distribution been in force from the start (common random numbers).
   Chosen.
4. **Identity.** (a) The run hash of the initial spec only: two different sessions share a hash.
   (b) **blake3 of the initial run hash and every accepted state-changing command (step, name,
   arguments)**, and exactly the initial run hash when no command changed the run. Chosen: an
   untouched live session is the same run as `marg run`.

## Decision
- Every command is stamped with `step` (steps completed) and `t`, and logged to
  `commands.parquet` with its arguments, outcome and wall-clock time. It acts before step
  `step + 1`.
- Only `set_param` and `resample` change the simulation. `set_param` is the resolver's `live`
  layer and accepts register entries with `live: true` only, within their range; `fix` entries
  stay protected.
- After `set_param`, the core rebuilds its parameters from the spec. Population-level values act
  on every agent from the next step. Per-agent draws apply to every agent that has not entered
  (queued arrivals re-draw from the same random numbers).
- `resample(path)` re-draws that parameter for agents in the network from block `k · 2³²` of
  the agent's stream (the block it was drawn from at entry), through the current distribution.
  `v0` keeps the agent's median-band bonus, which is stored per agent (`AgentStore::v0_bonus`).
  Vehicle dimensions are refused (a vehicle cannot change size on the road), as are paths that
  are not per-agent draws.
- A session folder holds `scenario.yaml`, a register snapshot (`params/`), the initial
  `spec.resolved.yaml` and `params.parquet`, `commands.parquet` and `manifest.json` (with
  `live.initial_run_hash`). `marg replay` re-runs it headless for the same number of steps,
  checks the output hash, and writes the output tables.
- Run hash: as option 4(b).

## Consequences
- `marg replay` reproduces sessions bitwise, for any thread count (tested in `marg-io` and
  `marg-server`).
- Live sessions get the per-step `kpis_now` and a second detector set at the live window
  (`enable_live_detectors`); headless runs pay nothing (one `Option` check per step; the core
  benchmarks did not change).
- The parameter panel offers "resample existing" only for distribution-valued entries; the core is
  the authority and refuses the rest with a reason.
- Changing a `live` entry that no module reads yet (personas, violations, P5 lateral
  parameters) is accepted and logged, and the acknowledgement says it has no effect yet.
