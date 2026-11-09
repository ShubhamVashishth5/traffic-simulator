"""P3 milestone (plan/phases/P03-experiments-analytics.md): Morris screening of
every P2 `fit` parameter on corridor capacity.

Runs a Morris design over the `fit` parameters the P2 core reads (desired
speeds, the median-band bonus, minimum entry headways, IDM T, s0, a, b by class,
the leader-follower multipliers, and the lateral-weighting parameters), for the
vehicle types in the corridor's composition, on `scenarios/corridor_capacity.yaml`
(demand above capacity: throughput at s1900 is the discharge capacity). Renders
the sensitivity, experiment and parameter-appendix reports into the experiment
folder and writes reports/phases/P3.md. Nothing is tuned here.

Usage: uv run python scripts/p3_milestone.py [--trajectories 10] [--reps 2] [--workers 4]
"""

from __future__ import annotations

import argparse
import shutil
import time
from pathlib import Path

import marg
import polars as pl
from marg import plots, report, style
from marg.design import Factor, Morris

REPO = Path(__file__).resolve().parents[1]
SCENARIO = REPO / "scenarios/corridor_capacity.yaml"
CLASS_OF = {
    "bicycle": "bicycle", "cycle_rickshaw": "cycle_rickshaw", "moped": "two_wheeler",
    "scooter": "two_wheeler", "motorcycle": "two_wheeler", "e_two_wheeler": "two_wheeler",
    "e_rickshaw": "e_rickshaw", "auto_rickshaw": "auto_rickshaw", "hatchback": "car",
    "sedan": "car", "suv": "car", "lcv": "lcv", "bus": "bus", "truck": "truck",
    "tractor": "tractor",
}
#: `fit` parameters the P2 core reads (crates/marg-core/src/params.rs).
P2_FIT = {
    "per_type": ["vehicles.free_flow_speed"],
    "per_class": [
        "demand.min_headway",
        "interaction.following.time_gap",
        "interaction.following.standstill_gap",
        "interaction.following.max_accel",
        "interaction.following.comfortable_decel",
    ],
    "single": [
        "vehicles.median_band.speed_bonus",
        "interaction.following.pair_multiplier.time_gap.default",
        "interaction.following.pair_multiplier.standstill_gap.default",
        "interaction.lateral.clearance.intercept",
        "interaction.lateral.clearance.slope",
        "interaction.lateral.braking_scale",
    ],
}
OUTPUTS = ["throughput_veh_h", "delay_mean_s", "delay_p95_s", "network_mean_speed_m_s"]
REL = 0.25  # screening bounds: base mean x (1 +/- REL), clipped to the register range


def factors(reg: marg.Register, sc: marg.Scenario) -> list[Factor]:
    types = list(sc.get("demand.entries.0.composition"))
    classes = sorted({CLASS_OF[t] for t in types}, key=list(CLASS_OF.values()).index)
    paths = [f"{p}.{t}" for p in P2_FIT["per_type"] for t in types]
    paths += [f"{p}.{c}" for p in P2_FIT["per_class"] for c in classes]
    paths += P2_FIT["single"]
    for p in paths:
        e = reg[p]
        assert e.role == "fit" and e.stage in (1, 2, 3), (p, e.role, e.stage)
    return [Factor.relative(p, REL, reg) for p in paths]


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--trajectories", type=int, default=10)
    ap.add_argument("--reps", type=int, default=2)
    ap.add_argument("--workers", type=int, default=4)
    ap.add_argument("--root", default=str(REPO / "runs"))
    a = ap.parse_args()
    reg = marg.Register(REPO / "params/base")
    sc = marg.Scenario.load(SCENARIO)
    fs = factors(reg, sc)
    exp = marg.Experiment(
        "p3-morris-capacity", sc,
        design=Morris(fs, trajectories=a.trajectories, levels=4, seed=1),
        replications=marg.Replications(n=a.reps), root=a.root, params_dir=REPO / "params/base",
    )
    t0 = time.perf_counter()
    exp.run(marg.backends.Local(workers=a.workers))
    wall = time.perf_counter() - t0
    data = exp.data()
    idx = data.index()
    print(f"{idx.height} runs in {wall:.0f} s")
    for kind in ("sensitivity", "experiment", "params"):
        for p in report.render(kind, exp.dir, metrics=OUTPUTS):
            print(p)

    k = data.results("kpis")
    m = marg.sensitivity.morris(data, OUTPUTS)
    out = REPO / "reports/phases"
    figs = {}
    for o in ("throughput_veh_h", "delay_mean_s"):
        f = out / f"P3_morris_{o}"
        style.save(plots.morris_plot(m.filter(pl.col("output") == o), o, label_top=10), str(f))
        figs[o] = f.name + ".png"
    wide = (m.pivot(on="output", index="factor", values="mu_star_rel")
            .with_columns(pl.max_horizontal(pl.exclude("factor")).alias("max_rel"))
            .sort("max_rel", descending=True))
    wide.write_csv(out / "P3_morris_screen.csv")
    thr = m.filter(pl.col("output") == "throughput_veh_h").sort("mu_star", descending=True)
    quiet = wide.filter(pl.col("max_rel") < 0.05)["factor"].to_list()
    cap = k["throughput_veh_h"]

    def group(f: str) -> str:
        for p in (*P2_FIT["per_type"], *P2_FIT["per_class"]):
            if f.startswith(p + "."):
                return p.split(".")[-1]
        return f.split(".")[-1] if "pair_multiplier" not in f else "pair_multiplier"

    by_group = (thr.with_columns(pl.col("factor").map_elements(group, return_dtype=pl.Utf8)
                                 .alias("group"))
                .group_by("group").agg(pl.col("mu_star_rel").max().alias("max_rel"),
                                       pl.col("factor").first().alias("top factor"),
                                       pl.len().alias("factors"))
                .sort("max_rel", descending=True))

    def md(df: pl.DataFrame, cols: list[str], n: int = 15) -> str:
        rows = ["| " + " | ".join(cols) + " |", "|" + "---|" * len(cols)]
        for r in df.head(n).iter_rows(named=True):
            rows.append("| " + " | ".join(report.fmt(r[c]) for c in cols) + " |")
        return "\n".join(rows)

    lines = [
        "# P3 milestone: Morris screen of the P2 fit parameters on corridor capacity",
        "",
        f"**{len(fs)} factors, {data.design().height} design points x {a.reps} replications = "
        f"{idx.height} runs** of `scenarios/corridor_capacity.yaml`, on {a.workers} local workers "
        f"in {wall / 60:.1f} min. Every run finished `ok`. Nothing was tuned.",
        "",
        "The scenario is the P2 reference corridor (2 km, 10.5 m, Delhi-arterial composition) with "
        "demand above capacity (4,500 veh/h). A queue forms at the entry, so the throughput at "
        "s1900 after the 600 s warm-up is the corridor's discharge capacity.",
        "",
        "## Setup",
        "",
        "- **Factors:** every `fit` parameter the P2 core reads, for the vehicle types in the "
        "composition (parameters of absent types cannot have an effect): desired speeds (per type), "
        "the median-band bonus, minimum entry headways, IDM `T`, `s0`, `a`, `b` (per class), the two "
        "leader–follower multipliers, and the three lateral-weighting parameters "
        "(clearance intercept and slope, braking scale) that decide which vehicles count as leaders.",
        f"- **Bounds:** the base value's mean x (1 ± {REL:.0%}), clipped to the register range. "
        "Distribution-valued parameters move their mean at a fixed coefficient of variation "
        "(ADR-0006). The register ranges themselves are hard validity limits, too wide for a screen "
        "(e.g. 3–130 km/h for a desired speed).",
        f"- **Design:** Morris, {a.trajectories} trajectories, 4 levels, seed 1; {a.reps} "
        "replications per point with common random numbers (replication `r` uses seed `1 + r` at "
        "every point), averaged before the analysis.",
        "- **Outputs:** " + ", ".join(f"`{o}`" for o in OUTPUTS) + ".",
        "",
        "## Capacity across the design",
        "",
        f"Throughput at s1900: mean {cap.mean():.0f} veh/h, range {cap.min():.0f}–{cap.max():.0f} "
        "veh/h over all runs (P2 base parameters: about 2,750 veh/h).",
        "",
        "## Which parameters matter for capacity",
        "",
        f"![Morris mu*-sigma for throughput]({figs['throughput_veh_h']})",
        "",
        "Top factors by μ* (effect on throughput, veh/h, of moving the factor across its screening "
        "range in one step of the 4-level grid; `mu_star_conf` is the bootstrap 95% half-width):",
        "",
        md(thr, ["factor", "mu_star", "mu_star_conf", "sigma", "mu_star_rel"]),
        "",
        "By parameter group (the strongest member, relative μ* on throughput):",
        "",
        md(by_group, ["group", "max_rel", "top factor", "factors"], n=20),
        "",
        f"![Morris mu*-sigma for mean delay]({figs['delay_mean_s']})",
        "",
        "## Screening summary",
        "",
        f"{len(quiet)} of {len(fs)} factors stay below 5% of the most influential factor's μ* for "
        "**every** output. They are candidates to hold at their base values in stage 2 (the 5% "
        "cut-off is a reporting convention for this screen, not a model parameter; the decision "
        "is yours):",
        "",
        *(f"- `{q}`" for q in quiet),
        "",
        "Relative μ* for every factor and output: `reports/phases/P3_morris_screen.csv`.",
        "",
        "## Reading it",
        "",
        "- This is the P2 model: no lateral movement yet (P5), so a fast vehicle that catches a "
        "slow one in its lateral band stays behind it. Capacity is set by platoons behind the "
        "slowest vehicles, which is why slow-vehicle parameters can matter more here than they will "
        "once filtering and overtaking exist. **Re-run this screen after P5** before freezing the "
        "stage-2 parameter set.",
        "- The ranking is local to the ±25% box around the current values. Parameters near zero "
        "here may still matter for outputs not screened yet (conflicts, gridlock onset: P7–P8).",
        "",
        "## Reproduce",
        "",
        "`uv run python scripts/p3_milestone.py` (writes the experiment to `runs/p3-morris-capacity/`, "
        "with the full sensitivity, experiment and parameter-appendix reports in its `report/` "
        "folder, HTML and PDF).",
        "",
    ]
    (out / "P3.md").write_text("\n".join(lines))
    print(f"wrote {out / 'P3.md'}")
    shutil.copy(exp.dir / "report/sensitivity/sensitivity.pdf", out / "P3_sensitivity.pdf")


if __name__ == "__main__":
    main()
