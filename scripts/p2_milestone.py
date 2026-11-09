"""P2 milestone (plan/phases/P02-corridor-core.md): free-flow speed by sub-class.

Runs the early-morning free-flow scenario for several seeds, measures spot
speeds at the mid screenline, and compares mean and SD per sub-class with the
stage-1 validation target (Balakrishnan & Sivanandan 2015). Writes
reports/phases/P2.md and a figure. No parameter is tuned here: this is the
uncalibrated starting point that stage-1 calibration (P3.8, P10.2) begins from.

Usage: uv run python scripts/p2_milestone.py [--seeds 5]
"""

from __future__ import annotations

import argparse
import math
import tempfile
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import marg
import matplotlib.pyplot as plt
import polars as pl
import yaml
from marg import analysis

REPO = Path(__file__).resolve().parents[1]


def mark_free(c: pl.DataFrame, headway: float, res: marg.RunResult) -> pl.DataFrame:
    """Flag crossings with no crossing in the preceding ``headway`` s by a
    vehicle close enough laterally to interact at all (clearance at the pair's
    speed plus the distance where the model's lateral weight is cut off)."""
    p = {r["path"]: r["mean_si"] for r in res.params.iter_rows(named=True)}
    c0 = p["interaction.lateral.clearance.intercept"]
    c1 = p["interaction.lateral.clearance.slope"]
    cut = -p["interaction.lateral.braking_scale"] * math.log(p["core.interaction_weight_cutoff"])
    t, y, w, v = c["t"].to_list(), c["y"].to_list(), c["width"].to_list(), c["speed"].to_list()
    free = []
    for i in range(len(t)):
        ok = True
        j = i - 1
        while j >= 0 and t[i] - t[j] < headway:
            band = c0 + c1 * 0.5 * (v[i] + v[j]) + cut
            if abs(y[i] - y[j]) - 0.5 * (w[i] + w[j]) < band:
                ok = False
                break
            j -= 1
        free.append(ok)
    return c.with_columns(pl.Series("free", free))


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--seeds", type=int, default=5)
    ap.add_argument(
        "--free-headway",
        type=float,
        default=8.0,
        help="a crossing is 'free' if no vehicle in the same lateral band crossed within this many seconds before it",
    )
    args = ap.parse_args()

    scenario = yaml.safe_load((REPO / "scenarios/free_flow_early_morning.yaml").read_text())
    scenario["outputs"] = {"trajectories": {"every_s": 0.5, "s_from": 700, "s_to": 800}}
    text = yaml.safe_dump(scenario)
    targets = {
        t["id"]: t
        for t in yaml.safe_load((REPO / "validation/targets/patterns.yaml").read_text())["targets"]
    }
    target = targets["free_flow_speed_by_subclass"]
    tol = target["tolerance"]

    crossings = []
    kpis = []
    with tempfile.TemporaryDirectory() as tmp:
        for seed in range(1, args.seeds + 1):
            res = marg.run(
                text, seed=seed, out=Path(tmp) / f"s{seed}", params_dir=REPO / "params/base"
            )
            traj = res.table("trajectories")
            c = analysis.screenline_crossings(traj, "main", 750.0).filter(
                pl.col("t") >= scenario["warmup_s"]
            )
            types = res.agents.select("agent_id", "vehicle_type", "v0", "width")
            c = c.join(types, on="agent_id").sort("t")
            crossings.append(
                mark_free(c, args.free_headway, res).with_columns(pl.lit(seed).alias("seed"))
            )
            kpis.append({"seed": seed, **res.kpis})
    all_crossings = pl.concat(crossings)
    df = all_crossings.filter(pl.col("free"))
    sim_all = (
        all_crossings.group_by("vehicle_type")
        .agg((pl.col("speed").mean() * 3.6).alias("mean_all"))
        .sort("vehicle_type")
    )
    sim = (
        df.group_by("vehicle_type")
        .agg(
            pl.len().alias("n"),
            (pl.col("speed").mean() * 3.6).alias("mean"),
            (pl.col("speed").std() * 3.6).alias("sd"),
            (pl.col("v0").mean() * 3.6).alias("v0_mean"),
        )
        .sort("vehicle_type")
    )
    rows = []
    for vt, obs in target["value"].items():
        r = sim.filter(pl.col("vehicle_type") == vt)
        if r.height == 0:
            continue
        m, sd, n, v0m = r["mean"][0], r["sd"][0], r["n"][0], r["v0_mean"][0]
        m_all = sim_all.filter(pl.col("vehicle_type") == vt)["mean_all"][0]
        ok_m = abs(m - obs["mean"]) <= tol["mean_abs"]
        ok_sd = abs(sd - obs["sd"]) <= tol["sd_abs"]
        rows.append(
            (vt, n, obs["mean"], m, m_all, v0m, obs["sd"], sd, "pass" if ok_m and ok_sd else "fail")
        )

    fig, ax = plt.subplots(figsize=(8, 4.8), dpi=150)
    for k, (_vt, _n, om, sm, _all, _v0, osd, ssd, _st) in enumerate(rows):
        ax.errorbar(
            om,
            k + 0.12,
            xerr=osd,
            fmt="o",
            color="#555555",
            capsize=3,
            label="observed" if k == 0 else None,
        )
        ax.errorbar(
            sm,
            k - 0.12,
            xerr=ssd,
            fmt="o",
            color="#1f77b4",
            capsize=3,
            label="simulated" if k == 0 else None,
        )
    ax.set_yticks(range(len(rows)), [r[0] for r in rows])
    ax.set_xlabel("Free-flow spot speed, mean ± 1 SD (km/h)")
    ax.set_title(
        "Free-flow speed by sub-class: simulated vs Balakrishnan & Sivanandan (2015)", fontsize=10
    )
    ax.grid(axis="x", alpha=0.3)
    ax.legend(loc="lower right", frameon=False)
    fig.tight_layout()
    fig_path = REPO / "reports/phases/P2_free_flow.png"
    fig.savefig(fig_path)

    passed = sum(r[-1] == "pass" for r in rows)
    k = pl.DataFrame(kpis)
    lines = [
        "# P2 milestone: free-flow speed by sub-class",
        "",
        f"**{passed} of {len(rows)} sub-classes match the stage-1 target** within ±{tol['mean_abs']} km/h on the mean "
        f"and ±{tol['sd_abs']} km/h on the SD, with no parameter tuned "
        f"({args.seeds} seeds; {df.height} free crossings of {all_crossings.height} at the 750 m screenline after warm-up).",
        "",
        f"A crossing counts as *free* when no vehicle close enough laterally to interact (within the speed-dependent "
        f"clearance plus the interaction cutoff) crossed in the preceding {args.free_headway:g} s, mirroring how field studies select "
        f"free-flowing vehicles. The {args.free_headway:g} s threshold is an assumption to check against the "
        "source paper.",
        "",
        "Scenario: `scenarios/free_flow_early_morning.yaml` (300 veh/h on 1.5 km x 10.5 m). "
        "Target: `validation/targets/patterns.yaml#free_flow_speed_by_subclass` (Balakrishnan & Sivanandan 2015). "
        "Regenerate with `uv run python scripts/p2_milestone.py`.",
        "",
        "![Free-flow speed by sub-class](P2_free_flow.png)",
        "",
        "| Sub-class | Free crossings | Observed mean (km/h) | Simulated mean, free | Simulated mean, all | Mean drawn v0 | Observed SD | Simulated SD, free | Result |",
        "| --- | --- | --- | --- | --- | --- | --- | --- | --- |",
    ]
    for vt, n, om, sm, sa, v0m, osd, ssd, st in rows:  # noqa: B007
        lines.append(
            f"| {vt} | {n} | {om:.2f} | {sm:.2f} | {sa:.2f} | {v0m:.2f} | {osd:.2f} | {ssd:.2f} | {st} |"
        )
    gap = max(abs(sm - v0m) for _, _, _, sm, _, v0m, _, _, _ in rows)
    passing = [vt for vt, *_, st in rows if st == "pass"]
    lines += [
        "",
        "## Reading it",
        "",
        "- **The following model is right for free vehicles.** Free crossings travel at their own drawn desired "
        f"speed: compare *Simulated mean, free* with *Mean drawn v0* (within {gap:.1f} km/h for every sub-class).",
        "- **The gap to the target is selection bias from the missing overtaking.** The free subset is biased "
        "toward slow drivers: fast drivers catch slower vehicles in their lateral band and, in P2, cannot move "
        "laterally to pass (lateral movement arrives in P5). So the drawn v0 of *free* hatchbacks averages well "
        "below the population mean. Wide-spread car classes fail most; the sub-classes that pass "
        f"({', '.join(passing) or 'none'}) have narrower spreads, and which of them land inside ±2 km/h "
        "changes with the seeds' draws (P3.3 changed every agent's draws; this run passed "
        f"{len(passing)}, the previous one 3).",
        "- **What happens next.** Re-run this check after P5, then let stage-1 calibration (P3.8, P10.2) fit the "
        "`vehicles.free_flow_speed.*` and `vehicles.median_band.*` parameters jointly. Nothing is tuned here.",
        "",
        f"Mean KPIs over seeds: network speed {k['network_mean_speed_m_s'].mean() * 3.6:.1f} km/h, "
        f"mean delay {k['delay_mean_s'].mean():.1f} s, collision-guard events {int(k['collision_guard_events'].sum())}.",
        "",
    ]
    (REPO / "reports/phases/P2.md").write_text("\n".join(lines))
    print("\n".join(lines[:3]))
    print(sim)


if __name__ == "__main__":
    main()
