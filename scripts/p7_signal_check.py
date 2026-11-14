"""P7.3 checks at the signalised crossing (plan/phases/P07-junctions-splits.md).

Runs `scenarios/signal_crossing.yaml` (every turn, 700 veh/h per arm: a queue
stands at every green) with the shipped register, nothing tuned, and reports:

1. Saturation flow per approach, from `signal_cycles.parquet` (headways from
   the fourth queued vehicle on, HCM 2010 ch. 31), in veh/h and per metre of
   approach width.
2. Mean discharge acceleration by class against the check values of G§1
   (Ramireddy et al. 2021, `validation/targets/patterns.yaml`,
   `discharge_acceleration`, a check: never tuned towards). The guide says
   these are averages over a whole manoeuvre: here, for every vehicle that
   stood in the queue at green, the speed it gained divided by the time from
   moving off until it first stopped accelerating after its stop line.

Writes reports/phases/P7_signal.json and prints a summary.

Usage: uv run python scripts/p7_signal_check.py [--seeds 3] [--duration 900]
"""

from __future__ import annotations

import argparse
import json
import subprocess
import tempfile
from pathlib import Path

import polars as pl

ROOT = Path(__file__).resolve().parent.parent
MARG = ROOT / "target" / "release" / "marg"
SCENARIO = ROOT / "scenarios" / "signal_crossing.yaml"

# Classes of the check (G§1) and the simulator's classes in each.
GROUPS = {
    "two_wheeler": ["two_wheeler"],
    "three_wheeler": ["auto_rickshaw", "e_rickshaw"],
    "car": ["car"],
    "lcv": ["lcv"],
}


def run(seed: int, duration: int, out: Path) -> Path:
    text = SCENARIO.read_text()
    text = text.replace("duration_s: 1800", f"duration_s: {duration}")
    text = text.replace("warmup_s: 300", "warmup_s: 0")
    text += "outputs:\n  trajectories: {every_s: 0.1}\n"
    spec = out / f"scenario_{seed}.yaml"
    spec.write_text(text)
    folder = out / f"run_{seed}"
    subprocess.run(
        [str(MARG), "run", str(spec), "--seed", str(seed), "--out", str(folder)],
        check=True,
        capture_output=True,
    )
    return folder


def stop_speed(folder: Path) -> float:
    """Below this speed a vehicle stands (m/s): the run's register value."""
    params = pl.read_parquet(folder / "params.parquet")
    row = params.filter(pl.col("path") == "observers.stop.speed_below")
    return float(row["value_si"][0])


def discharge(folder: Path) -> pl.DataFrame:
    """Mean acceleration of each queued vehicle's discharge, with its class."""
    standing = stop_speed(folder)
    ev = pl.read_parquet(folder / "events.parquet").filter(pl.col("kind") == "stop_line_crossed")
    crossed = ev.with_columns(
        pl.col("attrs").str.json_path_match("$.queued").alias("queued")
    ).filter(pl.col("queued") == "true")
    tr = pl.read_parquet(folder / "trajectories.parquet").sort("agent_id", "t")
    agents = pl.read_parquet(folder / "agents.parquet").select("agent_id", "class")
    rows = []
    for agent_id, t_cross in crossed.select("agent_id", "t").iter_rows():
        own = tr.filter(pl.col("agent_id") == agent_id)
        before = own.filter((pl.col("t") <= t_cross) & (pl.col("v") < standing))
        if before.is_empty():
            continue
        t0 = before["t"].max()
        after = own.filter(pl.col("t") > t_cross)
        # The first time it stopped gaining speed after the line.
        done = after.filter(pl.col("a") <= 0.0)
        if done.is_empty():
            continue
        t1 = done["t"].min()
        span = own.filter((pl.col("t") >= t0) & (pl.col("t") <= t1))
        dv = span["v"][-1] - span["v"][0]
        dt = t1 - t0
        if dt > 0:
            rows.append((agent_id, dv / dt, dt))
    out = pl.DataFrame(rows, schema=["agent_id", "accel", "duration"], orient="row")
    return out.join(agents, on="agent_id")


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--seeds", type=int, default=3)
    ap.add_argument("--duration", type=int, default=900)
    args = ap.parse_args()
    targets = {
        "two_wheeler": 0.60,
        "three_wheeler": 0.43,
        "car": 0.49,
        "lcv": 0.46,
    }
    cycles, accels = [], []
    with tempfile.TemporaryDirectory() as tmp:
        for seed in range(1, args.seeds + 1):
            folder = run(seed, args.duration, Path(tmp))
            cycles.append(
                pl.read_parquet(folder / "signal_cycles.parquet").with_columns(
                    pl.lit(seed).alias("seed")
                )
            )
            accels.append(discharge(folder).with_columns(pl.lit(seed).alias("seed")))
    cyc = pl.concat(cycles).filter(pl.col("saturation_flow").is_not_null())
    sat = (
        cyc.group_by("arm")
        .agg(
            pl.len().alias("cycles"),
            pl.col("saturation_flow").median().alias("veh_per_h"),
            pl.col("saturation_flow_per_m").median().alias("veh_per_h_per_m"),
            pl.col("approach_width").first().alias("width_m"),
        )
        .sort("arm")
    )
    acc = pl.concat(accels)
    report = {"saturation_flow": sat.to_dicts(), "discharge_acceleration": {}}
    print("Saturation flow by approach (median over cycles):")
    print(sat)
    print("Mean discharge acceleration (m/s²): simulated / check (±20%)")
    for group, classes in GROUPS.items():
        sub = acc.filter(pl.col("class").is_in(classes))
        if sub.is_empty():
            continue
        mean = float(sub["accel"].mean())
        sd = float(sub["accel"].std() or 0.0)
        n = len(sub)
        target = targets[group]
        within = abs(mean - target) <= 0.2 * target
        report["discharge_acceleration"][group] = {
            "mean": mean,
            "sd": sd,
            "n": n,
            "check": target,
            "within_20pct": within,
            "mean_duration_s": float(sub["duration"].mean()),
        }
        print(
            f"  {group:14s} {mean:.2f} ± {sd / n**0.5:.2f} (n={n}) / {target:.2f}"
            f"  {'within' if within else 'outside'}"
        )
    out = ROOT / "reports" / "phases" / "P7_signal.json"
    out.write_text(json.dumps(report, indent=2))
    print(f"wrote {out.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
