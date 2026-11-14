"""P7.4 stage-4 synthetic recovery: do the critical gaps come back out of the simulation?

Runs the priority crossing (`scenarios/priority_crossing.yaml`: the east-west road is major)
with no buses, trucks or tractors anywhere, so that every driver decides by its own critical
gap (the heavy-stream factor never applies), at flows where gaps of 4-10 s come by often
enough. From the `gap_accepted` and `gap_rejected` events it estimates, by vehicle class, the
critical-gap population with the two gap-acceptance estimators of `marg.gaps`:

- Troutbeck's maximum likelihood (lognormal), which should recover the register's inputs
  (`interaction.gap_acceptance.critical_gap.*`: median and sigma), and
- Raff's method, as Abhigna et al. (2020) report it beside their clearing-behaviour values.

Writes reports/phases/P7_gaps.json and prints a summary.

Usage: uv run python scripts/p7_gap_recovery.py [--seeds 4] [--duration 3600]
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
from marg import gaps

ROOT = Path(__file__).resolve().parent.parent
MARG = ROOT / "target" / "release" / "marg"
SCENARIO = ROOT / "scenarios" / "priority_crossing.yaml"
REGISTER = ROOT / "params" / "base" / "interaction.yaml"
CLASSES = ["two_wheeler", "auto_rickshaw", "car"]

# No heavy vehicles: the drivers' own critical gaps decide every gap.
MAJOR = {"motorcycle": 0.35, "scooter": 0.15, "auto_rickshaw": 0.15, "hatchback": 0.25, "sedan": 0.10}
MINOR = {"motorcycle": 0.35, "scooter": 0.15, "auto_rickshaw": 0.2, "hatchback": 0.2, "sedan": 0.1}
FLOWS = {"e": 300, "w": 300, "n": 150, "s": 150}


def scenario(duration: int) -> dict:
    spec = yaml.safe_load(SCENARIO.read_text())
    spec["name"] = "gap_recovery"
    spec["duration_s"] = duration
    for e in spec["demand"]["entries"]:
        e["flow"] = [{"from_s": 0, "veh_per_h": FLOWS[e["id"]]}]
        e["composition"] = MAJOR if e["id"] in ("e", "w") else MINOR
    return spec


def run(spec_path: Path, seed: int, out: Path) -> Path:
    folder = out / f"run_{seed}"
    subprocess.run(
        [str(MARG), "run", str(spec_path), "--seed", str(seed), "--out", str(folder)],
        check=True,
        capture_output=True,
    )
    return folder


def truths() -> dict[str, dict[str, float]]:
    reg = yaml.safe_load(REGISTER.read_text())["entries"]
    out = {}
    for c in CLASSES:
        v = reg[f"gap_acceptance.critical_gap.{c}"]["value"]
        assert v["dist"] == "log_normal_median"
        out[c] = {"median": float(v["median"]), "sigma": float(v["sigma"])}
    return out


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--seeds", type=int, default=4)
    ap.add_argument("--duration", type=int, default=3600)
    args = ap.parse_args()
    truth = truths()
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        spec_path = tmp / "gap_recovery.yaml"
        spec_path.write_text(yaml.safe_dump(scenario(args.duration), sort_keys=False))
        with ThreadPoolExecutor() as pool:
            folders = list(pool.map(lambda s: run(spec_path, s, tmp), range(1, args.seeds + 1)))
        tables = []
        for seed, f in enumerate(folders, 1):
            t = gaps.gap_table(f).with_columns(pl.lit(seed).alias("seed"))
            tables.append(t)
        table = pl.concat(tables)
    heavy = table.filter(pl.col("against_class").is_in(list(gaps.HEAVY))).height
    per = {}
    for c in CLASSES:
        rows = table.filter(pl.col("class") == c)
        d = gaps.drivers(rows.with_columns((pl.col("seed") * 10**18 + pl.col("agent_id")).alias("agent_id")))
        fit = gaps.mle(d["accepted"].to_numpy(), d["largest_rejected"].to_numpy())
        acc = rows.filter(pl.col("accepted"))["gap_s"].to_numpy()
        rej = rows.filter(~pl.col("accepted"))["gap_s"].to_numpy()
        # The critical gaps these drivers drew (logged with each decision).
        drawn = np.log(
            rows.group_by(["seed", "agent_id"]).agg(pl.col("critical_gap_s").min())["critical_gap_s"].to_numpy()
        )
        t = truth[c]
        per[c] = {
            "truth_median_s": t["median"],
            "truth_sigma": t["sigma"],
            "drawn_median_s": round(math.exp(drawn.mean()), 3),
            "drawn_sigma": round(float(drawn.std()), 3),
            "z_median_vs_drawn": round((fit.median - math.exp(drawn.mean())) / fit.median_se, 2),
            "drivers": fit.drivers,
            "accepted": int(acc.size),
            "rejected": int(rej.size),
            "mle_median_s": round(fit.median, 3),
            "mle_median_se": round(fit.median_se, 3),
            "mle_sigma": round(fit.sigma, 3),
            "mle_sigma_se": round(fit.sigma_se, 3),
            "z_median": round((fit.median - t["median"]) / fit.median_se, 2),
            "z_sigma": round((fit.sigma - t["sigma"]) / fit.sigma_se, 2),
            "raff_s": round(gaps.raff(acc, rej), 3),
        }
    result = {
        "scenario": "priority_crossing.yaml, no heavy vehicles",
        "flows_veh_h": FLOWS,
        "seeds": args.seeds,
        "duration_s": args.duration,
        "gaps_against_heavy": heavy,
        "classes": per,
        "abhigna_2020_table4_s": {
            "note": "Same data, two methods (intersection-1 / intersection-2)",
            "clearing_behaviour": {"2w": [4.84, 4.03], "3w": [7.08, 6.53], "4w": [8.59, 7.69]},
            "raff": {"2w": [2.6, 2.75], "3w": [2.27, 4.05], "4w": [2.8, 4.8]},
        },
    }
    out = ROOT / "reports" / "phases" / "P7_gaps.json"
    out.write_text(json.dumps(result, indent=2) + "\n")
    print(f"gaps against heavy vehicles: {heavy}")
    for c, r in per.items():
        print(
            f"{c:14s} n={r['drivers']:4d} truth {r['truth_median_s']:.2f}/{r['truth_sigma']:.2f} "
            f"drawn {r['drawn_median_s']:.2f}/{r['drawn_sigma']:.3f}  "
            f"MLE {r['mle_median_s']:.2f}±{r['mle_median_se']:.2f} (z {r['z_median']:+.1f}, "
            f"vs drawn {r['z_median_vs_drawn']:+.1f}) "
            f"sigma {r['mle_sigma']:.3f}±{r['mle_sigma_se']:.3f} (z {r['z_sigma']:+.1f})  "
            f"Raff {r['raff_s']:.2f}"
        )
    print(f"wrote {out.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
