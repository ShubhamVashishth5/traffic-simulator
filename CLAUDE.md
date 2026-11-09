# CLAUDE.md

Guidance for AI sessions working in this repository.

## Start here

- The design and roadmap are in `plan/` (start at `plan/README.md`).
- Work one task card at a time (`plan/phases/Pxx-*.md`). Track progress in `plan/STATUS.md`.
- The model's numbers come from the *Simulator Parameter Guide: Delhi Disordered Traffic*
  (Claude Doc linked in `plan/reference/parameter-guide-summary.md`), cited as "G§n".
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
```

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
