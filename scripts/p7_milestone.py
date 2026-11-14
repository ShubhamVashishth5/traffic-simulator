"""P7 milestone (plan/phases/P07-junctions-splits.md): the split experiment, and the phase report.

Runs, with the shipped register and nothing tuned towards any target:

1. **The split experiment.** `scenarios/split_corridor.yaml` (a 10.5 m road splitting into a
   kerb-side and a median-side branch) with the late-realiser share swept against the
   two-wheeler share of the stream, five seeds each, at 1,000 veh/h and, for the onset of
   gridlock, at 1,200 veh/h. Throughput leaving the split,
   stop-and-wait and give-up decisions, voids (10 s in which nothing leaves the split), and the
   gridlock indicator, with 95% confidence intervals over seeds.
2. P7.5: U-turn paths at a median opening on a four-lane and a six-lane divided road.
3. P7.7: headways downstream of a signal that feeds an entry, against a renewal process.
4. P7.8: the roundabout at light, moderate and the scenario's flows.

It reads reports/phases/P7_signal.json (scripts/p7_signal_check.py) and
reports/phases/P7_gaps.json (scripts/p7_gap_recovery.py) for P7.3 and P7.4.

Every split input is a placeholder (ADR-0017): the guide has no Indian study of late crossing
at splits. So are the U-turn path distributions and the roundabout inputs.

Writes reports/phases/P7.md, reports/phases/P7_split.csv, reports/phases/P7_split.png and
reports/phases/P7_milestone.json.

Usage: uv run python scripts/p7_milestone.py [--seeds 5] [--jobs 4]
"""

from __future__ import annotations

import argparse
import json
import math
import subprocess
import tempfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import numpy as np
import polars as pl
import yaml
from scipy import stats as sps

ROOT = Path(__file__).resolve().parent.parent
MARG = ROOT / "target" / "release" / "marg"
OUT = ROOT / "reports" / "phases"
SCEN = ROOT / "scenarios"

LATE = [0.0, 0.1, 0.2, 0.3, 0.4]
TWO_WHEELERS = [0.15, 0.27, 0.45]
TW_TYPES = ("motorcycle", "scooter", "moped", "e_two_wheeler")
VOID_S = 10

#: What the tables show, written from them (rerun with --from-json after editing).
FINDINGS = """
## Findings

1. **The split experiment does not show what P7.6 asks for, and the cause is in the model,
   not in the sweep.** At 1,000 veh/h with *no* late realisers, about 13% of drivers reach
   their deadline on the wrong side and stop to wait, as many again give up, and the split
   gridlocks in most runs (3 of 5 at the base two-wheeler share). Raising the late-realiser
   share from 0 to 0.4 moves none of throughput, stop-and-wait or voids significantly at any
   two-wheeler share (|ρ| ≤ 0.38, p ≥ 0.06; the counts are also confounded by gridlock, since
   a locked run sends fewer drivers to the split), and at 1,200 veh/h almost every run
   gridlocks whatever the share. Three forms of ADR-0017, now run for the first time, cause
   this; the options are in ADR-0017 and need your decision:
   - *Drivers who know their branch move over late.* The pull towards the branch acts only
     within the 150 m lookahead, and it is proportional to urgency, which stays near zero
     until the move is nearly impossible (û = Δy / (tan θ_max × distance left)). In single
     runs at 1,000 veh/h with no late realisers, 44% of drivers were on the wrong side at
     their deadline with the placeholder `w_route` = 1, and still 28% at the register's
     maximum of 20.
   - *A stop-and-wait never ends.* Two waiting drivers who need each other's side block each
     other for good, and the road behind them locks: the gridlocks start there.
   - *A late realiser who has not learnt its branch by its deadline does not stop* (it does
     not know): more late realisers turn would-be stoppages into missed branches. Missed
     branches rise with the share only with 45% two-wheelers (ρ = +0.48, p = 0.015).
2. **More two-wheelers make the split work better.** Averaged over the late-realiser shares,
   throughput is 373, 598 and 764 veh/h at 15%, 27% and 45% two-wheelers, voids 63%, 42% and
   27% of 10 s intervals, gridlock in 72%, 68% and 40% of runs: two-wheelers filter past the
   waiting cars. All inputs are placeholders: this says what the mechanisms do, not what
   Delhi does.
3. **Gap acceptance (P7.4).** The maximum-likelihood estimate recovers two-wheelers and cars
   within about one standard error of the critical gaps they drew, auto-rickshaws 8% low
   (z −2.9); with busier major streams the low bias grows (cars −5% at 600 veh/h each way).
   The core re-reads a gap at every step as a driver approaches (the forecast lag rises as it
   nears its line), so a driver that takes a gap on the move takes it the moment the forecast
   reaches its critical gap. The logged gap then sits at the critical gap, not at the fixed
   gap a survey records, and the estimator, which assumes fixed gaps, comes out low.
   Restricting to drivers who stood at the line did not remove it. **For your decision:** the
   register's critical gaps are Abhigna et al.'s clearing-behaviour values (4.4–10.75 s); the
   same paper's Raff values, from the same data, are 2.3–4.8 s, and the paper calls the
   clearing-behaviour value "the safest critical gap". The register uses it as each driver's
   decision threshold, which is why compliant turns are the bottleneck at the signal crossing
   (ADR-0016 open problem 4).
4. **U-turns (P7.5)** follow the road-width rule: on the four-lane road the paths have mean
   1.92 m and SD 1.19 m (the gamma's 2.0 and 1.26); on the six-lane road they are bimodal, with
   1% of paths between the modes. One collision-guard event in each 30 min run.
5. **Entries fed through a signal (P7.7)** arrive in platoons: the coefficient of variation
   of headways is 2.98 (simulated stop line) and 2.15 (entry held) against 0.95 for the
   renewal entry, with one gap over 25 s per cycle (16 and 15 in 1,500 s of 90 s cycles)
   against none.
6. **Roundabouts (P7.8)** carry light traffic (150 veh/h per arm: every seed flows, 0–15
   guard events in 30 min) and lock at 300–500 veh/h per arm. The geometry was wrong in ways
   that are now fixed and tested (ADR-0009 addendum); what remains is that the junction
   arbitration of decision D, built for a box that vehicles cross, does not carry over to a
   ring where every vehicle is always inside the junction. Options in ADR-0009, for your
   decision.
"""


def load(name: str) -> dict:
    return yaml.safe_load((SCEN / name).read_text())


def with_two_wheelers(comp: dict[str, float], share: float) -> dict[str, float]:
    tw = sum(v for k, v in comp.items() if k in TW_TYPES)
    rest = sum(v for k, v in comp.items() if k not in TW_TYPES)
    return {
        k: round(v * (share / tw if k in TW_TYPES else (1 - share) / rest), 6)
        for k, v in comp.items()
    }


def split_spec(flow: float, tw: float) -> dict:
    spec = load("split_corridor.yaml")
    entry = spec["demand"]["entries"][0]
    entry["flow"] = [{"from_s": 0, "veh_per_h": flow}]
    entry["composition"] = with_two_wheelers(entry["composition"], tw)
    for d in spec["detectors"]:
        if d["id"].startswith("out_"):
            d["interval_s"] = VOID_S
    return spec


def run_marg(spec: dict, seed: int, sets: dict[str, str], tmp: Path, tag: str) -> Path:
    path = tmp / f"{tag}.yaml"
    path.write_text(yaml.safe_dump(spec, sort_keys=False))
    folder = tmp / tag
    cmd = [str(MARG), "run", str(path), "--seed", str(seed), "--out", str(folder)]
    for k, v in sets.items():
        cmd += ["--set", f"{k}={v}"]
    subprocess.run(cmd, check=True, capture_output=True)
    return folder


def kpis(folder: Path) -> dict[str, float]:
    k = pl.read_parquet(folder / "kpis.parquet").to_dicts()[0]
    return {key: (math.nan if v is None else v) for key, v in k.items()}


def split_metrics(folder: Path, spec: dict) -> dict:
    k = kpis(folder)
    warm, dur = spec["warmup_s"], spec["duration_s"]
    det = pl.read_parquet(folder / "detectors.parquet").filter(
        pl.col("detector_id").is_in(["out_service", "out_flyover"]) & (pl.col("t_start") >= warm)
    )
    per = det.group_by("t_start").agg(pl.col("count").sum())
    counts = dict(per.iter_rows())
    starts = np.arange(warm, dur, VOID_S)
    series = np.array([counts.get(float(t), 0) for t in starts])
    spawned = max(k["spawned"], 1.0)
    return {
        "throughput_veh_h": float(series.sum()) * 3600.0 / (dur - warm),
        "void_share": float(np.mean(series == 0)),
        "stop_and_wait_per_1000": 1000.0 * k["split_stop_and_wait"] / spawned,
        "give_up_per_1000": 1000.0 * k["split_give_up"] / spawned,
        "missed_per_1000": 1000.0 * k["missed_branches"] / spawned,
        "stop_and_wait": k["split_stop_and_wait"],
        "gridlock_share": k["gridlock_share"],
        "gridlock_onset_s": k["gridlock_onset_s"],
        "guard": k["collision_guard_events"],
    }


def ci(xs: list[float]) -> tuple[float, float]:
    xs = [x for x in xs if not math.isnan(x)]
    if not xs:
        return math.nan, math.nan
    m = float(np.mean(xs))
    if len(xs) < 2:
        return m, math.nan
    half = float(sps.t.ppf(0.975, len(xs) - 1) * np.std(xs, ddof=1) / math.sqrt(len(xs)))
    return m, half


def fmt(m: float, h: float, digits: int = 0) -> str:
    if math.isnan(m):
        return "–"
    if math.isnan(h):
        return f"{m:.{digits}f}"
    return f"{m:.{digits}f} ± {h:.{digits}f}"


def crossing_times(folder: Path, at: float) -> np.ndarray:
    tr = pl.read_parquet(folder / "trajectories.parquet").filter(pl.col("ribbon") == "main")
    first = tr.filter(pl.col("s") >= at).group_by("agent_id").agg(pl.col("t").min())
    return np.sort(first["t"].to_numpy())


def headway_stats(t: np.ndarray, after: float, long_s: float) -> dict:
    t = t[t > after]
    h = np.diff(t)
    h = h[h > 0]
    return {
        "headways": int(h.size),
        "mean_s": round(float(h.mean()), 2),
        "cv": round(float(h.std(ddof=1) / h.mean()), 2),
        "share_below_1_5_s": round(float(np.mean(h < 1.5)), 3),
        "long_gaps": int(np.sum(h > long_s)),
    }


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--seeds", type=int, default=5)
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument(
        "--from-json",
        action="store_true",
        help="rewrite the report from reports/phases/P7_milestone.json without running",
    )
    args = ap.parse_args()
    if args.from_json:
        report(json.loads((OUT / "P7_milestone.json").read_text())["runs"], args.seeds)
        return
    seeds = list(range(1, args.seeds + 1))
    tmp = Path(tempfile.mkdtemp(prefix="p7_"))
    jobs: list[tuple] = []

    # 1. The split experiment.
    for flow in (1000, 1200):
        tws = TWO_WHEELERS if flow == 1000 else [0.27]
        for tw in tws:
            spec = split_spec(flow, tw)
            for late in LATE:
                for seed in seeds:
                    tag = f"split_{flow}_{tw}_{late}_{seed}"
                    sets = {"road.split.late_realiser_share": str(late)}
                    jobs.append(("split", tag, spec, seed, sets, {"flow": flow, "tw": tw, "late": late}))
    # 2. U-turn paths.
    for half, lanes in ((3.5, "four-lane"), (5.25, "six-lane")):
        spec = load("median_opening.yaml")
        off = half + 0.5
        for r in spec["network"]["ribbons"]:
            r["width"] = [{"s": 0, "y_min": -half, "y_max": half}]
            r["polyline"] = [[x, math.copysign(off, y)] for x, y in r["polyline"]]
        for e in spec["demand"]["entries"]:
            e["flow"] = [{"from_s": 0, "veh_per_h": 500}]
        j = spec["network"]["junctions"][0]
        j["turns"] = {"e": {"w": 0.5, "e": 0.5}, "w": {"e": 0.5, "w": 0.5}}
        spec["duration_s"], spec["warmup_s"] = 1800, 0
        jobs.append(("uturn", f"uturn_{lanes}", spec, 1, {}, {"lanes": lanes, "width": 2 * half}))
    # 3. Upstream signal.
    base = load("upstream_signal.yaml")
    base["outputs"] = {"trajectories": {"every_s": 0.1, "s_from": 299.0, "s_to": 303.0}}
    variants = {
        "renewal": None,
        "simulated stop line at 200 m": {"stop_line_s": 200, "cycle_s": 90, "green_s": 40, "amber_s": 3},
        "entry held unless green": {"cycle_s": 90, "green_s": 40, "amber_s": 3},
    }
    for name, sig in variants.items():
        spec = json.loads(json.dumps(base))
        entry = spec["demand"]["entries"][0]
        if sig is None:
            entry.pop("upstream_signal")
        else:
            entry["upstream_signal"] = sig
        jobs.append(("signal", f"signal_{len(jobs)}", spec, 1, {}, {"variant": name}))
    # 4. Roundabout.
    for flow in (150, 300, 500):
        spec = load("roundabout.yaml")
        for e in spec["demand"]["entries"]:
            e["flow"] = [{"from_s": 0, "veh_per_h": flow}]
        for seed in seeds[:3]:
            jobs.append(("ring", f"ring_{flow}_{seed}", spec, seed, {}, {"flow": flow}))

    def work(job: tuple) -> tuple:
        kind, tag, spec, seed, sets, meta = job
        folder = run_marg(spec, seed, sets, tmp, tag)
        if kind == "split":
            res = split_metrics(folder, spec)
        elif kind == "uturn":
            ev = pl.read_parquet(folder / "events.parquet").filter(pl.col("kind") == "u_turn")
            paths = np.array([json.loads(a)["path_from_median_m"] for a in ev["attrs"]])
            k = kpis(folder)
            res = {
                "u_turns": int(paths.size),
                "mean_m": round(float(paths.mean()), 2),
                "sd_m": round(float(paths.std(ddof=1)), 2),
                "share_0_2_5": round(float(np.mean(paths < 2.5)), 3),
                "share_3_4": round(float(np.mean((paths >= 3) & (paths < 4))), 3),
                "share_4_8": round(float(np.mean((paths >= 4) & (paths < 8))), 3),
                "guard": k["collision_guard_events"],
                "exited": k["exited"],
                "spawned": k["spawned"],
            }
        elif kind == "signal":
            res = headway_stats(crossing_times(folder, 300.0), 300.0, 25.0)
        else:
            k = kpis(folder)
            res = {
                "spawned": k["spawned"],
                "exited": k["exited"],
                "in_network_at_end": k["in_network_at_end"],
                "guard": k["collision_guard_events"],
                "gaps_accepted": k["gaps_accepted"],
            }
        return kind, {**meta, "seed": seed, **res}

    with ThreadPoolExecutor(args.jobs) as pool:
        results = list(pool.map(work, jobs))
    by_kind: dict[str, list[dict]] = {}
    for kind, r in results:
        by_kind.setdefault(kind, []).append(r)
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "P7_milestone.json").write_text(json.dumps({"runs": by_kind}, indent=1, default=float) + "\n")
    report(by_kind, args.seeds)


def report(by_kind: dict[str, list[dict]], n_seeds: int) -> None:
    seeds = list(range(1, n_seeds + 1))
    split = pl.DataFrame(by_kind["split"])
    OUT.mkdir(parents=True, exist_ok=True)
    split.write_csv(OUT / "P7_split.csv")
    guard_total = int(split["guard"].sum())

    md = [
        "# P7 milestone: junctions, splits and gap acceptance",
        "",
        "Generated by `scripts/p7_milestone.py` (P7.3 from `scripts/p7_signal_check.py`, P7.4 from",
        "`scripts/p7_gap_recovery.py`). Nothing here was tuned towards a target.",
        "",
        "**Read this first.** The guide has no Indian study of late crossing at splits, no U-turn",
        "path parameters that could be read here, and no roundabout numbers: the inputs of",
        "sections 1, 3 and 5 are placeholders (in the scenarios' `overrides`). Two things in this",
        "report need your decision before the split experiment means much: the forms ADR-0017",
        "proposed for when drivers move over and how a stop-and-wait ends (section 1), and how",
        "vehicles on a ring are arbitrated (section 5, ADR-0009).",
        "",
        "## 1. The split experiment (milestone)",
        "",
        "`scenarios/split_corridor.yaml`: 700 m of 10.5 m carriageway, then a kerb-side branch",
        "(55%) and a median-side branch (45%), each 5.25 m. Vehicles enter at their class's usual",
        "lateral position whatever branch they need. Late realisers learn their branch 20-120 m",
        "before the split, or from the sign at 80 m unless a bus, truck or tractor is in front;",
        "everyone else knows it from the entry. At its deadline a driver on the wrong side stops",
        "to wait with probability 0.5, otherwise it gives up and takes the branch it is in.",
        f"{len(seeds)} seeds per point, 30 min (5 min warm-up); ± is a 95% CI over seeds.",
        f"Collision-guard events in all {split.height} runs: **{guard_total}**.",
        "",
        "### At 1,000 veh/h (in a single seed, the stream flowed without late realisers)",
        "",
        "| 2W share | Late realisers | Throughput (veh/h) | Stop-and-wait /1000 | Give-up /1000 | Voids (10 s, %) | Gridlock (runs) |",
        "| --- | --- | --- | --- | --- | --- | --- |",
    ]
    summary = []
    for tw in TWO_WHEELERS:
        for late in LATE:
            rs = split.filter(
                (pl.col("flow") == 1000) & (pl.col("tw") == tw) & (pl.col("late") == late)
            )
            row = {"flow": 1000, "tw": tw, "late": late}
            for key in ("throughput_veh_h", "stop_and_wait_per_1000", "give_up_per_1000", "void_share"):
                row[key] = ci(rs[key].to_list())
            locked = int(np.sum(np.isfinite(rs["gridlock_onset_s"].fill_null(math.nan).to_numpy())))
            row["gridlocked"] = locked
            summary.append(row)
            v = row["void_share"]
            md.append(
                f"| {tw:.2f} | {late:.1f} | {fmt(*row['throughput_veh_h'])} | "
                f"{fmt(*row['stop_and_wait_per_1000'], 1)} | {fmt(*row['give_up_per_1000'], 1)} | "
                f"{fmt(100 * v[0], 100 * v[1], 1)} | {locked}/{rs.height} |"
            )
    # Monotone over the sweep? Spearman over runs, per two-wheeler share.
    md += ["", "Rank correlation with the late-realiser share over runs (Spearman ρ, p):", ""]
    md += ["| 2W share | Throughput | Stop-and-wait | Voids |", "| --- | --- | --- | --- |"]
    mono = {}
    for tw in TWO_WHEELERS:
        rs = split.filter((pl.col("flow") == 1000) & (pl.col("tw") == tw))
        cells = []
        for key in ("throughput_veh_h", "stop_and_wait_per_1000", "void_share"):
            rho, pv = sps.spearmanr(rs["late"].to_numpy(), rs[key].to_numpy())
            mono[f"{tw}:{key}"] = (round(float(rho), 3), round(float(pv), 4))
            cells.append(f"{rho:+.2f} ({pv:.3f})")
        md.append(f"| {tw:.2f} | " + " | ".join(cells) + " |")
    md += [
        "",
        "### At 1,200 veh/h: the onset of gridlock",
        "",
        "Gridlock (plan 04 §3, now built): an area detector's space-mean speed below",
        "`observers.gridlock.speed` (5 km/h) for `observers.gridlock.duration` (60 s). Onset is",
        "the start of the first such spell.",
        "",
        "| Late realisers | Runs gridlocked | Onset (s), median [min, max] | Gridlock share of time | Throughput (veh/h) |",
        "| --- | --- | --- | --- | --- |",
    ]
    for late in LATE:
        rs = split.filter((pl.col("flow") == 1200) & (pl.col("late") == late))
        on = rs["gridlock_onset_s"].fill_null(math.nan).to_numpy()
        on = on[np.isfinite(on)]
        onset = f"{np.median(on):.0f} [{on.min():.0f}, {on.max():.0f}]" if on.size else "–"
        md.append(
            f"| {late:.1f} | {on.size}/{rs.height} | {onset} | "
            f"{fmt(*ci(rs['gridlock_share'].to_list()), 2)} | {fmt(*ci(rs['throughput_veh_h'].to_list()))} |"
        )
    md += [
        "",
        "![Throughput, stop-and-wait and voids against the late-realiser share](P7_split.png)",
        "",
    ]
    # 2. Gap acceptance and the signal.
    gaps_json = json.loads((OUT / "P7_gaps.json").read_text())
    md += [
        "## 2. Gap acceptance (P7.4): critical gaps back out of the simulation",
        "",
        "`scripts/p7_gap_recovery.py`: the priority crossing with no heavy vehicles, major road",
        f"{gaps_json['flows_veh_h']['e']} veh/h each way, minor {gaps_json['flows_veh_h']['n']} veh/h,",
        f"{gaps_json['seeds']} seeds × {gaps_json['duration_s']} s. Every gap a waiting driver turns down is now",
        "logged (`gap_rejected`) beside the one it takes; `marg.gaps` estimates the critical-gap",
        "population from them. *Drawn* is the critical gaps the deciding drivers actually drew.",
        "",
        "| Class | Drivers | Register (median / σ) | Drawn | MLE median (z vs drawn) | MLE σ | Raff |",
        "| --- | --- | --- | --- | --- | --- | --- |",
    ]
    for c, r in gaps_json["classes"].items():
        md.append(
            f"| {c} | {r['drivers']} | {r['truth_median_s']} / {r['truth_sigma']} | "
            f"{r['drawn_median_s']} / {r['drawn_sigma']} | {r['mle_median_s']} ± {r['mle_median_se']} "
            f"({r['z_median_vs_drawn']:+.1f}) | {r['mle_sigma']} ± {r['mle_sigma_se']} | {r['raff_s']} |"
        )
    sig = json.loads((OUT / "P7_signal.json").read_text())
    md += [
        "",
        "## 3. U-turns at a median opening (P7.5)",
        "",
        "`scenarios/median_opening.yaml`, half of each direction U-turning, 500 veh/h each way.",
        "Where a U-turner joins the receiving carriageway is drawn from the four-lane path below",
        "six notional lanes of receiving width (10.5 m) and from the six-lane one at or above it.",
        "Both are placeholders (Mohapatra, Dey & Chandra 2016 could not be read): a gamma",
        "(2.5, 0.8 m) and a mixture 0.6 Gamma(4, 0.3 m) + 0.4 N(5.5, 0.8 m).",
        "",
        "| Road | Receiving width | U-turns | Mean (m) | SD (m) | < 2.5 m | 3-4 m | 4-8 m | Guard events |",
        "| --- | --- | --- | --- | --- | --- | --- | --- | --- |",
    ]
    for r in by_kind["uturn"]:
        md.append(
            f"| {r['lanes']} | {r['width']} m | {r['u_turns']} | {r['mean_m']} | {r['sd_m']} | "
            f"{r['share_0_2_5']} | {r['share_3_4']} | {r['share_4_8']} | {r['guard']:.0f} |"
        )
    md += [
        "",
        "## 4. Entries fed through a signal (P7.7)",
        "",
        "`scenarios/upstream_signal.yaml`: 1,000 veh/h, a 90 s cycle with 40 s green and 3 s amber.",
        "Headways where vehicles cross station 300 m (100 m past the simulated stop line), from 300 s.",
        "",
        "| Feed | Headways | Mean (s) | CV | Share below 1.5 s | Gaps over 25 s |",
        "| --- | --- | --- | --- | --- | --- |",
    ]
    for r in by_kind["signal"]:
        md.append(
            f"| {r['variant']} | {r['headways']} | {r['mean_s']} | {r['cv']} | "
            f"{r['share_below_1_5_s']} | {r['long_gaps']} |"
        )
    md += [
        "",
        "## 5. Roundabouts (P7.8)",
        "",
        "`scenarios/roundabout.yaml`: ring radius 16 m, ring width 8 m, stop lines 4 m outside it,",
        "every turn (60% through). Three seeds per flow, 30 min.",
        "",
        "| Flow per arm (veh/h) | Seed | Spawned | Exited | Still in the network | Guard events |",
        "| --- | --- | --- | --- | --- | --- |",
    ]
    for r in sorted(by_kind["ring"], key=lambda r: (r["flow"], r["seed"])):
        md.append(
            f"| {r['flow']} | {r['seed']} | {r['spawned']:.0f} | {r['exited']:.0f} | "
            f"{r['in_network_at_end']:.0f} | {r['guard']:.0f} |"
        )
    md += [
        "",
        "## 6. Signals (P7.3, from P7_signal.json)",
        "",
        "`scenarios/signal_crossing.yaml`, every turn at 700 veh/h per arm (unchanged by this phase:",
        "the box scenarios give the same output hashes as before it).",
        "",
        "| Approach | Cycles | Saturation flow (veh/h) | Per metre of width |",
        "| --- | --- | --- | --- |",
    ]
    for r in sig["saturation_flow"]:
        md.append(f"| {r['arm']} | {r['cycles']} | {r['veh_per_h']:.0f} | {r['veh_per_h_per_m']:.0f} |")
    md += ["", "| Class | Discharge acceleration (m/s²) | Check (G§1) | Within ±20% |", "| --- | --- | --- | --- |"]
    for c, r in sig["discharge_acceleration"].items():
        md.append(f"| {c} | {r['mean']:.2f} (n {r['n']}) | {r['check']} | {'yes' if r['within_20pct'] else 'no'} |")
    md += ["", FINDINGS.strip(), ""]
    (OUT / "P7.md").write_text("\n".join(md) + "\n")

    # Figure.
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    from marg import style

    style.use()
    fig, axes = plt.subplots(1, 3, figsize=(10.5, 3.4))
    for k, tw in enumerate(TWO_WHEELERS):
        rows = [r for r in summary if r["tw"] == tw]
        x = [r["late"] for r in rows]
        for ax, key, scale in zip(
            axes,
            ("throughput_veh_h", "stop_and_wait_per_1000", "void_share"),
            (1, 1, 100),
            strict=True,
        ):
            m = [scale * r[key][0] for r in rows]
            h = [scale * r[key][1] for r in rows]
            ax.errorbar(x, m, yerr=h, color=style.color(k), marker="o", lw=2, capsize=3, label=f"2W {tw:.0%}")
    for ax, title in zip(
        axes, ("Throughput (veh/h)", "Stop-and-wait per 1,000", "Voids (% of 10 s)"), strict=True
    ):
        ax.set_title(title)
        ax.set_xlabel("Late-realiser share")
    axes[0].legend(frameon=False)
    fig.tight_layout()
    style.save(fig, str(OUT / "P7_split"))
    plt.close(fig)

    (OUT / "P7_milestone.json").write_text(
        json.dumps(
            {
                "split_summary": [
                    {k: (list(v) if isinstance(v, tuple) else v) for k, v in r.items()} for r in summary
                ],
                "spearman": mono,
                "split_guard_events": guard_total,
                "runs": by_kind,
            },
            indent=1,
            default=float,
        )
        + "\n"
    )
    print("\n".join(md))


if __name__ == "__main__":
    main()
