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
| P1 | P1.1 Param entry type | todo | | |
| P1 | P1.2 Distributions | todo | | |
| P1 | P1.3 Transcribe register (§9) | todo | | Needs the full guide (Claude Doc or exported copy) |
| P1 | P1.4 Scenario schema + resolver | todo | | |
| P1 | P1.5 Run identity, params.parquet | todo | | |
| P1 | P1.6 Typed handles, no-magic-numbers check | todo | | |
| P1 | P1.7 `marg params` CLI | todo | | |
| P2 | P2.1–P2.10 | todo | | See card |
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
