# ADR-0001: Rust core, Python research API, web viewer

- **Status:** accepted (user choice, 2026-11-09)
- **Date:** 2026-11-09
- **Phase / task:** plan

## Context
Research needs many runs (Morris and Sobol sweeps, replications, calibration) on a laptop and on HPC.
It also needs research-ready output, a live and replay web view with parameter tweaking, and strong
analytics. The code will be written mostly with AI, and the user will read and extend it.

## Options
1. **Rust core + Python API + TS web viewer.** Fast and memory-safe; static binaries for HPC; the
   Python ecosystem for analysis.
2. Pure Python with Numba or JAX. One language, but branch-heavy agent logic is slow and hard to keep
   fast.
3. C++ core + Python. Equally fast, but undefined behaviour and memory bugs are riskier with
   AI-written code.
4. Julia. Fast and scientific, but a smaller ecosystem for web and GIS, and start-up cost per run.

## Decision
Option 1:

- the core is in a Cargo workspace;
- PyO3 + maturin for Python;
- axum WebSocket server + React/deck.gl viewer;
- one schema (serde/schemars) generates the Python and TS types.

## Consequences
- Two toolchains (Rust and Python) plus Node for the web, managed by CI.
- The Python side never re-implements model logic; it orchestrates and analyses.
- HPC runs use the static CLI, so clusters need no Rust toolchain.
