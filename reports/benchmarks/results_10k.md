# P3.6 benchmark: results for 10,000 runs

10000 runs (50 points x 2 arms x 100 replications), one `kpis.parquet` and one `manifest.json` per run folder, as a real sweep writes them. Runs come from the Function backend (stand-in simulator), so the numbers measure the results layer only.

Machine: x86_64, 4 cores, Python 3.13.18, Polars 2.1.0. Regenerate with `uv run python scripts/bench_results.py`.

| Step | Time (s) |
| --- | --- |
| expand + write run list (10000 runs, run hashes) | 3.91 |
| write runs (Function backend, 4 threads) | 13.45 |
| marg collect (validate every folder, write index.parquet) | 0.44 |
| load kpis for every run (Polars) | 1.09 |
| aggregate: CI per point x arm (Polars) | 0.01 |
| aggregate: paired differences per point (Polars) | 0.01 |
| load one arm, 5 points (index filter first) | 0.07 |
| load + aggregate in DuckDB (views over the same files) | 2.10 |

**Load and aggregate: 3.3 s in total** for 10000 runs.
