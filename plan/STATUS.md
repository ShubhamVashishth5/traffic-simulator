# Status

Last updated: 2026-11-09. Plan under review; implementation running ahead (P0–P3 merged from `implementation-v1`; P4 on `ccr-88d9b767-74ihaq`).

Statuses: `todo` · `doing` · `review` (PR open) · `done` · `blocked` (say why in Notes).

| Phase | Task | Status | PR | Notes |
| --- | --- | --- | --- | --- |
| Plan | Review plan/ folder | review | — | Waiting for the user's review |
| P0 | P0.1 Cargo workspace | done | implementation-v1 | 7 crates; `marg-py` excluded from default members (extension module) |
| P0 | P0.2 Python package (maturin, uv) | done | implementation-v1 | `marg.run()`, `marg.version()`; 3 smoke tests |
| P0 | P0.3 Web skeleton | done | implementation-v1 | Vite + React + TS + Vitest; ESLint deferred (tsc strict covers it for now) |
| P0 | P0.4 Schema pipeline | done | implementation-v1 | `scripts/gen-schema.sh`; CI job fails on drift |
| P0 | P0.5 RNG streams | done | implementation-v1 | Philox4x32-10 instead of ChaCha8 (ADR-0003 updated); Random123 known-answer tests pass; ~4 ns per u64 |
| P0 | P0.6 Step loop + determinism harness | done | implementation-v1 | Toy agents; 1 vs 4 threads bitwise identical (Rust and Python tests) |
| P0 | P0.7 Benchmark harness | done | implementation-v1 | Criterion benches + `scripts/bench-compare.sh` (base vs head on one runner). CI job not yet observed running |
| P0 | P0.8 CI, CLAUDE.md | done | implementation-v1 | `.github/workflows/ci.yml` written, not yet observed on GitHub; pre-commit config deferred |
| P0 | P0.9 Native Windows builds and CI | todo | | Your answer to open question 6 (Windows PC). Add `x86_64-pc-windows-msvc` to CI tests and to the build artifacts (binary, wheel). WSL works until then |
| P1 | P1.1 Param entry type | done | implementation-v1 | `ParamEntry` with grade, role, stage, source, guide, register row, range, live, notes; 20 units with SI conversion. `Check` rows go to `validation/targets/`, not the register |
| P1 | P1.2 Distributions | done | implementation-v1 | 13 forms incl. three lognormal parameterisations, empirical, mixture; `libm`-only sampling. Wong & Goh: quantiles exact, mean 0.977 vs 1.02 s (criterion revised, see notes in the card) |
| P1 | P1.3 Transcribe register (§9) | done | implementation-v1 | 249 entries in `params/base/` (72 A, 52 B, 121 C) + `validation/targets/`. All 83 §9 rows traced (`params/guide_rows.yaml`, tested). No-number rows are `null`, not invented |
| P1 | P1.4 Scenario schema + resolver | done | implementation-v1 | Network, demand, detectors, outputs, KPIs, overrides; layers base ← scenario ← experiment with provenance; `fix` guarded |
| P1 | P1.5 Run identity, params.parquet | done | implementation-v1 | Canonical JSON (sorted keys) → blake3; order- and layer-independent (tested); `spec.resolved.yaml`, `params.parquet` |
| P1 | P1.6 Typed handles, no-magic-numbers check | done | implementation-v1 | `CoreParams` read once via access-tracked accessors; `tests/no_magic_numbers.rs` scans `marg-core/src/model/` |
| P1 | P1.7 `marg params` CLI | done | implementation-v1 | `list` (grade, role, stage, group filters), `explain` (SI value, provenance), `run --set path=value` |
| P2 | P2.1 Ribbons | done | implementation-v1 | Polyline + width profile, world↔(s, y) (round trip < 1 mm, property test), chains with cycle check. Binary `build-network` deferred: inline YAML is enough until OSM import (P11) |
| P2 | P2.2 Agent store | done | implementation-v1 | SoA, stable compaction; draws keyed by agent id (CRN tested) |
| P2 | P2.3 Neighbour index | done | implementation-v1 | Per-ribbon order sorted by (s, id), adaptive re-sort; leader scan with lateral "shadowing" (bug with long vehicles found and fixed) |
| P2 | P2.4 Longitudinal model | done | implementation-v1 | IDM with lane-free leader set and ω weighting; platoon equilibrium within 2% and lateral-separation tests pass |
| P2 | P2.5 Integration and transitions | done | implementation-v1 | Ballistic update, ribbon handover, exits; conservation tested; 0 collision-guard events on the reference corridor |
| P2 | P2.6 Demand | done | implementation-v1 | Shifted-exponential arrivals per class (KS test passes), composition within binomial error, virtual entry queue with insertion speed |
| P2 | P2.7 Observers | done | implementation-v1 | Screenlines (exact match to trajectory recomputation), Edie areas (< 0.5%), agent trips, 14 KPIs |
| P2 | P2.8 Parquet writers | done | implementation-v1 | agents, detectors, events, kpis, streamed trajectories (background thread); `docs/data-dictionary.md` generated and tested |
| P2 | P2.9 `marg run` and `marg.run()` | done | implementation-v1 | Python `RunResult` table accessors, `marg.analysis` |
| P2 | P2.10 Parallel move and benchmarks | done | implementation-v1 | Reference corridor: 1 h in 1.2 s on one core (~90 ns/agent-step, ~2,900× real time); deterministic across 1/3/4 threads; parallel path only above 1,024 agents |
| P2 | Milestone: free-flow speeds | partial | implementation-v1 | 2 of 10 sub-classes within ±2 km/h untuned (3 of 10 before P3.3 changed every agent's draws: pass/fail at the boundary moves with the draws). Free vehicles move at their own v0 (within 1.4 km/h); the gap is selection bias from no overtaking (see `reports/phases/P2.md`). Re-run after P5 |
| P3 | P3.1 Experiment model | done | implementation-v1 | `marg.Experiment` → `runs.jsonl` with run hashes computed by the core (`_core.Resolver`), `experiment.yaml`, `design.parquet`, register snapshot. Identical experiments give byte-identical run lists (tested). Folders are `scenario=/point=/arm=/rep=/run=<hash>` (plan 04 §1 updated). `marg batch` re-checks each hash and fails the run on mismatch |
| P3 | P3.2 Designs | done | implementation-v1 | Grid, FullFactorial, FractionalFactorial (generators or SALib Hadamard), LatinHypercube, Morris, Saltelli; counts and bounds tested; `fix` refused. Distribution-valued factors set the mean at fixed CV (ADR-0006, proposed). `Factor.relative` for screening boxes |
| P3 | P3.3 Twin arms, CRN | done | implementation-v1 | Arms share seeds; same agent ID → same draws (tested on agents.parquet). **Fixed a CRN bug:** draws shared one sequential stream, so a rejection-sampled v0 shifted T, s0, a, b; each draw now has its own counter block (ADR-0003 note; changes every agent's values for a seed). Paired-difference variance with CRN = 0.001 × independent on the test experiment. `marg.bundles.cooperative()` sets two new `scenario` switches; what "cooperative" means is pending (ADR-0007, needs your decision) |
| P3 | P3.4 Replications | done | implementation-v1 | Fixed or sequential (relative or absolute CI half-width, per point across all arms so pairs stay matched); `replications.json` records n, half-width and reason. Synthetic test: stops inside the Monte-Carlo 90% range of the stopping time; a point needing ~384 reps stops at `max` |
| P3 | P3.5 Backends | done | implementation-v1 | `Local` (pool of `marg batch`), `Slurm` (array + `afterany` collect, `container=` for Apptainer, `submit=False` dry run), `Function` (fake simulator). Runs written atomically (`*.partial-*` + rename). 200-run local test passes; SLURM tested with a fake `sbatch` that kills a task mid-chunk: resubmission completes with no duplicates. Static musl binary (`scripts/build-static.sh`, CI job; 7% slower than glibc). `deploy/marg.def` written but **not built here** (no Apptainer) |
| P3 | P3.6 Results dataset | done | implementation-v1 | `index.parquet` from `marg collect` (ok/failed/corrupt/stale/missing, orphans, partials); `ExperimentData.results/scan/duckdb`; `marg.stats` (CIs, paired differences, variance ratio, main effects); plots. 10,000 runs: load all KPIs 1.2 s (Polars), DuckDB 2.1 s, aggregation < 0.1 s (`reports/benchmarks/results_10k.md`) |
| P3 | P3.7 Sensitivity | done | implementation-v1 | `marg.sensitivity.morris/sobol/surrogate`. Ishigami through the full pipeline (5,120 runs, fake backend): S1 0.314/0.442/0.000, ST 0.566/0.442/0.240 (analytic 0.314/0.442/0, 0.558/0.442/0.244); GP surrogate R² 0.9995. Morris reports a CRN-aware noise floor of μ* from replications (tested against known noise) |
| P3 | P3.8 Staged calibration | done | implementation-v1 | `calibrate.Stage(k)`: refuses to start before earlier stages are frozen, never re-opens a frozen stage, writes only its own `fit` parameters (also enforced in the core loader). Frozen stages apply as a new `calibration` resolver layer (provenance `set_by = calibration`). Least squares and CMA-ES on standardised moments. Synthetic stage-1 recovery of three desired-speed means: all truths inside their 95% CIs. ABC-SMC (pyabc) not yet |
| P3 | P3.9 Reports v1 | done | implementation-v1 | Quarto templates for run, experiment, sensitivity, calibration-stage, parameter appendix; house style (validated palette); tables to CSV and LaTeX. Renders with Quarto when installed, otherwise pandoc (HTML) + headless Chrome (PDF). **Quarto itself was not installed in this environment**, so only the fallback route is tested |
| P3 | Milestone: Morris screen on capacity | partial | implementation-v1 | 58 P2 `fit` factors (±25% boxes), Morris r = 10, 2 replications under CRN: 1,180 runs, all ok (64 CPU-min). Sensitivity, experiment and parameter-appendix reports render (`reports/phases/P3.md`, `P3_sensitivity.pdf`). **7 factors clearly drive capacity or delay**: the shared standstill-gap multiplier, car `a` and `s0`, the clearance intercept, the shared time-gap multiplier, and the bicycle and motorcycle desired speeds (bicycles most likely a P2 artefact: no overtaking). The noise floor of μ* is 8–12% of the top effect at 2 replications, so 33 factors are unresolved and 18 within noise: **not yet enough to choose the stage-2 parameter set**. Re-run after P5 with more replications |
| P4 | P4.1 Live server | done | ccr-88d9b767-74ihaq | `marg serve` (axum + tokio, one sim thread per session). Commands are stamped with the step they act before and logged to `commands.parquet`. Save writes the session folder (scenario, register snapshot, seed, command log); `marg replay` reproduces the output hash bitwise (1 and 4 threads; tested over WebSocket). Live run hash and what `set_param`/`resample` do: ADR-0008 (proposed). Signal and object commands are refused until P8/P6 (logged) |
| P4 | P4.2 Frame protocol | done | ccr-88d9b767-74ihaq | Arrow IPC frames (33 B per agent), decimated to the viewer's fps, culled to the viewport, protocol version checked at connect. Types generated from Rust (`web/src/protocol.gen.ts`). **20,000+ agents: 30.0 frames/s at 20.7 MB/s (5× real time); 10 frames/s at 1× (every state)** |
| P4 | P4.3 Renderer | review | ccr-88d9b767-74ihaq | Custom instanced deck.gl layer, ribbons, median band, detectors, colour modes, legend, trails, picking. **60 fps check pending: no GPU in this environment** (to run on your RTX 4070; steps in `reports/phases/P4.md`) (software WebGL numbers in `reports/phases/P4.md`). No MapLibre (all scenarios are local metric until P11) |
| P4 | P4.4 Controls and inspector | done | ccr-88d9b767-74ihaq | Play, pause, step, time warp, event and time breakpoints. The inspector equals the replayed run's `agents.parquet` bit for bit (tested); the neighbour terms reproduce the next acceleration (tested). Persona and logit breakdown after P8 |
| P4 | P4.5 Parameter panel | done | ccr-88d9b767-74ihaq | Grouped like the register, with unit, range, grade and role badges. Distribution editors come from the register JSON Schema. Only `live: true` entries are editable. Apply to new agents or resample existing; history with undo. The car v0 change moves the live FD within seconds and is logged (milestone) |
| P4 | P4.6 Live dashboards | done | ccr-88d9b767-74ihaq | KPI strip (violations, conflicts and gridlock say "not modelled before P8/P5/P7"), uPlot time series, FD scatter, speed and lateral histograms by class, time–space diagram (canvas: ECharts large mode cannot colour points). Live windows equal the observers at the same interval, row for row (tested) |
| P4 | P4.7 Replay | done | ccr-88d9b767-74ihaq | DuckDB-WASM over HTTP range requests or a local folder; scrubber; SQL filter; the same dashboards from the stored tables; heatmaps. 1 h corridor with 1 Hz trajectories: opens in 4–5 s; seeking to a new minute takes a median of about 110 ms |
| P4 | P4.8 Twin compare | done | ccr-88d9b767-74ihaq | Side by side or overlay, KPI difference strip, spawn matching, most divergent trips (`scripts/p4_twins.py`). 100% of shared agents spawn at the same place and time. Observed vs `fast_cars` diverge (up to 1 km). **Observed vs cooperative are bitwise identical**: ADR-0007's switches are not read before P5/P8 |
| P4 | Milestone: live FD response + replay filter | done | ccr-88d9b767-74ihaq | `reports/phases/P4.md` (GIF recorded headless by `web/e2e/milestone.cjs`) |
| P5 | P5.1 Lateral ADR (review gate) | todo | | |
| P5 | P5.2–P5.9 | todo | | |
| P6 | P6.1–P6.8 | todo | | |
| P7 | P7.0 Roundabouts ADR (review gate) | todo | | In scope (your answer); ring of ribbons or junction box to be chosen at the gate (ADR-0009) |
| P7 | P7.1–P7.7 | todo | | |
| P8 | P8.1 Decision-timing ADR (review gate) | todo | | |
| P8 | P8.2–P8.8 | todo | | |
| P9 | P9.1–P9.7 | todo | | |
| P10 | P10.0 Data split (review gate) | todo | | Data-bound |
| P10 | P10.1–P10.5 | todo | | |
| P11 | P11.1–P11.5 | todo | | |
| P12 | P12.1–P12.4 | todo | | |

When a phase starts, expand its row into one row per task.
