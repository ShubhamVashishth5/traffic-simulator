# ADR-0004: Parquet outputs, resolved register per run, run identity by hash

- **Status:** proposed
- **Date:** 2026-11-09
- **Phase / task:** plan (implemented in P1.5, P2.8, P3.6)

## Context
Outputs must be usable in research tools (Python, R, MATLAB, SQL) and scale to tens of thousands of
runs. They must also answer the reviewer question "which value did each parameter take, and how sure
are you?" (guide §8.1, §9).

## Options
1. XML or CSV. Large, untyped, slow (SUMO's own benchmark shows the FCD file is 5× smaller as
   Parquet).
2. HDF5. Good for arrays, weaker in SQL and browser tools.
3. **Parquet** with versioned schemas, a run folder with `manifest.json` and `params.parquet`, and
   Hive partitioning per experiment.

## Decision
Option 3:

- the run hash = `hash(resolved spec + seed + version)`;
- batch runners skip runs already done;
- every run carries its resolved register with grade, role and source.

## Consequences
- Sweeps are resumable and deduplicated.
- The web replay reads the same files through DuckDB-WASM.
- Schema changes need version bumps and data-dictionary regeneration.
