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
   e-rickshaw share of the stream, every violation rule on: KPIs over both.
5. **Milestone: the dark-signal experiment (G§6.6).** The crossing working (fixed plan,
   countdown) and dark, each with its cooperative twin (`marg.bundles.cooperative`: no
   violations, squeezing at the 25th-percentile κ), swept over the aggressive-persona (rushed
   driver) share: throughput, delay, conflicts, gridlock and the social efficiency deficit
   (excess of total delay over the twin).

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

import numpy as np
import polars as pl
import yaml
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
SOLE = ["e_rickshaw_driver", "auto_rickshaw_driver", "bus_driver", "truck_lcv_driver", "cyclist"]

FINDINGS = """
## Findings

Nothing below was tuned. The coefficients are assumed, and only the red-light intercept was
fitted, to its rate.

1. **P8.5, rate.** The red-light intercept fits 1 in 17, but noisily.
   - The share is not monotone in β0 near the fit (−7.67 gives 6.5%, −7.72 gives 5.2%). Runners
     change the traffic around them: who is at the front, imitation, and a locked run (1 of 5
     at the fitted value).
   - β0 = −7.68 gave 7.3% at its own evaluation, against the 5.9% target. Treat the fit as
     β0 ≈ −7.7 ± 0.1.
   - At 500 veh/h per arm the same fit gave −8.9; the intercept is not portable across demand.
2. **P8.5, timing regimes (a check): partly met.**
   - 36% of red runs fall in the first third of the red and 40% in the last; Mumbai has about a
     third and two-thirds. The after-red-starts regime emerges (time saved is largest and the
     cross streams are still in their all-red).
   - The before-green regime is weaker than observed. Its terms are the cross streams emptying
     at their amber and all-red, and waiting time.
   - At 500 veh/h the split was 40% / 25%: the end-of-red share depends on how many vehicles
     stand at the line.
3. **P8.5, countdown (a check): not met with equal intercepts.**
   - With a working countdown and the early-start intercept equal to the red-light one, cycles
     with a red crossing at the start of red fall from 9.4% to 7.0% (Sharma's direction, inside
     the CIs). At the end of red they stay at about 9%; early starts are rare (about 3 per run).
   - The sensitivity rows show the mechanism can produce the shift: 3 or 6 above the red-light
     intercept gives 25% and 41% at the end of red, with the start of red at 10% and 6.5%.
   - Sharma's 12% → 75% would need early start to be far more likely, per second, than running
     the red. That is a calibration question (stage 6), not a fit to make here.
4. **P8.6, imitation (done-when not met).** The early-start share rises from 7% at βs⁺ = 0 to
   9–12% at βs⁺ ≥ 4, with no sharp tipping. βs⁻ = 1 lowers it at 6 of 8 points, all inside
   the CIs.
   - Imitation does act: at βs⁺ = 10, 40 of 98 early starts had a moving violator in view, 4 of
     65 at βs⁺ = 0.
   - What is missing is a crowd to tip. An early-start episode has on average 0.22 standing
     compliers in view.
   - The field of view (ADR-0018 F1) is a sector from the vehicle's *front* centre, so
     neighbours standing abreast at the line, whose centres are slightly behind it, are outside
     it until one moves ahead.
   - For review: measure the sector from the driver's position (mid-length), or a larger front
     row (more two-wheelers, wider approaches).
5. **P8.8, persona shares.**
   - Violations per 1,000 vehicles rise with the delivery-rider share: 65–78 at 0, 81–97 at a
     third of two-wheelers. Red runs follow: 36–44 → 50–57. The CIs overlap (3 seeds per
     point).
   - Delay (about 26 s) and throughput do not move measurably with either share.
   - E-rickshaws (3–20% of the stream) change nothing at this junction. Their low speed matters
     on links, not at a signal that meters the flow.
6. **Milestone: the dark-signal experiment does not show a negative-sum game.**
   - **Dark.** Violations (almost all forced entries, 360–410 per 1,000 vehicles) *cut* total
     delay by 73–81% against the twin. Without them the dark junction serves only about 1,220
     of the 1,600 veh/h, at 160–200 s of delay, with 2–3 times the conflicts per vehicle.
   - **Fixed signal.** Red-light running (40–70 per 1,000) cuts delay by 6–12%.
   - The rushed-driver share raises violations but moves neither result.
   - **Two modelling choices drive this; both are for review (ADR-0019 part 5).**
     - Dark is built as *every movement gives way to every other*. With nobody forcing, that is
       not a cooperative regime but mutual deference: approaches go only when every one has
       stopped and all release together, which is slow and conflict-prone.
     - The twin (ADR-0007) is "the same rules without violations". Under a signal that is
       compliance; when dark it is that mutual deference.
     - A twin that coordinates instead (first come first served at the lines, or giving way
       to traffic from the right, as the Rules of the Road Regulations 1989 ask at an
       unregulated junction) would test the guide's claim. So would a dark regime in which
       drivers yield by a convention rather than to everyone (ADR-0019 part 5, options D1–D3).
7. **Robustness: the P7.2 limit is reached at 400 veh/h with personas on.**
   - Up to 2 of 5 fixed-signal runs and 3 of 5 dark runs locked. The fixed signal's cooperative
     twin locked too (0–1 of 5); the dark twin never did. Means above exclude locked runs,
     which are counted apart.
   - Two dark runs with forced entry held pairs in the collision guard every step: 40,318 and
     4,652 events, against 0–4 in the other dark runs. This is ADR-0016's open problem 3, still
     waiting on your decision.
   - At 500 veh/h, 1–2 of 5 fixed-signal runs locked in every arm.
   - A hang in the arbitration's ring breaker was found and fixed (ADR-0016, rings).
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


def with_e_rickshaws(comp: dict, share: float) -> dict:
    rest = {k: v for k, v in comp.items() if k != "e_rickshaw"}
    total = sum(rest.values())
    out = {k: v * (1.0 - share) / total for k, v in rest.items()}
    out["e_rickshaw"] = share
    return out


def personas(rushed: float = 0.15, delivery: float = 0.1) -> dict[str, float]:
    """All ten persona shares (a population is all or none, ADR-0018 part 6). Shares are
    renormalised within each vehicle class; classes with one persona take it whatever the
    share."""
    shares = {
        "average_commuter": max(0.85 - rushed - delivery, 0.0),
        "rushed_driver": rushed,
        "delivery_rider": delivery,
        "bike_taxi_rider": 0.05,
        "cautious_driver": 0.1,
    }
    shares.update({p: 1.0 for p in SOLE})
    return {f"personas.shares.{k}": v for k, v in shares.items()}


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


def dark_experiment(pool, tmp: Path, seeds: list[int], b0: float) -> list[dict]:
    coop = {"violations.enabled": 0.0, "personas.cooperative.squeeze_kappa_quantile": 0.25}
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
    lines += [
        "",
        "## 5. Milestone: the dark-signal experiment",
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
        "--only", default="", help="comma-separated steps: fit,countdown,imitation,personas,dark"
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
            else ["fit", "countdown", "imitation", "personas", "dark"]
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
