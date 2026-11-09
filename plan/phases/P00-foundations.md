# P0 · Foundations

**Goal:** a workspace where every later task can be built, tested, benchmarked and checked for
determinism automatically, so mistakes in AI-written changes get caught by CI rather than by review.

- **Depends on:** nothing.
- **Read first:** [02-architecture.md](../02-architecture.md) (stack, layout, quality gates),
  [06-ai-workflow.md](../06-ai-workflow.md).

## Tasks

### P0.1 Cargo workspace and crate skeletons
- **Do:**
  - Create the crates `marg-params`, `marg-net`, `marg-core`, `marg-io`, `marg-cli`, `marg-server`
    and `marg-py` under `crates/`, with the dependency direction
    `params ← net ← core ← io ← {cli, server, py}`.
  - Pin the toolchain in `rust-toolchain.toml`.
  - Set `#![forbid(unsafe_code)]` in all crates for now.
  - Shared lints: `clippy::pedantic` warnings allowed only where listed.
- **Done when:** `cargo build --workspace` and `cargo test --workspace` pass on a clean clone.

### P0.2 Python package via maturin + uv
- **Do:**
  - Add `pyproject.toml` (maturin backend, abi3, Python ≥ 3.11) and the `python/marg/__init__.py`
    package.
  - Expose `marg.version()` from Rust.
  - Set up uv-managed dev dependencies: pytest, polars, duckdb, pyarrow, ruff, mypy.
- **Done when:** `uv run maturin develop && uv run pytest` passes, and `python -c "import marg;
  print(marg.version())"` works.

### P0.3 Web skeleton
- **Do:** set up `web/` with Vite + React + TypeScript, ESLint, Vitest, and an empty app shell.
- **Done when:** `npm ci && npm run build && npm test` pass.

### P0.4 Schema pipeline (single source of truth)
- **Do:**
  1. Write a toy `RunSpec` struct in `marg-params` with `serde` and `schemars`.
  2. Add `marg schema export` to write `schema/run_spec.schema.json`.
  3. Generate pydantic models (`datamodel-code-generator`) into `python/marg/_schema.py`, and TS
     types (`json-schema-to-typescript`) into `web/src/schema.ts`.
  4. Add a CI step that regenerates all three and fails on any diff.
- **Done when:** changing a field in Rust without regenerating makes CI fail.

### P0.5 RNG streams
- **Do:**
  - In `marg-core::rng`, derive a stream from a key `(master_seed, StreamKind, entity_id)`, using
    ChaCha8 with a stream and word position.
  - `StreamKind` covers Demand, AgentDraws, Decisions, Environment, Objects and Pedestrians.
  - No global RNG anywhere.
- **Done when:**
  - A property test shows that draws for agent 7 are identical whether 10 or 10,000 agents exist.
  - Draws are identical across thread counts.

### P0.6 Empty step loop and determinism harness
- **Do:**
  - Build `Simulation::new(spec)` and `step()` with the phases from architecture §4, as no-ops over
    an agent store with one dummy kinematic agent kind.
  - Write `manifest.json` and an output hash.
  - Add a `determinism` test that runs the reference spec with 1 and 4 threads, twice each, and
    compares output hashes.
- **Done when:** the determinism test passes in CI.

### P0.7 Benchmark harness
- **Do:** add criterion benches under `benches/` and a CI job that stores the baseline from `main`
  and fails if a PR is more than 5% slower on the reference bench.
- **Done when:** the job runs on a PR and reports the delta.

### P0.8 CI and repo hygiene
- **Do:**
  - GitHub Actions: fmt, clippy, tests, pytest, web build, schema diff, determinism, benchmarks,
    wheel build (manylinux x86_64; macOS arm64 optional).
  - Pre-commit config.
  - `CLAUDE.md` at the repo root, holding the rules from [06-ai-workflow.md](../06-ai-workflow.md)
    §Rules.
  - `CONTRIBUTING.md` (short).
- **Done when:** a PR with a clippy warning fails, and a clean PR passes.

## Out of scope
Any traffic behaviour.

## Notes
The repo is MIT-licensed. Do not copy code from GPL projects (traffic-simulation.de, SimWrapper,
SiMTraM). Read them for ideas only.
