# Status

Last updated: 2026-11-09. Plan under review; implementation running ahead on branch `implementation-v1`.

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
| P2 | Milestone: free-flow speeds | partial | implementation-v1 | 3 of 10 sub-classes within ±2 km/h untuned; free vehicles move at their own v0, gap is selection bias from no overtaking (see `reports/phases/P2.md`). Re-run after P5 |
| P3 | P3.1–P3.9 | todo | | See card |
| P4 | P4.1–P4.8 | todo | | See card |
| P5 | P5.1 Lateral ADR (review gate) | todo | | |
| P5 | P5.2–P5.9 | todo | | |
| P6 | P6.1–P6.8 | todo | | |
| P7 | P7.1–P7.7 | todo | | |
| P8 | P8.1 Decision-timing ADR (review gate) | todo | | |
| P8 | P8.2–P8.8 | todo | | |
| P9 | P9.1–P9.7 | todo | | |
| P10 | P10.0 Data split (review gate) | todo | | Data-bound |
| P10 | P10.1–P10.5 | todo | | |
| P11 | P11.1–P11.5 | todo | | |
| P12 | P12.1–P12.4 | todo | | |

When a phase starts, expand its row into one row per task.
