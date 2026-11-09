"""P3.6 benchmark: build a 10,000-run experiment and time collect, load and
aggregate (plan/phases/P03-experiments-analytics.md, P3.6 "Done when").

The runs come from the Function backend (a stand-in simulator writing a
kpis.parquet with the real KPI columns), so the benchmark measures the results
layer, not the simulator. Writes reports/benchmarks/results_10k.md.

Usage: uv run python scripts/bench_results.py [--runs 10000] [--root /tmp/marg-bench]
"""

from __future__ import annotations

import argparse
import os
import platform
import shutil
import time
from pathlib import Path

import marg
import numpy as np
import polars as pl
from marg.design import Grid

REPO = Path(__file__).resolve().parents[1]
KPIS = [
    "throughput_veh_h",
    "delay_mean_s",
    "delay_p95_s",
    "trips_completed",
    "network_mean_speed_m_s",
    "spawned",
    "exited",
    "in_network_at_end",
    "waiting_to_enter_at_end",
    "spawn_delay_mean_s",
    "spawn_delay_max_s",
    "saturated_headways",
    "collision_guard_events",
    "agent_steps",
]


def fake(e: dict) -> dict[str, float]:
    rng = np.random.default_rng(e["seed"] * 1009 + e["point"])
    v = e["factors"]["vehicles.free_flow_speed.hatchback"]
    base = {k: float(x) for k, x in zip(KPIS, rng.normal(100, 10, len(KPIS)), strict=True)}
    base["throughput_veh_h"] = 2500 + 5 * v + rng.normal(0, 30)
    return base


def timed(label: str, f, results: list):
    t = time.perf_counter()
    out = f()
    dt = time.perf_counter() - t
    results.append((label, dt))
    print(f"{label}: {dt:.2f} s")
    return out


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--runs", type=int, default=10_000)
    ap.add_argument("--root", default="/tmp/marg-bench")
    a = ap.parse_args()
    root = Path(a.root)
    shutil.rmtree(root, ignore_errors=True)
    points, arms = 50, 2
    reps = a.runs // (points * arms)
    sc = marg.Scenario.load(REPO / "scenarios/corridor_2km.yaml")
    exp = marg.Experiment(
        "bench10k",
        sc,
        design=Grid({"vehicles.free_flow_speed.hatchback": list(np.linspace(40, 80, points))}),
        arms={"observed": {}, "cooperative": marg.bundles.cooperative()},
        replications=marg.Replications(n=reps),
        root=root,
        params_dir=REPO / "params/base",
    )
    res: list[tuple[str, float]] = []
    timed(f"expand + write run list ({points * arms * reps} runs, run hashes)", exp.write, res)
    timed(
        "write runs (Function backend, 4 threads)",
        lambda: marg.backends.Function(fake, workers=4).run(exp.dir, exp.run_list()),
        res,
    )
    rep = timed("marg collect (validate every folder, write index.parquet)", exp.collect, res)
    assert rep["ok"] == rep["total"] == points * arms * reps, rep
    data = marg.open_experiment(exp.dir)
    df = timed("load kpis for every run (Polars)", lambda: data.results("kpis"), res)
    assert df.height == rep["total"]
    timed(
        "aggregate: CI per point x arm (Polars)",
        lambda: marg.stats.summarize(df, "throughput_veh_h"),
        res,
    )
    timed(
        "aggregate: paired differences per point (Polars)",
        lambda: marg.stats.paired_summary(df, "throughput_veh_h", "cooperative", "observed"),
        res,
    )
    filtered = timed(
        "load one arm, 5 points (index filter first)",
        lambda: data.results("kpis", filters={"arm": "observed", "point": [0, 1, 2, 3, 4]}),
        res,
    )
    assert filtered.height == 5 * reps

    def duck():
        con = data.duckdb(["kpis"])
        return con.sql(
            "SELECT point, arm, count(*) n, avg(throughput_veh_h) m, stddev(throughput_veh_h) s "
            "FROM kpis GROUP BY point, arm ORDER BY point, arm"
        ).pl()

    d = timed("load + aggregate in DuckDB (views over the same files)", duck, res)
    assert d.height == points * arms
    total = sum(t for label, t in res if "load" in label or "aggregate" in label)
    lines = [
        "# P3.6 benchmark: results for 10,000 runs",
        "",
        f"{rep['total']} runs ({points} points x {arms} arms x {reps} replications), one "
        "`kpis.parquet` and one `manifest.json` per run folder, as a real sweep writes them. "
        "Runs come from the Function backend (stand-in simulator), so the numbers measure the "
        "results layer only.",
        "",
        f"Machine: {platform.machine()}, {os.cpu_count()} cores, Python {platform.python_version()}, "
        f"Polars {pl.__version__}. Regenerate with `uv run python scripts/bench_results.py`.",
        "",
        "| Step | Time (s) |",
        "| --- | --- |",
        *[f"| {label} | {t:.2f} |" for label, t in res],
        "",
        f"**Load and aggregate: {total:.1f} s in total** for {rep['total']} runs.",
        "",
    ]
    out = REPO / "reports/benchmarks/results_10k.md"
    out.write_text("\n".join(lines))
    print(f"wrote {out}")


if __name__ == "__main__":
    main()
