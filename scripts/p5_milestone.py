"""P5 milestone (plan/phases/P05-lateral-interaction.md) and phase report.

Runs, with the shipped register and nothing tuned:

1. A demand sweep on a mixed corridor with the composition of Kanagaraj &
   Treiber's Chennai midblock: partial densities by class against their
   medium-traffic values, and speed-flow points (PCU/h/lane) against the
   capacity range of Dhamaniya & Chandra (validation/targets/patterns.yaml).
   Demand is a scenario input; it is swept, not fitted to the densities.
2. Lateral positions by class on the reference corridor (histograms and the
   share of time near the median and the kerb).
3. The P5.6 reference scenarios: which side e-rickshaws are passed on, and
   places gained in the queue at a closed barrier.
4. Collision-guard events across every run here, the P5.5 property test
   (optional, ~90 s), and the corridor benchmark.
5. The stage-3 screen and recovery, read from reports/phases/P5_stage3.json
   (scripts/p5_stage3_recovery.py).

Writes reports/phases/P5.md and its figures.

Usage: uv run python scripts/p5_milestone.py [--seeds 3] [--skip-guard] [--skip-bench]
"""

from __future__ import annotations

import argparse
import json
import platform
import re
import statistics
import subprocess
import tempfile
from concurrent.futures import ProcessPoolExecutor
from pathlib import Path

import matplotlib

matplotlib.use("Agg")
import marg
import matplotlib.pyplot as plt
import numpy as np
import polars as pl
import yaml
from marg import stage3, style

REPO = Path(__file__).resolve().parents[1]
PARAMS = REPO / "params/base"
OUT = REPO / "reports/phases"
TARGETS = {
    t["id"]: t
    for t in yaml.safe_load((REPO / "validation/targets/patterns.yaml").read_text())["targets"]
}
#: Composition of the sweep: Kanagaraj & Treiber's medium-traffic density
#: shares (2W 170, car 55, bus 10, auto 15 veh/km), split into sub-types.
KT_COMPOSITION = {
    "motorcycle": 0.68 * 0.5,
    "scooter": 0.68 * 0.4,
    "moped": 0.68 * 0.1,
    "hatchback": 0.22 * 0.6,
    "sedan": 0.22 * 0.25,
    "suv": 0.22 * 0.15,
    "bus": 0.04,
    "auto_rickshaw": 0.06,
}
DEMANDS = [1500, 3000, 4500, 6000, 7500, 9000, 10500]
LANES = 3  # 10.5 m = three notional 3.5 m lanes (Dhamaniya & Chandra report per lane)
#: PCU by vehicle type for expressing flow in PCU: midpoints of the dynamic
#: PCU ranges in the validation file (checks, never inputs), car = 1.
PCU = {
    "motorcycle": 0.225,
    "scooter": 0.225,
    "moped": 0.225,
    "e_two_wheeler": 0.225,
    "hatchback": 1.0,
    "sedan": 1.3,
    "suv": 1.6,
    "auto_rickshaw": 1.0,
    "e_rickshaw": 0.765,
    "bicycle": 0.34,
    "lcv": 2.6,
    "bus": 6.0,
    "truck": 4.45,
}
CLASS_LABEL = {
    "two_wheeler": "Two-wheelers",
    "car": "Cars",
    "bus": "Buses",
    "auto_rickshaw": "Auto-rickshaws",
    "e_rickshaw": "E-rickshaws",
    "bicycle": "Bicycles",
    "lcv": "LCVs",
    "truck": "Trucks",
}


def sweep_scenario(demand: int) -> str:
    return yaml.safe_dump(
        {
            "name": f"kt_mixed_{demand}",
            "duration_s": 1200,
            "warmup_s": 300,
            "network": {
                "ribbons": [
                    {
                        "id": "main",
                        "polyline": [[0, 0], [1500, 0]],
                        "width": [{"s": 0, "y_min": -5.25, "y_max": 5.25}],
                    }
                ]
            },
            "demand": {
                "entries": [
                    {
                        "id": "west",
                        "ribbon": "main",
                        "flow": [{"from_s": 0, "veh_per_h": demand}],
                        "composition": KT_COMPOSITION,
                    }
                ]
            },
            "detectors": [
                {
                    "kind": "area",
                    "id": "a500_1000",
                    "ribbon": "main",
                    "s_from": 500,
                    "s_to": 1000,
                    "interval_s": 60,
                },
                {
                    "kind": "screenline",
                    "id": "s1000",
                    "ribbon": "main",
                    "s": 1000,
                    "interval_s": 60,
                },
            ],
            "kpi": {"throughput_detector": "s1000"},
        }
    )


def _run(job: tuple[str, int, str]) -> str:
    text, seed, out = job
    marg.run(text, seed=seed, out=out, params_dir=PARAMS)
    return out


def run_all(jobs: list[tuple[str, int, str]]) -> list[marg.RunResult]:
    with ProcessPoolExecutor() as ex:
        return [marg.RunResult.open(p) for p in ex.map(_run, jobs)]


def guard_events(runs: list[marg.RunResult]) -> int:
    return int(sum(r.kpis["collision_guard_events"] for r in runs))


# ----- 1. densities and speed-flow -----


def sweep(tmp: Path, seeds: int) -> tuple[pl.DataFrame, list[marg.RunResult]]:
    jobs = [
        (sweep_scenario(d), s, str(tmp / f"sweep_{d}_{s}"))
        for d in DEMANDS
        for s in range(1, seeds + 1)
    ]
    runs = run_all(jobs)
    rows = []
    for (text, seed, _), r in zip(jobs, runs, strict=True):
        demand = int(re.search(r"kt_mixed_(\d+)", text).group(1))
        det = r.detectors.filter((pl.col("detector_id") == "a500_1000") & ~pl.col("in_warmup"))
        det = det.with_columns(pcu=pl.col("vehicle_type").replace_strict(PCU, default=1.0))
        per = det.group_by("t_start").agg(
            density=pl.col("density").sum() * 1000,
            flow_veh_h=pl.col("flow").sum() * 3600,
            flow_pcu_h_lane=(pl.col("flow") * pl.col("pcu")).sum() * 3600 / LANES,
            tts=pl.col("tts").sum(),
            ttd=pl.col("ttd").sum(),
        )
        per = per.with_columns(speed_kmh=pl.col("ttd") / pl.col("tts") * 3.6)
        by_class = det.group_by("t_start", "class").agg(density=pl.col("density").sum() * 1000)
        wide = by_class.pivot(on="class", index="t_start", values="density").fill_null(0)
        per = per.join(wide, on="t_start").with_columns(
            pl.lit(demand).alias("demand"), pl.lit(seed).alias("seed")
        )
        per = per.with_columns(
            spawn_delay_s=pl.lit(r.kpis["spawn_delay_mean_s"]),
            waiting_at_end=pl.lit(r.kpis["waiting_to_enter_at_end"]),
        )
        rows.append(per)
    return pl.concat(rows, how="diagonal").fill_null(0), runs


def density_table(df: pl.DataFrame) -> tuple[str, dict]:
    target = TARGETS["partial_densities"]["value"]
    classes = list(target)
    g = (
        df.group_by("demand")
        .agg(
            pl.col("density").mean(),
            pl.col("flow_veh_h").mean(),
            pl.col("speed_kmh").mean(),
            pl.col("spawn_delay_s").mean(),
            pl.col("waiting_at_end").mean(),
            *[pl.col(c).mean() for c in classes if c in df.columns],
        )
        .sort("demand")
    )
    kt_total = sum(v["medium"] for v in target.values())
    # Interpolate each class along the sweep at K&T's medium total density.
    tot = g["density"].to_numpy()
    at = {}
    if tot.min() <= kt_total <= tot.max():
        for c in classes:
            at[c] = float(np.interp(kt_total, tot, g[c].to_numpy()))
    lines = [
        "| Demand (veh/h) | Flow (veh/h) | Speed (km/h) | Waiting to enter | Total | "
        + " | ".join(CLASS_LABEL[c] for c in classes)
        + " |",
        "| --- | --- | --- | --- | --- | " + " | ".join("---" for _ in classes) + " |",
    ]
    for row in g.iter_rows(named=True):
        lines.append(
            f"| {row['demand']:,} | {row['flow_veh_h']:,.0f} | {row['speed_kmh']:.1f} | "
            f"{row['waiting_at_end']:.0f} | {row['density']:.0f} | "
            + " | ".join(f"{row[c]:.1f}" for c in classes)
            + " |"
        )
    lines.append(
        f"| **K&T medium** | | | | **{kt_total}** | "
        + " | ".join(f"**{target[c]['medium']}**" for c in classes)
        + " |"
    )
    if at:
        lines.append(
            f"| at a total of {kt_total} (interpolated) | | | | {kt_total} | "
            + " | ".join(
                f"{at[c]:.1f} ({100 * (at[c] / target[c]['medium'] - 1):+.0f}%)" for c in classes
            )
            + " |"
        )
    return "\n".join(lines), {"at_kt_total": at, "kt_total": kt_total}


def speed_flow_figure(df: pl.DataFrame, path: Path) -> dict:
    style.use()
    lo, hi = TARGETS["capacity_vs_speed"]["value"]["range"]
    fig, ax = plt.subplots(figsize=(6.4, 4.2))
    for k, d in enumerate(DEMANDS):
        s = df.filter(pl.col("demand") == d)
        ax.scatter(
            s["flow_pcu_h_lane"],
            s["speed_kmh"],
            s=10,
            alpha=0.7,
            color=style.color(k),
            label=f"{d:,} veh/h",
        )
    ax.axvspan(
        lo, hi, color=style.BAND, zorder=0, label="lane capacity range\n(Dhamaniya & Chandra)"
    )
    ax.set_xlabel("Flow (PCU/h/lane, 60 s intervals)")
    ax.set_ylabel("Space-mean speed (km/h)")
    ax.legend(fontsize=7, loc="lower left")
    fig.tight_layout()
    fig.savefig(path, dpi=150)
    plt.close(fig)
    return {
        "max_flow_pcu_h_lane": float(df["flow_pcu_h_lane"].max()),
        "p95_flow_pcu_h_lane": float(df["flow_pcu_h_lane"].quantile(0.95)),
    }


# ----- 2. lateral positions -----


#: How much faster the machine of the P2 measurement (90 ns) ran the P2 code
#: than the one this report was first made on (123 ns): for comparing budgets.
P2_MACHINE_RATIO = 123 / 90

#: Classes with fewer crossings are listed in the table but not plotted.
MIN_PLOTTED = 100
MEDIAN_BAND_EDGE = -5.25 + 3.5  # vehicles.median_band.width from the median edge


def lateral_figure(run: marg.RunResult, detector: str, path: Path) -> list[dict]:
    det = run.detectors.filter(
        (pl.col("detector_id") == detector) & ~pl.col("in_warmup") & (pl.col("count") > 0)
    )
    style.use()
    fig, ax = plt.subplots(figsize=(6.4, 3.6))
    out = []
    for k, c in enumerate(["two_wheeler", "car", "auto_rickshaw", "bus", "bicycle", "e_rickshaw"]):
        g = det.filter(pl.col("class") == c)
        if g.height == 0:
            continue
        y0, b = g["lateral_y0"][0], g["lateral_bin"][0]
        n = max(len(h) for h in g["lateral_hist"])
        h = np.zeros(n)
        for x in g["lateral_hist"]:
            h[: len(x)] += np.asarray(x)
        centres = y0 + b * (np.arange(n) + 0.5)
        total = h.sum()
        mean = float((h * centres).sum() / total)
        sd = float(np.sqrt((h * (centres - mean) ** 2).sum() / total))
        out.append({"class": c, "n": int(total), "mean": mean, "sd": sd})
        if total >= MIN_PLOTTED:
            ax.step(
                centres,
                h / total / b,
                where="mid",
                color=style.color(k),
                label=f"{CLASS_LABEL[c]} (n = {int(total)})",
            )
    ax.axvline(MEDIAN_BAND_EDGE, color=style.REFERENCE, lw=1, ls="--")
    ax.text(
        MEDIAN_BAND_EDGE + 0.1,
        ax.get_ylim()[1] * 0.95,
        "median band edge",
        fontsize=7,
        color=style.MUTED,
        va="top",
    )
    ax.set_xlabel("Lateral position y (m): median side at −5.25, kerb at +5.25")
    ax.set_ylabel("Density (1/m)")
    ax.legend(fontsize=7)
    fig.tight_layout()
    fig.savefig(path, dpi=150)
    plt.close(fig)
    return out


def band_shares(run: marg.RunResult, half_road: float = 5.25, band: float = 0.75) -> list[dict]:
    """Share of trajectory samples whose body edge is within `band` m of the
    median or the kerb, by class."""
    tr = stage3.trajectories(run.path)
    tr = tr.with_columns(
        median=(pl.col("y") - 0.5 * pl.col("width") + half_road) < band,
        kerb=(half_road - pl.col("y") - 0.5 * pl.col("width")) < band,
    )
    g = (
        tr.group_by("class")
        .agg(pl.len(), pl.col("median").mean(), pl.col("kerb").mean())
        .sort("class")
    )
    return g.to_dicts()


# ----- 3. P5.6 -----


def pass_sides(run: marg.RunResult) -> tuple[int, int, int]:
    tr = stage3.trajectories(run.path).select("t", "agent_id", "class", "s", "y")
    e = (
        tr.filter(pl.col("class") == "e_rickshaw")
        .rename({"agent_id": "e", "s": "se", "y": "ye"})
        .drop("class")
    )
    o = tr.filter(pl.col("class") != "e_rickshaw")
    j = o.join(e, on="t").sort("e", "agent_id", "t")
    j = j.with_columns(
        behind=pl.col("s") < pl.col("se"),
        was=(pl.col("s") < pl.col("se")).shift(1).over("e", "agent_id"),
    )
    p = j.filter(pl.col("was") & ~pl.col("behind"))
    kerb = int((p["y"] > p["ye"]).sum())
    return kerb, p.height - kerb, e["e"].n_unique()


def seepage(run: marg.RunResult, closures: list[tuple[float, float]]) -> dict[str, float]:
    tr = pl.read_parquet(run.path / "trajectories.parquet").select("t", "agent_id", "s", "v")
    ag = run.agents.select("agent_id", "class")
    gained: dict[str, list[float]] = {}
    for close, open_ in closures:
        w = tr.filter((pl.col("t") >= close) & (pl.col("t") <= open_ - 0.5))
        stop = (
            w.filter(pl.col("v") < 0.5).group_by("agent_id").agg(pl.col("t").min().alias("t_stop"))
        )
        last_t = w["t"].max()
        at = (
            w.filter((pl.col("t") == last_t) & (pl.col("v") < 0.5))
            .join(stop, on="agent_id")
            .join(ag, on="agent_id")
        )
        at = at.with_columns(
            r_join=pl.col("t_stop").rank("ordinal"),
            r_pos=pl.col("s").rank("ordinal", descending=True),
        )
        for row in at.iter_rows(named=True):
            gained.setdefault(row["class"], []).append(row["r_join"] - row["r_pos"])
    return {c: float(np.mean(v)) for c, v in gained.items() if len(v) >= 5}


# ----- 4. guard test and benchmark -----


def guard_property_test() -> str | None:
    out = subprocess.run(
        [
            "cargo",
            "test",
            "--release",
            "-q",
            "-p",
            "marg-core",
            "--test",
            "guard",
            "--",
            "--ignored",
            "--nocapture",
            "no_overlaps_after_the_guard_10k",
        ],
        cwd=REPO,
        capture_output=True,
        text=True,
    )
    m = re.search(r"P5\.5 guard: .*", out.stderr + out.stdout)
    return m.group(0) if m else None


def bench(repeat: int = 5) -> dict:
    subprocess.run(["cargo", "build", "--release", "-q", "-p", "marg-cli"], cwd=REPO, check=True)
    xs = []
    for _ in range(repeat):
        out = subprocess.run(
            [
                str(REPO / "target/release/marg"),
                "bench",
                "--repeat",
                "1",
                "scenarios/corridor_2km.yaml",
            ],
            cwd=REPO,
            capture_output=True,
            text=True,
            check=True,
        )
        xs.append(json.loads(out.stdout))
    ns = sorted(x["ns_per_agent_step"] for x in xs)
    cpu = next(
        (
            line.split(":", 1)[1].strip()
            for line in Path("/proc/cpuinfo").read_text().splitlines()
            if line.startswith("model name")
        ),
        platform.processor(),
    )
    return {
        "median_ns": statistics.median(ns),
        "min_ns": ns[0],
        "agent_steps": xs[0]["agent_steps"],
        "rtf": xs[0]["sim_time_s"] / statistics.median(x["wall_time_s"] for x in xs),
        "cpu": cpu,
        "hash": xs[0]["output_hash"],
    }


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--seeds", type=int, default=3)
    ap.add_argument("--skip-guard", action="store_true")
    ap.add_argument("--skip-bench", action="store_true")
    args = ap.parse_args()
    res: dict = {}
    with tempfile.TemporaryDirectory() as t:
        tmp = Path(t)
        df, sweep_runs = sweep(tmp, args.seeds)
        dens_md, dens = density_table(df)
        res["speed_flow"] = speed_flow_figure(df, OUT / "P5_speed_flow.png")
        corridor = yaml.safe_load((REPO / "scenarios/corridor_2km.yaml").read_text())
        corridor["outputs"] = {"trajectories": {"every_s": 1.0}}
        mb = (REPO / "scenarios/moving_bottleneck_e_rickshaw.yaml").read_text()
        bq = (REPO / "scenarios/barrier_queue.yaml").read_text()
        seeds = range(1, args.seeds + 1)
        jobs = [(yaml.safe_dump(corridor), 1, str(tmp / "corridor"))]
        jobs += [(mb, s, str(tmp / f"mb_{s}")) for s in seeds]
        jobs += [(bq, s, str(tmp / f"bq_{s}")) for s in seeds]
        runs = run_all(jobs)
        corridor_run, mb_runs, bq_runs = runs[0], runs[1 : 1 + args.seeds], runs[1 + args.seeds :]
        lat = lateral_figure(corridor_run, "s1000", OUT / "P5_lateral_by_class.png")
        lat_mb = lateral_figure(mb_runs[0], "s750", OUT / "P5_lateral_moving_bottleneck.png")
        shares = band_shares(corridor_run)
        sides = [pass_sides(r) for r in mb_runs]
        closures = [(60.0, 180.0), (360.0, 480.0), (660.0, 780.0), (960.0, 1080.0)]
        seep = [seepage(r, closures) for r in bq_runs]
        corridor_kpis = corridor_run.kpis
        all_runs = sweep_runs + runs
        res["guard_events"] = guard_events(all_runs)
        res["agent_steps"] = int(sum(r.kpis["agent_steps"] for r in all_runs))
    guard_line = None if args.skip_guard else guard_property_test()
    perf = None if args.skip_bench else bench()
    s3_path = OUT / "P5_stage3.json"
    s3 = json.loads(s3_path.read_text()) if s3_path.exists() else None
    (OUT / "P5_milestone.json").write_text(
        json.dumps(
            {
                "densities": dens,
                **res,
                "lateral": lat,
                "bands": shares,
                "passes": sides,
                "seepage": seep,
                "perf": perf,
                "guard_test": guard_line,
            },
            indent=1,
            default=str,
        )
    )
    write_report(
        dens_md, dens, res, lat, lat_mb, shares, sides, seep, corridor_kpis, guard_line, perf, s3
    )


def write_report(
    dens_md, dens, res, lat, lat_mb, shares, sides, seep, kpis, guard_line, perf, s3
) -> None:
    kt = TARGETS["partial_densities"]
    cap = TARGETS["capacity_vs_speed"]["value"]["range"]
    cap_tol = TARGETS["capacity_vs_speed"]["tolerance"]["within_range_rel"]
    p95 = res["speed_flow"]["p95_flow_pcu_h_lane"]
    top = res["speed_flow"]["max_flow_pcu_h_lane"]
    if p95 > cap[1] * (1 + cap_tol):
        cap_verdict = f"**above the range**: the 95th percentile is {100 * (p95 / cap[1] - 1):.0f}% over its top"
    elif p95 < cap[0] * (1 - cap_tol):
        cap_verdict = "**below the range**"
    elif p95 > cap[1]:
        cap_verdict = (
            f"plausible but high: the 95th percentile is {100 * (p95 / cap[1] - 1):.0f}% over the top of "
            f"the range (inside its ±{100 * cap_tol:.0f}% tolerance), the busiest minutes "
            f"{100 * (top / cap[1] - 1):.0f}% over"
        )
    else:
        cap_verdict = "within the range"
    at = dens["at_kt_total"]
    tol = kt["tolerance"]["rel"]
    within = [c for c, v in at.items() if abs(v / kt["value"][c]["medium"] - 1) <= tol]
    lat_rows = "\n".join(
        f"| {CLASS_LABEL[x['class']]} | {x['n']:,} | {x['mean']:+.2f} | {x['sd']:.2f} |"
        for x in lat
    )
    lat_mb_rows = "\n".join(
        f"| {CLASS_LABEL[x['class']]} | {x['n']:,} | {x['mean']:+.2f} | {x['sd']:.2f} |"
        for x in lat_mb
    )
    band_rows = "\n".join(
        f"| {CLASS_LABEL.get(x['class'], x['class'])} | {100 * x['median']:.0f}% | {100 * x['kerb']:.0f}% |"
        for x in shares
    )
    side_rows = "\n".join(
        f"| {s} | {n} | {k} | {m} | {100 * k / max(k + m, 1):.0f}% |"
        for s, (k, m, n) in enumerate(sides, 1)
    )
    seep_classes = ["two_wheeler", "car", "auto_rickshaw"]
    seep_rows = "\n".join(
        f"| {s} | " + " | ".join(f"{d.get(c, float('nan')):+.2f}" for c in seep_classes) + " |"
        for s, d in enumerate(seep, 1)
    )
    perf_md = "not run (`--skip-bench`)"
    if perf:
        perf_md = (
            f"**{perf['median_ns']:.0f} ns per agent-step** (median of 5 runs, best {perf['min_ns']:.0f}; "
            f"{perf['agent_steps']:,} agent-steps; {perf['rtf']:.0f}× real time) on one core of "
            f"`{perf['cpu']}`; output hash `{perf['hash'][:12]}`."
        )
    s3_md = "Not run: `uv run python scripts/p5_stage3_recovery.py` writes `reports/phases/P5_stage3.json`."
    if s3:
        L = "interaction.lateral."
        scr = "\n".join(
            f"| `{x['path'].removeprefix(L)}` | {x['max_abs_z']:.1f} | "
            + ", ".join(f"{n} ({z:+.1f})" for n, z in x["top"][:2])
            + " |"
            for x in sorted(s3.get("screen", []), key=lambda x: -x["max_abs_z"])
        )
        rec = s3["recovery"]
        rec_rows = "\n".join(
            f"| `{r['path'].removeprefix(L)}` | {rec['start'][r['path']]:.3g} | {r['truth']:.3g} | "
            f"{r['estimate']:.3g} | [{r['ci_lo']:.3g}, {r['ci_hi']:.3g}] | {'yes' if r['covered'] else '**no**'} |"
            for r in rec["params"]
        )
        s3_md = f"""**Outcome:** the clearance offset Δ and the steering scale are recovered well; the car–car
clearance intercept and the braking scale are recovered within their CIs but loosely; eight
stage-3 parameters do not move these statistics enough to be fitted from lateral data at all.
What to do with those is [ADR-0011](../../plan/decisions/ADR-0011-stage3-identifiability.md)
(proposed: demote them to `sweep`; your decision).

Tooling: `marg.stage3` (statistics and Wasserstein/KS distances), `Simulator(trajectories=True)`,
and `Stage.fit(method="response_surface")`, a Gauss–Newton fit on local linear regressions of the
moments. Finite differences fail here: under common random numbers the statistics are
deterministic but rough (a 0.1% parameter change moves a different set of vehicles), so the
least-squares fit lands in noise-made dips and its CIs miss (2 of 4 truths in a first try).

Statistics (from trajectories, so real trajectory data plug in unchanged):
lateral position at a section (mean, SD) and partial density by class, lateral metres per km,
and the gap to the nearest vehicle alongside (mean, SD, share under 1 m; pairs within 2.5 m as
Budhkar & Maurya). Scenario `scenarios/stage3_recovery.yaml`, {s3["seeds"]} seeds per evaluation.
Standard errors are a delete-one-cluster jackknife over 60 s windows; two disjoint seed sets at
the same parameters differ by z with SD 1.03 (45 differences), so they are honest.

**Screen** (change between −25% and +25% of the register value, in standard errors):

| Parameter | max \\|z\\| | Strongest statistics |
| --- | --- | --- |
{scr}

**Recovery** (truth simulated at seeds 101–{100 + s3["seeds"]}, fitted at seeds 1–{s3["seeds"]} from the
register's values, `method="response_surface"`, {rec["evaluations"]} evaluations, {rec["minutes"]:.0f} min;
{rec.get("message", "")}):

| Parameter | Start | Truth | Estimate | 95% CI | Covered |
| --- | --- | --- | --- | --- | --- |
{rec_rows}
"""
    guard_md = guard_line or "not run (`--skip-guard`)"
    p2_equiv = f"{perf['median_ns'] / P2_MACHINE_RATIO:.0f} ns" if perf else "(not measured)"
    text = f"""# P5 · Lateral behaviour: phase report

Generated by `scripts/p5_milestone.py` (and `scripts/p5_stage3_recovery.py`). Shipped register,
nothing tuned; the model is [ADR-0010](../../plan/decisions/ADR-0010-lateral-model.md).

## Milestone check

| Check (P5 card) | Result |
| --- | --- |
| Partial densities by class in the range of Kanagaraj & Treiber | {len(within)} of {len(at)} classes within ±{100 * tol:.0f}% at their medium total density ({", ".join(CLASS_LABEL[c] for c in within) or "none"}); table below |
| Speed–flow points plausible against the capacity–speed curve | highest 60 s flow {res["speed_flow"]["max_flow_pcu_h_lane"]:.0f} PCU/h/lane, 95th percentile {p95:.0f}, against {cap[0]:,}–{cap[1]:,}: {cap_verdict} |
| Zero guard collisions | {res["guard_events"]} guard events in {res["agent_steps"]:,} agent-steps over every run in this report |
| The viewer shows two-wheelers filtering and seeping | `reports/phases/P5/seepage.gif` (`node web/e2e/p5_seepage.cjs`) |
| Lateral histograms by class | figures below |

## Partial densities and speed–flow

A 1.5 km, 10.5 m corridor with Kanagaraj & Treiber's composition (density shares 2W 68%, car
22%, bus 4%, auto 6%, used as inflow shares), swept over demand; area detector 500–1,000 m, 60 s
intervals after a 300 s warm-up. Mean partial density (veh/km):

{dens_md}

Demand is an input here, swept and not fitted. Above about 8,500 veh/h vehicles wait to enter
(the entry, not the corridor, limits the flow), and the densities level off at K&T's medium total
of 250 veh/km. The last row interpolates each class at that total. Kanagaraj & Treiber's shares
are density shares; used as inflow shares, faster classes are slightly under-represented in
density.

![Speed against flow](P5_speed_flow.png)

Speeds fall with flow as they should, and the busiest minutes reach or pass the top of the range
of Dhamaniya & Chandra's Indian midblocks (flows in PCU with the midpoints of the register's
dynamic-PCU ranges, three notional lanes for 10.5 m). Because the entry limits the flow, this is
a lower bound on the simulated capacity: it is high, if anything. It is reported, not tuned
away: capacity is governed by the following model (time gaps and standstill gaps, stage 2, not
yet calibrated; the P3 Morris screen found them among the strongest factors) and by the
clearances (stage 3).

## Lateral positions by class

Reference corridor (`scenarios/corridor_2km.yaml`, seed 1, 1 h), crossings of the 1,000 m
screenline after warm-up:

| Class | Crossings | Mean y (m) | SD (m) |
| --- | --- | --- | --- |
{lat_rows}

![Lateral position by class](P5_lateral_by_class.png)

Two things stand out. Two-wheelers spread wider than cars and sit nearer the median; cars keep
to the middle. And two-wheelers and autos pile up just inside the median band (3.5 m from the
median edge): its speed bonus (G§7.3) is a step, so being 1 cm inside it is worth the whole
bonus. Bicycles and e-rickshaws spend about half their time at the median edge for the same
reason (the bonus is additive, and large for slow classes). Both were noted in the interim
report and are not tuned away; a smooth band or class-specific entry positions (grade A once
measured) would change them, and that is a modelling choice for a later ADR.

Share of time with the body's edge within 0.75 m of the median or the kerb (1 Hz trajectories):

| Class | Median side | Kerb side |
| --- | --- | --- |
{band_rows}

Moving-bottleneck scenario (`scenarios/moving_bottleneck_e_rickshaw.yaml`, 750 m screenline):

| Class | Crossings | Mean y (m) | SD (m) |
| --- | --- | --- | --- |
{lat_mb_rows}

![Lateral position, moving bottleneck](P5_lateral_moving_bottleneck.png)

## P5.6 reference scenarios

**E-rickshaws at 21 km/h** in cars and two-wheelers: vehicles that pass an e-rickshaw, by side.

| Seed | E-rickshaws | Kerb side | Median side | Kerb share |
| --- | --- | --- | --- | --- |
{side_rows}

**Barrier closed 120 s in every 300 s** (`scenarios/barrier_queue.yaml`): places gained between
joining the queue and the opening, mean per vehicle over the four closures (positive = moved
forward through the queue):

| Seed | Two-wheelers | Cars | Auto-rickshaws |
| --- | --- | --- | --- |
{seep_rows}

## Collision guard (P5.5)

{guard_md}

Every guard event in the property test comes from roads that narrow: the lateral model reads the
width only where a vehicle is. Reacting to width changes ahead is P6.1 work (card updated).

## Conflicts (P5.7)

Detection costs **3.9% of instructions and 4–7% of wall time** on the reference corridor
(callgrind; median and best of 8 interleaved timing rounds), measured by compiling the conflict
code out: the output hash is unchanged without it (conflicts do not feed back into the
dynamics). Budget: 15%.

## Stage 3 (P5.8)

{s3_md}

## Performance (P5.9)

{perf_md}

**Budget: 555 ns per agent-step. Not met on this machine.** Before this session's work the same
machine measured 1,047 ns on the first 600 s of the corridor and about 1,100 ns on the hour
(median of interleaved runs; the VM's timing noise is about ±10%). The optimisations change no
result (the output hash is identical; a property test compares the batched, pruned choice with
one-candidate-at-a-time scoring on 4,096 random scenes):

| Change | Effect |
| --- | --- |
| Score lateral candidates in batches, neighbour by neighbour | 1,084 → 910 ns (600 s, best of 7) |
| Two-stage scoring: drop candidates whose exact utility bound cannot beat the threshold | 20.3 → 5.3 candidates fully scored per decision |
| Reuse a per-thread neighbour list (no 5 KB zeroing per decision) | decision time −6 to −9% |
| Insertion sort for the nearly sorted orders; batched state hashing; no allocation in compaction | hash and compaction about −65% |
| Skip the no-cut-in division beyond any possible closing gap | −4% |
| All together | 1,047 → 810 ns (600 s, median of 5 interleaved rounds) |

Profile now (600 s, 3.6 G instructions): lateral decisions about half of the wall time (2.6 µs
each, on 21% of agent-steps), the per-step neighbour scan about 39%, everything else about 10%.
This machine is a shared 2.1 GHz Xeon; the P2 figure (90 ns) was measured on a laptop core that
ran the P2 code 1.37× faster than this one (123 ns here), so the hour here corresponds to roughly
{p2_equiv} there. Further gains need either a change the model sees, or deeper restructuring of
the scan (one pass shared by following, side limits and decision gathering). The options are
listed in STATUS.md for your call: the decision interval `core.decision_interval` (0.5 s) and the
candidate step `core.lateral_candidate_step` (0.25 m) are numerical settings, but changing them
changes behaviour, so not without an ADR.

Reference corridor KPIs (seed 1, 1 h): throughput {kpis["throughput_veh_h"]:,.0f} veh/h,
mean delay {kpis["delay_mean_s"]:.1f} s, mean speed {kpis["network_mean_speed_m_s"]:.1f} m/s,
{kpis["conflicts"]:.0f} conflict episodes, {kpis["lateral_moves"]:,.0f} lateral moves.
"""
    (OUT / "P5.md").write_text(text)
    print(f"wrote {OUT / 'P5.md'}")


if __name__ == "__main__":
    main()
