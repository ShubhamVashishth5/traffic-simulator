"""P8 measurements and milestone (plan/phases/P08-personas-violations.md).

Runs, with the shipped register, one assumed set of shared rule coefficients (the guide has
none: G§4.1 grade C) and nothing tuned towards any check:

1. **P8.5, red-light timing.** `scenarios/signal_crossing.yaml` without a countdown: the
   `red_light` intercept is fitted (bisection, common random numbers) so that 1 in 17 drivers
   who face a red cross it (TRR 2017, Mumbai). Then, as a check, where in the red those
   crossings fall (a third just after red starts, two-thirds just before green in Mumbai).
2. **P8.5, countdown.** The same with a working countdown and `early_start` on (its intercept
   assumed equal to the fitted `red_light` one): the share of cycles with a crossing on red in
   the first and in the last third of the red, with and without the countdown, 95% CIs over
   seeds (Sharma et al. 2012: end of red 12% → 75%, start 59% → 31%; a check).
3. **P8.6, imitation.** `early_start` alone at a working countdown, swept over the
   violators-moving coefficient βs⁺ for two values of the compliers-waiting coefficient βs⁻:
   the early-start share (acts per episode).
4. **P8.8, persona shares.** The delivery-rider share (within two-wheelers) against the
   e-rickshaw share of the stream, every violation rule on: KPIs over both. The shares come
   from `marg.templates` (the reusable form of this sweep: `marg.templates.persona_shares`).
5. **P8.4, the new types.** Stop-line seepage against the measured saturation flow (G§4.2:
   standing past the line raises it, a check); opposite-carriageway encroachment at a level
   crossing on an undivided road (`scenarios/undivided_level_crossing.yaml`), against its
   intercept.
6. **Milestone: the dark-signal experiment (G§6.6).** The crossing working (fixed plan,
   countdown) and dark, each with its cooperative twin (`marg.bundles.cooperative`: no
   violations, squeezing at the 25th-percentile κ, and at a dark junction first come first
   served at the lines, ADR-0019 D1), swept over the aggressive-persona (rushed driver) share:
   throughput, delay, conflicts, gridlock and the social efficiency deficit (excess of total
   delay over the twin).

Writes reports/phases/P8.md, reports/phases/P8_milestone.json and reports/phases/P8_dark.png.

Usage: uv run python scripts/p8_milestone.py [--seeds 5] [--jobs 4] [--from-json]
"""

from __future__ import annotations

import argparse
import json
import math
import subprocess
import tempfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import marg
import numpy as np
import polars as pl
import yaml
from marg import templates
from scipy import stats as sps

ROOT = Path(__file__).resolve().parent.parent
MARG = ROOT / "target" / "release" / "marg"
OUT = ROOT / "reports" / "phases"
SCEN = ROOT / "scenarios"

#: One assumed coefficient set shared by every type (G§4.1 has no numbers; the same values as
#: the P8.4 forced-entry measurement, plus time saved). Sweep these; never tune them to a check.
SHARED = {
    "violations.coefficients.aggressiveness": 0.5,
    "violations.coefficients.time_saved": 0.05,
    "violations.coefficients.waiting_time": 0.08,
    "violations.coefficients.imitation_violators": 0.5,
    "violations.coefficients.imitation_compliers": 0.2,
    "violations.coefficients.enforcement": 1.0,
    "violations.coefficients.risk": 2.0,
}
#: Intercepts assumed where the guide has no rate to fit (P8.4's forced entry; the others by
#: analogy). The red-light intercept is fitted in step 1; early start takes the same value.
ASSUMED = {
    "violations.forced_entry.intercept": -5.0,
    "violations.box_blocking.intercept": -5.0,
    "violations.amber_go.intercept": -1.0,
}
TARGET_SHARE = 1.0 / 17.0  # TRR 2017 (Mumbai): about 1 in 17 drivers ran the red
#: veh/h per arm: queues at every red, inside the range where P7.2 found the box robust (at
#: 500 the fixed signal locked in 1-2 of 5 seeds, in the cooperative twin as well).
FLOW = 400.0
#: A run is "locked" if less than this share of the demand leaves the junction.
LOCKED_SHARE = 0.5
DURATION, WARMUP = 1800, 300
BETA_PLUS = [0.0, 1.0, 2.0, 3.0, 4.0, 6.0, 8.0, 10.0]  # the register's range ends at 10
#: Sensitivity, not a fit: early-start intercepts above the red-light one (added to it).
EARLY_SHIFTS = [3.0, 6.0]
BETA_MINUS = [0.0, 1.0]
RUSHED = [0.0, 0.25, 0.5]
DELIVERY = [0.0, 0.15, 0.3]
E_RICKSHAW = [0.03, 0.10, 0.20]
#: P8.4: assumed intercepts for the new types (no rates in the guide), and the level-crossing
#: sweep over the encroachment intercept.
SEEPAGE = -3.0
#: More seeds for seepage: at 400 veh/h the crossing locks on some seeds in either arm.
SEEPAGE_SEEDS = 8
ENCROACH = [-10.0, -8.0, -6.0]

FINDINGS = """
## Findings

Nothing below was tuned. The coefficients are assumed, and only the red-light intercept was
fitted, to its rate. Since the previous report the code changed in six ways (the user's
decisions of 2026-11-16):

- the cooperative twin takes turns at a dark junction, first come first served (ADR-0019 D1);
- the field of view is measured from mid-length (ADR-0018);
- the left-hook rules (ADR-0016 open problem 3);
- the split options A2, B2 and C2 (ADR-0017);
- the delivery-rider share is exact within two-wheelers (`marg.templates`);
- the new violation types (ADR-0020);
- at amber onset, a vehicle that can still stop loses any gate it had (ADR-0016).

1. **P8.5, rate.** The red-light intercept fits 1 in 17, but noisily.
   - The share is not monotone in β0 near the fit (−8.094 gives 7.2%, −8.141 gives 5.4%).
     Runners change the traffic around them: who is at the front, imitation, locked runs.
   - β0 = −8.11 gave 6.0% against the 5.9% target. Treat the fit as β0 ≈ −8.1 ± 0.1. It was
     −7.68 before the left-hook rules and the closed gates at amber (ADR-0016) changed the
     junction's traffic.
2. **P8.5, timing regimes (a check): partly met.**
   - 31% of red runs fall in the first third of the red, 24% in the middle and 45% in the
     last. Mumbai has about a third and two-thirds.
   - The after-red-starts regime emerges: time saved is largest and the cross streams are
     still in their all-red.
   - The before-green regime is weaker than observed. Its terms are the cross streams
     emptying at their amber and all-red, and waiting time.
3. **P8.5, countdown (a check): not met with equal intercepts.**
   - With a working countdown and the early-start intercept equal to the red-light one, the
     start and end of red barely move (5.4% → 6.8%, 8.9% → 7.7%). Early starts are rare
     (about 2 per run).
   - 3 or 6 above the red-light intercept gives 20% and 50% at the end of red, with the start
     of red at 4.6% and 5.2%: the mechanism can produce Sharma's shift.
   - Sharma's 12% → 75% would need early start to be far more likely, per second, than
     running the red. That is a calibration question (stage 6).
4. **P8.6, imitation (done-when still not met).** With the field of view measured from
   mid-length, the early-start share rises from 5.6% at βs⁺ = 0 to about 10% at βs⁺ ≥ 6
   (3.8% → 9–10% with βs⁻ = 1). It roughly doubles, then levels off, with no sharp tipping.
   βs⁻ = 1 lowers it at 7 of 8 points, mostly inside the CIs.
   - An early-start episode lasts at most the 5 s change interval, and only vehicles standing
     at the front of the line are in one. Few are in view of each other at once, and a crowd
     to tip does not form.
   - What could: early start decided further back in the queue, or a longer window (W2 in
     ADR-0019). Both are modelling choices, for review.
5. **P8.8, persona shares** (now `marg.templates.persona_shares`).
   - Violations per 1,000 vehicles do not move measurably with the delivery-rider share:
     64–71 at 0, 67–90 at 30% of two-wheelers, with wide CIs (3 seeds per point). The
     previous report's rise (65–78 → 81–97) is within the noise.
   - Delay (about 27 s) and throughput do not move measurably with either share.
   - E-rickshaws (3–20% of the stream) change nothing at this junction.
6. **P8.4, the new types.**
   - **Seepage (check not met).** About 240 vehicles a run crept past a red line. The
     saturation flow measured at the line rose from 227 ± 34 to 246 ± 85 veh/h/m, G§4.2's
     direction but well inside the CI.
     - The measure counts the queue crossing the line at green. A vehicle already past the
       line is not in it, and the queue behind starts no sooner.
     - A head start measured where the field counts it (Ramireddy et al.) needs their
       definition of the discharge; not built.
     - Locked runs: 0 of 8 without seepage, 1 of 8 with it. 2 of 8 held a pair in the guard.
   - **Opposite encroachment at a level crossing: a negative-sum game.**
     - Each encroacher saves itself the queue, and everyone loses: vehicles through 86% →
       73%, 42%, 20% as the intercept rises, gridlock share 0 → 0.10, 0.38, 0.50, conflicts
       three to six times as many.
     - When the gate opens, the two streams meet head on with nowhere to go (vehicles cannot
       reverse in the model), as at Indian level crossings.
     - At −8 a run can hold a head-on pair in the collision guard (441 ± 1,142 events per run).
7. **Milestone: the dark-signal experiment, with the coordinated twin (D1).**
   - **Dark.** Violations (forced entries, 350–410 per 1,000 vehicles) no longer clearly cut
     delay: SED −28 ± 47%, +10 ± 142%, −7 ± 40% for rushed shares 0, 0.25 and 0.5. Every CI
     spans zero. The previous report's −73% to −81% came from a twin that waited for gaps that
     rarely came.
   - Taking turns, the twin serves the same throughput at 55–61 s of mean delay, against
     48–56 s observed, with about as many conflicts per vehicle. At 400 veh/h per arm, the
     agreed order is about as slow as the Chicken of the observed drivers.
   - **The guide's negative-sum game does not appear at the dark junction.** It does appear
     at the level crossing (6).
   - **Fixed signal.**
     - With a quarter of drivers rushed, red-light running cuts delay by 12 ± 4% (5 pairs).
     - With none, the SED is +120 ± 371%: one observed run nearly locked (818 veh/h, just
       above the lock threshold).
     - With half rushed, −9 ± 30% over the 2 pairs that did not lock.
8. **Robustness.**
   - The dark runs' guard loops are gone after the left-hook rules: at most 6 guard events
     per run, against 40,318 and 4,652 before.
   - Junction lock-ups at 400 veh/h remain (ADR-0016 open problem 1), in every arm: 0–2 of 5
     runs.
   - One fixed-signal run with red runners held a pair in the guard (23,078 events).
   - A gap taken far upstream for a wait inside the box no longer opens a line that turns red
     (ADR-0016): without violations nobody now crosses on red but those that could not stop.
"""


# ------------------------------------------------------------------ scenarios


def crossing(countdown: str = "none", control: dict | None = None, composition=None) -> dict:
    spec = yaml.safe_load((SCEN / "signal_crossing.yaml").read_text(encoding="utf-8"))
    spec["duration_s"], spec["warmup_s"] = DURATION, WARMUP
    j = spec["network"]["junctions"][0]
    j["control"]["countdown"] = countdown
    j["control"].update(control or {})
    for e in spec["demand"]["entries"]:
        e["flow"] = [{"from_s": 0, "veh_per_h": FLOW}]
        if composition is not None:
            e["composition"] = composition(e["composition"])
    # The last 60 m of each approach, for the gridlock indicator (plan/04 §3).
    for arm in ("e", "n", "w", "s"):
        spec["detectors"].append(
            {
                "kind": "area",
                "id": f"q_{arm}",
                "ribbon": f"{arm}_in",
                "s_from": 240,
                "s_to": 300,
                "interval_s": 60,
            }
        )
    return spec


with_e_rickshaws = templates.with_e_rickshaws


def personas(rushed: float = 0.15, delivery: float = 0.1) -> dict[str, float]:
    """All ten persona shares (a population is all or none, ADR-0018 part 6), from
    `marg.templates`: the delivery rider is exactly `delivery` of two-wheelers. Shares are
    renormalised within each vehicle class; classes with one persona take it whatever the
    share."""
    return templates.persona_overrides(
        delivery,
        {"average_commuter": max(0.85 - rushed, 0.0), "rushed_driver": rushed},
    )


def run_marg(spec: dict, seed: int, sets: dict, tmp: Path, tag: str) -> Path:
    path = tmp / f"{tag}.yaml"
    path.write_text(yaml.safe_dump(spec, sort_keys=False), encoding="utf-8")
    folder = tmp / tag
    cmd = [str(MARG), "run", str(path), "--seed", str(seed), "--out", str(folder)]
    for k, v in sets.items():
        cmd += ["--set", f"{k}={v}"]
    subprocess.run(cmd, check=True, capture_output=True)
    return folder


# -------------------------------------------------------------------- metrics


def kpis(folder: Path) -> dict[str, float]:
    k = pl.read_parquet(folder / "kpis.parquet").to_dicts()[0]
    return {key: (math.nan if v is None else v) for key, v in k.items()}


def attr(col: str, path: str) -> pl.Expr:
    return pl.col(col).str.json_path_match(path)


def crossings(folder: Path) -> pl.DataFrame:
    ev = pl.read_parquet(folder / "events.parquet").filter(
        (pl.col("kind") == "stop_line_crossed") & (pl.col("t") >= WARMUP)
    )
    return ev.with_columns(
        attr("attrs", "$.queued").alias("queued"),
        attr("attrs", "$.light").cast(pl.Int32).alias("light"),
        attr("attrs", "$.act").alias("act"),
        attr("attrs", "$.into_red_s").cast(pl.Float64).alias("into_red"),
        attr("attrs", "$.red_s").cast(pl.Float64).alias("red_len"),
    )


def red_metrics(folder: Path) -> dict:
    c = crossings(folder)
    facing = c.filter((pl.col("queued") == "true") | (pl.col("light") == 3)).height
    runs = c.filter((pl.col("light") == 3) & pl.col("act").is_in(["red_light", "early_start"]))
    frac = (runs["into_red"] / runs["red_len"]).to_numpy() if runs.height else np.array([])
    cyc = pl.read_parquet(folder / "signal_cycles.parquet").filter(pl.col("t_green") >= WARMUP)
    # The last cycle of each arm has a red cut off by the end of the run.
    last = cyc.group_by("arm").agg(pl.col("cycle").max().alias("last"))
    cyc = cyc.join(last, on="arm").filter(pl.col("cycle") < pl.col("last"))
    return {
        "facing_red": facing,
        "runs": runs.height,
        "share": runs.height / facing if facing else math.nan,
        "fractions": frac.tolist(),
        "cycles": cyc.height,
        "cycles_start": int((cyc["red_crossings_early"] > 0).sum()),
        "cycles_end": int((cyc["red_crossings_late"] > 0).sum()),
        "early_starts": kpis(folder)["early_starts"],
        "locked": traffic(folder)["locked"],
    }


def early_share(folder: Path) -> dict:
    ev = pl.read_parquet(folder / "events.parquet").filter(
        pl.col("kind").is_in(["violation", "complied"]) & (pl.col("t") >= WARMUP)
    )
    ev = ev.filter(attr("attrs", "$.type") == "early_start")
    acts = ev.filter(pl.col("kind") == "violation").height
    return {
        "acts": acts,
        "episodes": ev.height,
        "share": acts / ev.height if ev.height else math.nan,
    }


def delay_total(folder: Path) -> float:
    """Total delay of every vehicle that arrived in the measured window (s): time lost on the
    road against free flow, plus the wait to enter."""
    ag = pl.read_parquet(folder / "agents.parquet").filter(pl.col("t_arrival") >= WARMUP)
    return float((ag["delay"].fill_null(0.0) + ag["spawn_delay"].fill_null(0.0)).sum())


def traffic(folder: Path) -> dict:
    k = kpis(folder)
    spawned = max(k["spawned"], 1.0)
    ag = pl.read_parquet(folder / "agents.parquet")
    out = _traffic(folder, k, spawned, ag)
    out["locked"] = float(out["throughput"] < LOCKED_SHARE * 4 * FLOW)
    return out


def _traffic(folder: Path, k: dict, spawned: float, ag: pl.DataFrame) -> dict:
    return {
        "throughput": sum(
            pl.read_parquet(folder / "detectors.parquet").filter(
                pl.col("detector_id").str.starts_with("out_") & (pl.col("t_start") >= WARMUP)
            )["count"]
        )
        * 3600.0
        / (DURATION - WARMUP),
        "delay_mean": k["delay_mean_s"],
        "delay_total": delay_total(folder),
        "conflicts_per_1000": 1000.0 * k["conflicts"] / spawned,
        "violations_per_1000": k["violations_per_1000_veh"],
        "red_runs_per_1000": 1000.0 * k["red_light_runs"] / spawned,
        "forced_per_1000": 1000.0 * k["forced_entries"] / spawned,
        "box_blocks_per_1000": 1000.0 * k["box_blocks"] / spawned,
        "gridlock_share": k["gridlock_share"],
        "gridlock_onset": k["gridlock_onset_s"],
        "guard": k["collision_guard_events"],
        "locked": 0.0,
        "rushed_realised": share_of(ag["persona"], "rushed_driver"),
        "delivery_realised": share_of(
            ag.filter(pl.col("class") == "two_wheeler")["persona"], "delivery_rider"
        ),
    }


def share_of(persona: pl.Series, key: str) -> float:
    """Share of agents with persona `key` (0 when personas are off)."""
    if persona.len() == 0:
        return 0.0
    return float((persona.fill_null("") == key).sum()) / persona.len()


def ci(xs: list[float]) -> tuple[float, float]:
    xs = [x for x in xs if not math.isnan(x)]
    if not xs:
        return math.nan, math.nan
    m = float(np.mean(xs))
    if len(xs) < 2:
        return m, math.nan
    return m, float(sps.t.ppf(0.975, len(xs) - 1) * np.std(xs, ddof=1) / math.sqrt(len(xs)))


def fmt(m: float, h: float, digits: int = 0) -> str:
    if math.isnan(m):
        return "—"
    if math.isnan(h):
        return f"{m:.{digits}f}"
    return f"{m:.{digits}f} ± {h:.{digits}f}"


# ---------------------------------------------------------------------- steps


def fit_red(pool, tmp: Path, seeds: list[int]) -> dict:
    """Bisection on the red-light intercept: the pooled share of drivers facing red who run
    it, over common seeds, is monotone in it (one threshold per episode, ADR-0018)."""
    spec = crossing("none")

    def share(b0: float) -> tuple[float, list[dict]]:
        sets = {**SHARED, "violations.red_light.intercept": b0}
        tag = f"fit_{b0:+.4f}".replace(".", "p")
        ms = list(
            pool.map(lambda s: red_metrics(run_marg(spec, s, sets, tmp, f"{tag}_{s}")), seeds)
        )
        runs = sum(m["runs"] for m in ms)
        facing = sum(m["facing_red"] for m in ms)
        return runs / facing, ms

    lo, hi = -14.0, -2.0
    history = []
    for _ in range(9):
        mid = 0.5 * (lo + hi)
        s, ms = share(mid)
        history.append((mid, s))
        if s < TARGET_SHARE:
            lo = mid
        else:
            hi = mid
    b0 = 0.5 * (lo + hi)
    s, ms = share(b0)
    frac = np.concatenate([np.array(m["fractions"]) for m in ms]) if ms else np.array([])
    per_seed = [m["share"] for m in ms]
    return {
        "intercept": b0,
        "share": s,
        "share_ci": ci(per_seed),
        "history": history,
        "runs": int(sum(m["runs"] for m in ms)),
        "facing": int(sum(m["facing_red"] for m in ms)),
        "locked": int(sum(m["locked"] for m in ms)),
        "first_third": float(np.mean(frac < 1 / 3)) if frac.size else math.nan,
        "middle_third": float(np.mean((frac >= 1 / 3) & (frac < 2 / 3))) if frac.size else math.nan,
        "last_third": float(np.mean(frac >= 2 / 3)) if frac.size else math.nan,
        "histogram": np.histogram(frac, bins=10, range=(0, 1))[0].tolist() if frac.size else [],
        "per_seed_thirds": [
            [
                float(np.mean(np.array(m["fractions"]) < 1 / 3)) if m["fractions"] else math.nan,
                float(np.mean(np.array(m["fractions"]) >= 2 / 3)) if m["fractions"] else math.nan,
            ]
            for m in ms
        ],
    }


def countdown(pool, tmp: Path, seeds: list[int], b0: float) -> dict:
    out = {}
    arms = [("none", "none", None), ("working", "working", 0.0)]
    arms += [(f"working, early start β0 {shift:+g}", "working", shift) for shift in EARLY_SHIFTS]
    for name, cd, early in arms:
        sets = {**SHARED, "violations.red_light.intercept": b0}
        if early is not None:
            sets["violations.early_start.intercept"] = b0 + early
        spec = crossing(cd)
        ms = list(
            pool.map(
                lambda s, spec=spec, sets=sets, name=name, early=early: red_metrics(
                    run_marg(spec, s, sets, tmp, f"cd_{name.split(',')[0]}_{early}_{s}")
                ),
                seeds,
            )
        )
        start = [m["cycles_start"] / m["cycles"] for m in ms if m["cycles"]]
        end = [m["cycles_end"] / m["cycles"] for m in ms if m["cycles"]]
        frac = np.concatenate([np.array(m["fractions"]) for m in ms])
        out[name] = {
            "start": ci(start),
            "end": ci(end),
            "share": ci([m["share"] for m in ms]),
            "early_starts": ci([m["early_starts"] for m in ms]),
            "first_third": float(np.mean(frac < 1 / 3)) if frac.size else math.nan,
            "last_third": float(np.mean(frac >= 2 / 3)) if frac.size else math.nan,
            "cycles": int(sum(m["cycles"] for m in ms)),
            "locked": int(sum(m["locked"] for m in ms)),
        }
    return out


def imitation(pool, tmp: Path, seeds: list[int], b0: float) -> list[dict]:
    spec = crossing("working")
    jobs = []
    for bm in BETA_MINUS:
        for bp in BETA_PLUS:
            for s in seeds:
                sets = {
                    **SHARED,
                    "violations.early_start.intercept": b0,
                    "violations.coefficients.imitation_violators": bp,
                    "violations.coefficients.imitation_compliers": bm,
                }
                jobs.append((bm, bp, s, sets))
    res = list(
        pool.map(
            lambda j: (
                j[0],
                j[1],
                early_share(run_marg(spec, j[2], j[3], tmp, f"im_{j[0]}_{j[1]}_{j[2]}")),
            ),
            jobs,
        )
    )
    rows = []
    for bm in BETA_MINUS:
        for bp in BETA_PLUS:
            got = [r[2] for r in res if r[0] == bm and r[1] == bp]
            rows.append(
                {
                    "beta_minus": bm,
                    "beta_plus": bp,
                    "share": ci([g["share"] for g in got]),
                    "acts": float(np.mean([g["acts"] for g in got])),
                    "episodes": float(np.mean([g["episodes"] for g in got])),
                }
            )
    return rows


def all_rules(b0: float) -> dict:
    return {
        **SHARED,
        **ASSUMED,
        "violations.red_light.intercept": b0,
        "violations.early_start.intercept": b0,
    }


def persona_sweep(pool, tmp: Path, seeds: list[int], b0: float) -> list[dict]:
    jobs = []
    for d in DELIVERY:
        for e in E_RICKSHAW:
            spec = crossing("working", composition=lambda c, e=e: with_e_rickshaws(c, e))
            for s in seeds:
                jobs.append((d, e, s, spec, {**all_rules(b0), **personas(delivery=d)}))
    res = list(
        pool.map(
            lambda j: (
                j[0],
                j[1],
                traffic(run_marg(j[3], j[2], j[4], tmp, f"ps_{j[0]}_{j[1]}_{j[2]}")),
            ),
            jobs,
        )
    )
    rows = []
    for d in DELIVERY:
        for e in E_RICKSHAW:
            got = [r[2] for r in res if r[0] == d and r[1] == e]
            moving = [g for g in got if not g["locked"]] or got
            rows.append(
                {
                    "delivery": d,
                    "e_rickshaw": e,
                    **{k: ci([g[k] for g in moving]) for k in got[0]},
                    "locked_runs": int(sum(g["locked"] for g in got)),
                    "runs": got,
                }
            )
    return rows


def saturation(folder: Path) -> float:
    """Median saturation flow per metre of approach over the cycles measured (veh/h/m)."""
    cyc = pl.read_parquet(folder / "signal_cycles.parquet").filter(pl.col("t_green") >= WARMUP)
    vals = cyc["saturation_flow_per_m"].drop_nulls().drop_nans()
    return float(vals.median()) if vals.len() else math.nan


def catalogue(pool, tmp: Path, seeds: list[int]) -> dict:
    """P8.4's new types: seepage against saturation flow at the crossing; encroachment at
    the level crossing on an undivided road, against its intercept."""
    spec = crossing("working")
    jobs = []
    for on in (False, True):
        sets = {**SHARED, **({"violations.stopline_seepage.intercept": SEEPAGE} if on else {})}
        for s in range(1, SEEPAGE_SEEDS + 1):
            jobs.append(("seep", on, s, spec, sets))
    road = yaml.safe_load((SCEN / "undivided_level_crossing.yaml").read_text(encoding="utf-8"))
    for b in [None, *ENCROACH]:
        sets = {**SHARED, **({"violations.opposite_encroachment.intercept": b} if b else {})}
        for s in seeds:
            jobs.append(("road", b, s, road, sets))

    def one(j):
        kind, x, s, sp, sets = j
        folder = run_marg(sp, s, sets, tmp, f"cat_{kind}_{x}_{s}".replace("-", "m"))
        k = kpis(folder)
        out = {
            "spawned": k["spawned"],
            "exited": k["exited"],
            "delay_mean": k["delay_mean_s"],
            "guard": k["collision_guard_events"],
            "conflicts_per_1000": 1000.0 * k["conflicts"] / max(k["spawned"], 1.0),
        }
        if kind == "seep":
            out["saturation_per_m"] = saturation(folder)
            out["seepages"] = k["stopline_seepages"]
            out["locked"] = float(k["exited"] < LOCKED_SHARE * k["spawned"])
        else:
            out["encroachments_per_1000"] = (
                1000.0 * k["opposite_encroachments"] / max(k["spawned"], 1.0)
            )
            out["gridlock_share"] = k["gridlock_share"]
            out["exited_share"] = k["exited"] / max(k["spawned"], 1.0)
        return kind, x, out

    res = list(pool.map(one, jobs))
    seep = []
    for on in (False, True):
        got = [r[2] for r in res if r[0] == "seep" and r[1] == on]
        moving = [g for g in got if not g["locked"]] or got
        seep.append(
            {
                "on": on,
                **{k: ci([g[k] for g in moving]) for k in got[0]},
                "locked_runs": int(sum(g["locked"] for g in got)),
                "runs": len(got),
                "guard_runs": int(sum(g["guard"] > 100 for g in got)),
            }
        )
    road_rows = []
    for b in [None, *ENCROACH]:
        got = [r[2] for r in res if r[0] == "road" and r[1] == b]
        road_rows.append({"intercept": b, **{k: ci([g[k] for g in got]) for k in got[0]}})
    return {"seepage": seep, "level_crossing": road_rows}


def dark_experiment(pool, tmp: Path, seeds: list[int], b0: float) -> list[dict]:
    coop = marg.bundles.cooperative()
    jobs = []
    for r in RUSHED:
        for state in ("fixed", "dark"):
            spec = crossing("working", {"state": state})
            for arm in ("observed", "cooperative"):
                sets = {**all_rules(b0), **personas(rushed=r)}
                if arm == "cooperative":
                    sets.update(coop)
                for s in seeds:
                    jobs.append((r, state, arm, s, spec, sets))
    res = list(
        pool.map(
            lambda j: (
                j[0],
                j[1],
                j[2],
                j[3],
                traffic(run_marg(j[4], j[3], j[5], tmp, f"dk_{j[0]}_{j[1]}_{j[2]}_{j[3]}")),
            ),
            jobs,
        )
    )
    rows = []
    for r in RUSHED:
        for state in ("fixed", "dark"):
            obs = {x[3]: x[4] for x in res if x[0] == r and x[1] == state and x[2] == "observed"}
            co = {x[3]: x[4] for x in res if x[0] == r and x[1] == state and x[2] == "cooperative"}
            # Over the seeds where neither arm locked (a lock is the P7.2
            # robustness limit, not the experiment's effect).
            pairs = [s for s in seeds if not obs[s]["locked"] and not co[s]["locked"]]
            sed = [
                (obs[s]["delay_total"] - co[s]["delay_total"]) / co[s]["delay_total"]
                for s in pairs
                if co[s]["delay_total"] > 0
            ]
            row = {"rushed": r, "state": state, "sed": ci(sed), "sed_pairs": len(sed)}
            for arm, got in (("observed", obs), ("cooperative", co)):
                vals = list(got.values())
                moving = [g for g in vals if not g["locked"]] or vals
                row[arm] = {k: ci([g[k] for g in moving]) for k in vals[0]}
                row[arm]["runs"] = vals
                row[arm]["gridlocked_runs"] = sum(
                    1 for g in vals if not math.isnan(g["gridlock_onset"])
                )
                row[arm]["locked_runs"] = int(sum(g["locked"] for g in vals))
            rows.append(row)
    return rows


# --------------------------------------------------------------------- report


def plot(dark: list[dict], path: Path) -> None:
    import matplotlib

    matplotlib.use("Agg")
    import matplotlib.pyplot as plt

    fig, axes = plt.subplots(1, 3, figsize=(12, 3.6))
    for state, colour in (("fixed", "#2a6fb0"), ("dark", "#c0392b")):
        rows = [r for r in dark if r["state"] == state]
        x = [r["rushed"] for r in rows]
        for ax, key, label in (
            (axes[0], "throughput", "throughput (veh/h)"),
            (axes[1], "delay_mean", "mean delay (s)"),
        ):
            for arm, style in (("observed", "-"), ("cooperative", "--")):
                m = [r[arm][key][0] for r in rows]
                h = [r[arm][key][1] for r in rows]
                ax.errorbar(
                    x, m, yerr=h, fmt="o" + style, color=colour, capsize=3, label=f"{state}, {arm}"
                )
                ax.set_xlabel("rushed-driver share (input)")
                ax.set_ylabel(label)
        axes[2].errorbar(
            x,
            [100 * r["sed"][0] for r in rows],
            yerr=[100 * r["sed"][1] for r in rows],
            fmt="o-",
            color=colour,
            capsize=3,
            label=state,
        )
        axes[2].set_xlabel("rushed-driver share (input)")
        axes[2].set_ylabel("social efficiency deficit (%)")
    axes[0].legend(fontsize=7)
    axes[2].axhline(0, color="#888", lw=0.8)
    fig.tight_layout()
    fig.savefig(path, dpi=150)


def report(res: dict) -> str:
    fit = res["fit"]
    lines = [
        "# P8 report: personas, violations, imitation and signal control states",
        "",
        "Generated by `scripts/p8_milestone.py` from the shipped register. Every rule coefficient",
        "is assumed (the guide gives none, G§4.1 grade C) and was not tuned towards any check:",
        "",
        "| Coefficient | Value |",
        "| --- | --- |",
        *[f"| `{k}` | {v} |" for k, v in SHARED.items()],
        *[f"| `{k}` | {v} |" for k, v in ASSUMED.items()],
        f"| `violations.red_light.intercept` | {fit['intercept']:.3f} (fitted, step 1) |",
        f"| `violations.early_start.intercept` | {fit['intercept']:.3f} (= red light's, assumed) |",
        "",
        f"Signal crossing at {FLOW:.0f} veh/h per arm, every turn, {DURATION} s with {WARMUP} s",
        f"warm-up, {res['seeds']} seeds per point (95% CIs over seeds).",
        "",
        "## 1. P8.5: fitting the red-light intercept to 1 in 17",
        "",
        "Drivers facing red: stop-line crossings by vehicles that had queued at the line or",
        "crossed on red. Runs: crossings on red by a `red_light` act. No countdown.",
        "",
        "| Intercept tried | Share running red |",
        "| --- | --- |",
        *[f"| {b:.3f} | {100 * s:.2f}% |" for b, s in fit["history"]],
        "",
        f"**Fitted: β0 = {fit['intercept']:.3f}**, share {100 * fit['share']:.2f}% "
        f"({fit['runs']} of {fit['facing']}; target {100 * TARGET_SHARE:.2f}%; "
        f"locked runs at the fitted value: {fit.get('locked', 0)} of {res['fit_seeds']}).",
        "",
        "**Check, not fitted: where in the red they run** (time since red began over the red's",
        "length; Mumbai: a third just after red starts, two-thirds just before green):",
        "",
        "| First third | Middle third | Last third |",
        "| --- | --- | --- |",
        f"| {100 * fit['first_third']:.0f}% | {100 * fit['middle_third']:.0f}% | {100 * fit['last_third']:.0f}% |",
        "",
        "Histogram by tenth of the red: " + ", ".join(str(n) for n in fit["histogram"]),
        "",
        "## 2. P8.5: the countdown",
        "",
        "Share of cycles (arm × cycle) with at least one crossing on red in the first or last",
        "third of the red. Sharma et al. 2012 (Chennai), with timers: end of red 12% → 75%,",
        "start of red 59% → 31%.",
        "",
        "| Countdown | Cycles | Start of red | End of red | Share running red | Early starts per run | Locked runs |",
        "| --- | --- | --- | --- | --- | --- | --- |",
    ]
    for name, c in res["countdown"].items():
        lines.append(
            f"| {name} | {c['cycles']} | {fmt(100 * c['start'][0], 100 * c['start'][1], 1)}% | "
            f"{fmt(100 * c['end'][0], 100 * c['end'][1], 1)}% | "
            f"{fmt(100 * c['share'][0], 100 * c['share'][1], 2)}% | "
            f"{fmt(*c['early_starts'], 1)} | {c.get('locked', 0)} |"
        )
    lines += [
        "",
        "## 3. P8.6: imitation",
        "",
        "`early_start` alone, working countdown: acts per episode (a front vehicle standing in",
        "the change interval before its green).",
        "",
        "| βs⁺ | " + " | ".join(f"βs⁻ = {bm}" for bm in BETA_MINUS) + " |",
        "| --- | " + " | ".join("---" for _ in BETA_MINUS) + " |",
    ]
    for bp in BETA_PLUS:
        cells = []
        for bm in BETA_MINUS:
            row = next(
                r for r in res["imitation"] if r["beta_plus"] == bp and r["beta_minus"] == bm
            )
            cells.append(
                f"{fmt(100 * row['share'][0], 100 * row['share'][1], 1)}% ({row['acts']:.0f}/{row['episodes']:.0f})"
            )
        lines.append(f"| {bp} | " + " | ".join(cells) + " |")
    lines += [
        "",
        "## 4. P8.8: persona shares",
        "",
        "Every rule on, working countdown. Delivery-rider share as input (within two-wheelers,",
        "realised in brackets) against the e-rickshaw share of the stream. Means over the runs",
        "that did not lock (a locked run: less than half the demand leaves the junction).",
        "",
        "| Delivery | E-rickshaw | Throughput (veh/h) | Mean delay (s) | Violations /1000 veh | Red runs /1000 veh | Conflicts /1000 veh | Locked runs |",
        "| --- | --- | --- | --- | --- | --- | --- | --- |",
    ]
    for r in res["personas"]:
        lines.append(
            f"| {r['delivery']} ({100 * r['delivery_realised'][0]:.0f}%) | {r['e_rickshaw']} | "
            f"{fmt(*r['throughput'])} | {fmt(*r['delay_mean'], 1)} | {fmt(*r['violations_per_1000'], 1)} | "
            f"{fmt(*r['red_runs_per_1000'], 1)} | {fmt(*r['conflicts_per_1000'], 1)} | "
            f"{r.get('locked_runs', 0)} of {len(r.get('runs', [])) or 3} |"
        )
    cat = res.get("catalogue")
    if cat:
        lines += [
            "",
            "## 5. P8.4: the new types",
            "",
            f"**Stop-line seepage** (intercept {SEEPAGE}, assumed; the other types off), the same",
            f"crossing, {SEEPAGE_SEEDS} seeds: median saturation flow per metre of approach over the",
            "measured cycles (P7.3's measure, over vehicles that did not cross on red). G§4.2:",
            "vehicles standing past the line raise it (Ramireddy et al. 2020; a check). Means over",
            "the runs that did not lock.",
            "",
            "| Seepage | Saturation flow (veh/h/m) | Seepages per run | Mean delay (s) | Locked runs | Runs with a pair held by the guard |",
            "| --- | --- | --- | --- | --- | --- |",
        ]
        for r in cat["seepage"]:
            lines.append(
                f"| {'on' if r['on'] else 'off'} | {fmt(*r['saturation_per_m'])} | "
                f"{fmt(*r['seepages'], 1)} | {fmt(*r['delay_mean'], 1)} | "
                f"{r.get('locked_runs', 0)} of {r.get('runs', '?')} | {r.get('guard_runs', 0)} |"
            )
        lines += [
            "",
            "**Opposite-carriageway encroachment at a level crossing**",
            "(`scenarios/undivided_level_crossing.yaml`: an undivided road, 3.5 m each way; the",
            "barrier closes the eastbound side 90 s in every 180 s; 1,200 veh/h east, 500 west).",
            "",
            "| Intercept | Encroachments /1000 veh | Vehicles through (of those in) | Mean delay (s) | Gridlock share | Conflicts /1000 veh | Guard events |",
            "| --- | --- | --- | --- | --- | --- | --- |",
        ]
        for r in cat["level_crossing"]:
            b = "off" if r["intercept"] is None else f"{r['intercept']}"
            lines.append(
                f"| {b} | {fmt(*r['encroachments_per_1000'], 1)} | "
                f"{fmt(100 * r['exited_share'][0], 100 * r['exited_share'][1])}% | "
                f"{fmt(*r['delay_mean'], 1)} | {fmt(*r['gridlock_share'], 2)} | "
                f"{fmt(*r['conflicts_per_1000'], 1)} | {fmt(*r['guard'], 1)} |"
            )
    lines += [
        "",
        "## 6. Milestone: the dark-signal experiment",
        "",
        "Identical demand and personas; the crossing working (fixed plan, countdown) or dark from",
        "the start; each against its cooperative twin. SED: excess of total delay (on the road and",
        "waiting to enter) over the twin's, over the seeds where neither arm locked (pairs in",
        "brackets). Means over the runs that did not lock; locked runs counted apart (less than",
        "half the demand leaves the junction: the P7.2 robustness limit, in both arms). Gridlock",
        "indicator: runs in which an approach's last 60 m stayed slow for the register's gridlock",
        "duration.",
        "",
        "| Rushed share | State | Arm | Throughput (veh/h) | Mean delay (s) | Conflicts /1000 veh | Violations /1000 veh | Gridlock indicator | Locked runs | SED |",
        "| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |",
    ]
    for r in res["dark"]:
        for arm in ("observed", "cooperative"):
            a = r[arm]
            sed = (
                fmt(100 * r["sed"][0], 100 * r["sed"][1], 1) + f"% ({r.get('sed_pairs', '?')})"
                if arm == "observed"
                else ""
            )
            lines.append(
                f"| {r['rushed']} ({100 * a['rushed_realised'][0]:.0f}%) | {r['state']} | {arm} | "
                f"{fmt(*a['throughput'])} | {fmt(*a['delay_mean'], 1)} | {fmt(*a['conflicts_per_1000'], 1)} | "
                f"{fmt(*a['violations_per_1000'], 1)} | {a['gridlocked_runs']} of {res['seeds']} | "
                f"{a['locked_runs']} | {sed} |"
            )
    lines += ["", "![dark-signal experiment](P8_dark.png)", "", FINDINGS.strip(), ""]
    return "\n".join(lines)


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--seeds", type=int, default=5)
    ap.add_argument("--jobs", type=int, default=4)
    ap.add_argument("--from-json", action="store_true")
    ap.add_argument(
        "--only",
        default="",
        help="comma-separated steps: fit,countdown,imitation,personas,catalogue,dark",
    )
    args = ap.parse_args()
    js = OUT / "P8_milestone.json"
    if args.from_json:
        res = json.loads(js.read_text(encoding="utf-8"))
    else:
        seeds = list(range(1, args.seeds + 1))
        res = json.loads(js.read_text(encoding="utf-8")) if args.only and js.exists() else {}
        steps = (
            args.only.split(",")
            if args.only
            else ["fit", "countdown", "imitation", "personas", "catalogue", "dark"]
        )
        res["seeds"] = args.seeds
        with tempfile.TemporaryDirectory() as td, ThreadPoolExecutor(args.jobs) as pool:
            tmp = Path(td)
            if "fit" in steps:
                res["fit_seeds"] = len(seeds)
                res["fit"] = fit_red(pool, tmp, seeds)
                print("fit", res["fit"]["intercept"], res["fit"]["share"])
                js.write_text(json.dumps(res, indent=1), encoding="utf-8")
            b0 = res["fit"]["intercept"]
            if "countdown" in steps:
                res["countdown"] = countdown(pool, tmp, seeds + [s + 100 for s in seeds], b0)
                print("countdown", res["countdown"])
                js.write_text(json.dumps(res, indent=1), encoding="utf-8")
            if "imitation" in steps:
                res["imitation"] = imitation(pool, tmp, seeds, b0)
                print("imitation done")
                js.write_text(json.dumps(res, indent=1), encoding="utf-8")
            if "personas" in steps:
                res["personas"] = persona_sweep(pool, tmp, seeds[:3], b0)
                print("personas done")
                js.write_text(json.dumps(res, indent=1), encoding="utf-8")
            if "catalogue" in steps:
                res["catalogue"] = catalogue(pool, tmp, seeds)
                print("catalogue done")
                js.write_text(json.dumps(res, indent=1), encoding="utf-8")
            if "dark" in steps:
                res["dark"] = dark_experiment(pool, tmp, seeds, b0)
                print("dark done")
                js.write_text(json.dumps(res, indent=1), encoding="utf-8")
        js.write_text(json.dumps(res, indent=1), encoding="utf-8")
    plot(res["dark"], OUT / "P8_dark.png")
    (OUT / "P8.md").write_text(report(res), encoding="utf-8")
    print(f"wrote {OUT / 'P8.md'}")


if __name__ == "__main__":
    main()
