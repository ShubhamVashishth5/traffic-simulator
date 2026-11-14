# CLAUDE.md

Guidance for AI sessions working in this repository.

## Start here

- **Goal:** a general-purpose, realistic, highly tunable, CPU- and memory-efficient lane-free
  traffic simulator that anyone can use. Delhi (the parameter guide) is its first calibrated
  setting; the code must not assume one study, site or city.
- The design and roadmap are in `plan/` (start at `plan/README.md`).
- Work one task card at a time (`plan/phases/Pxx-*.md`). Track progress in `plan/STATUS.md`.
- The model's numbers come from the *Simulator Parameter Guide: Delhi Disordered Traffic*
  (full copy in `plan/reference/parameter-guide.md`), cited as "G§n".
- Modelling choices not settled in `plan/03-model-design.md` or an ADR in `plan/decisions/`:
  write an ADR draft with options and stop; do not choose silently.

## Commands

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
cargo test                                   # all Rust tests (excludes the Python extension crate)
cargo test --release -p marg-core determinis # determinism across thread counts
uv sync && uv run maturin develop --release && uv run pytest
uv run ruff check python
(cd web && npm ci && npm run build && npm test)
./scripts/gen-schema.sh                      # after changing any type in marg-params
cargo run -q -p marg-cli -- data-dictionary  # after changing any output table
cargo run --release -p marg-cli -- bench scenarios/corridor_2km.yaml   # ns per agent-step
uv run python scripts/p2_milestone.py        # regenerate reports/phases/P2.md
./scripts/bench-compare.sh origin/master HEAD
cargo run --release -p marg-cli -- serve     # web viewer on :8080 (build web/ first); --runs, --scenarios
cargo run --release -p marg-cli -- replay runs/live/<session>   # headless replay of a live session
cargo test --release -p marg-server --test server frame_rate -- --ignored --nocapture  # P4.2 bench
uv run python scripts/p4_twins.py --root runs  # twin arms with trajectories, for Compare
node web/e2e/milestone.cjs                     # P4 milestone recording (needs playwright + marg serve)
cargo test --release -p marg-core --test guard -- --ignored --nocapture   # P5.5: 10^4 random dense scenarios
uv run python scripts/p5_stage3_recovery.py    # P5.8 stage-3 screen + synthetic recovery (~15 min) -> reports/phases/P5_stage3.json
uv run python scripts/p5_milestone.py          # regenerate reports/phases/P5.md (reads P5_stage3.json)
node web/e2e/p5_seepage.cjs                    # P5 viewer recording (needs playwright + marg serve)
uv run python scripts/p6_milestone.py          # regenerate reports/phases/P6.md (93 runs, ~8 min)
uv run python scripts/p7_signal_check.py       # P7.3: saturation flow and discharge acceleration (~10 s) -> reports/phases/P7_signal.json
```

## Builds are expensive: batch them

A release build of the workspace takes minutes and every rebuild rewrites a cold cache.

- Design the whole change for a task (all files, tests, the measurement script) before editing,
  then write it in one pass. Don't rebuild after each small edit.
- Catch compile errors with `cargo check`; build in release once per batch, then run the tests
  once.
- Run sweeps (flows × seeds, the guard property test, benchmarks) once per batch, not after each
  tweak.
- Diagnose from the outputs of finished runs (events, trajectories) where you can. A diagnostic
  harness reads what to inspect from its arguments or environment, so it is compiled once, not
  once per hypothesis.
- Build another revision (a base commit for comparison) only when a comparison needs it.

## Rules

1. **No magic numbers.** Every behavioural constant comes from the register (`params/base/`),
   with grade, role, stage and source. A new parameter means a new register entry first.
2. **No capacity or flow multipliers, ever.** Road effects go only through width, speed caps, gap
   supply or incentives (G§6, ADR-0005). Published capacity losses are validation targets.
3. **Never tune toward a validation target directly.** Targets in `validation/targets/` are checks.
   If one is missed, report the gap; don't add a fudge factor.
4. **Respect stage freezing.** Don't edit frozen `calibration/stage_*.yaml` values or `Fix`
   parameters to make a later test pass.
5. **Determinism is sacred.** All randomness comes from `marg_core::rng::StreamRng` (counter-based
   Philox streams, ADR-0003). No global RNG, no `HashMap` iteration order in outputs, no unordered
   parallel reductions. The determinism tests must pass.
6. **Performance is a feature.** No more than 5% regression on the core benchmarks without an ADR.
   Hot-loop code stays allocation-free.
7. **Every mechanism ships with micro-scenario tests and an output.**
8. **Baseline is not violation.** Filtering, staggered following and moving between notional lanes
   are physics, never counted as violations (G§4.4).
9. **Explain decisions.** Every stochastic decision logs its features and logit terms.
10. **Schema changes regenerate** the Python and TS types (`scripts/gen-schema.sh`) and the data
    dictionary in the same change.
11. **Licence hygiene.** The repo is MIT. Don't copy code from GPL projects; cite papers for
    equations.
12. **Units.** SI inside the core (m, s, m/s). Other units appear only in the register and are
    converted by the resolver.
