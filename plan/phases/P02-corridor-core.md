# P2 · Corridor core (longitudinal, demand, outputs)

**Goal:** a fast headless simulator of a straight or curved corridor with mixed classes and lane-free
longitudinal following. It writes research-grade Parquet, has detectors, and meets the corridor
performance target. Lateral position stays fixed at the entry draw until P5.

- **Depends on:** P1.
- **Read first:**
  - guide §1, §2.1, §7.1–7.3;
  - [03-model-design.md](../03-model-design.md) §1, §2, §12;
  - [04-data-outputs-analytics.md](../04-data-outputs-analytics.md) §1–3.

## Tasks

### P2.1 Ribbons (`marg-net`)
- **Do:**
  - A polyline reference line with arc-length parametrisation.
  - Width profile `y_min(s)`, `y_max(s)` as a piecewise-linear table.
  - Conversion from `(s, y)` to world `(x, y)`, and heading.
  - Strips with access flags (carriageway, footpath, service road, opposite).
  - Corridor YAML format and `marg build-network` writing a binary network file.
- **Done when:** round-trip `(s, y) → world → (s, y)` error is under 1 mm on curved test ribbons.

### P2.2 Agent store
- **Do:**
  - Struct-of-arrays store with generational IDs, spawn and despawn, and periodic compaction.
  - Columns per architecture §2.
  - Per-agent draws at spawn from the register, using the AgentDraws RNG stream.
- **Done when:**
  - 10⁶ spawn/despawn cycles cause no leaks.
  - Draws are identical by agent ID regardless of spawn order (twin-safe).

### P2.3 Neighbour index
- **Do:**
  - Keep agents sorted by `s` per ribbon (incremental insertion sort).
  - Query leaders and followers within a look-ahead window and lateral band.
  - Cache the results per step.
- **Done when:** results match a brute-force reference on random configurations (property test), and
  a benchmark is recorded.

### P2.4 Longitudinal model
- **Do:** implement [03-model-design.md](../03-model-design.md) §2:
  - the IDM with the lane-free leader set and the ω weighting;
  - pair multiplier matrices;
  - the emergency braking cap;
  - optional reaction delay (off by default).
- **Done when** these micro-scenario tests pass:
  1. A single vehicle reaches `v0` within ±1%.
  2. Two vehicles reach the equilibrium gap `s0 + vT` ±2%.
  3. A stopped leader → the follower stops at ≥ `s0` with no overlap.
  4. A leader offset laterally by more than the clearance has no effect.
  5. Partial overlap gives an intermediate deceleration.

### P2.5 Integration and ribbon transitions
- **Do:** ballistic update, non-negative speed, ribbon-to-ribbon handover at the corridor's end, and
  despawn at the exit.
- **Done when:** energy and position sanity tests pass, and the vehicle count is conserved.

### P2.6 Demand
- **Do:**
  - Entries with a 15-minute flow profile and composition per interval.
  - Shifted-exponential arrivals per class; entry lateral position from a class distribution.
  - An insertion guard: delay the spawn if the space is occupied, and log the spawn delay.
  - Warm-up handling.
- **Done when:**
  - Realised counts per class are within a binomial CI of the targets over 10 seeds.
  - The headway distribution fits a shifted exponential (KS test).

### P2.7 Observers: detectors, agents, KPIs
- **Do:**
  - Screenline and area detectors (count, PCU, speeds, density, lateral histogram).
  - Agent trip summaries with delay against free flow.
  - The run KPI row.
  - The online implementation is in Rust; a Python cross-check recomputes the same values from
    trajectories.
- **Done when:** the Rust and Python metrics agree within 0.5% on the reference scenario.

### P2.8 Parquet writers
- **Do:**
  - In `marg-io`: Arrow builders, a background writer thread, zstd compression.
  - The tables `agents`, `kpis`, `detectors`, `events` and `trajectories` (rate and window
    configurable).
  - Data dictionary generation.
- **Done when:** the outputs open in Polars and DuckDB with the documented schema, and the data
  dictionary is committed.

### P2.9 CLI `marg run` and Python `marg.run()`
- **Do:**
  - `marg run scenario.yaml --seed N --out DIR [--threads K]`.
  - Python `marg.run(scenario, seed)` returns a `RunResult` with Polars frames (zero-copy Arrow).
- **Done when:** an example notebook in `examples/` runs end to end.

### P2.10 Rayon two-phase update and benchmarks
- **Do:**
  - Parallelise perceive and move.
  - Determinism across thread counts (P0.6 harness).
  - A reference corridor bench: 2 km, 1 h peak, ~3,000 agents at once.
- **Done when:** the [performance targets](../02-architecture.md#performance-targets) for the
  corridor are met, or the gap is measured and recorded as an issue with a profile (flamegraph).

## Milestone check
Stage-1 pattern: on a low-density early-morning scenario, the free-flow speed mean and SD per
sub-class match the Balakrishnan & Sivanandan values (validation target file), after fitting
`v0` distributions. Render the result in `reports/phases/P2.qmd`.

## Out of scope
Lateral movement, junctions, objects, personas.
