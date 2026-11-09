"""P3.3: twin arms share seeds, so the same agent gets the same draws, and
paired differences have far lower variance than independent ones."""

from __future__ import annotations

from pathlib import Path

import marg
import polars as pl
from conftest import PARAMS

DRAWS = ["vehicle_type", "t_arrival", "length", "width", "time_gap", "standstill_gap"]
DRAWS += ["max_accel", "comfortable_decel"]


def test_cooperative_bundle_switches() -> None:
    b = marg.bundles.cooperative()
    assert b == {"violations.enabled": 0.0, "personas.cooperative.squeeze_kappa_quantile": 0.25}
    reg = marg.Register(PARAMS)
    for path in b:
        assert reg[path].role == "scenario"


def test_same_agent_same_draws_in_every_arm(tiny, tmp_path: Path) -> None:
    arms = {
        "observed": {},
        "cooperative": marg.bundles.cooperative(),
        "fast_cars": {"vehicles.free_flow_speed.hatchback": marg.design.Mean(66.0)},
    }
    exp = marg.Experiment(
        "twins",
        tiny,
        arms=arms,
        replications=marg.Replications(n=2),
        root=tmp_path,
        params_dir=PARAMS,
    )
    exp.run(marg.backends.Local(workers=2))
    agents = exp.results("agents")
    obs = agents.filter(pl.col("arm") == "observed")
    assert obs.height > 20
    for arm in ("cooperative", "fast_cars"):
        other = agents.filter(pl.col("arm") == arm)
        j = obs.join(other, on=["rep", "agent_id"], suffix="_b")
        assert j.height == obs.height == other.height  # the same agents arrive
        for c in DRAWS:
            assert (j[c] == j[f"{c}_b"]).all(), f"{arm}: {c} differs"
        not_cars = j.filter(pl.col("vehicle_type") != "hatchback")
        assert (not_cars["v0"] == not_cars["v0_b"]).all()
        if arm == "fast_cars":
            # The rescaled distribution moves every car's drawn v0 by the same factor;
            # cars entering in the median band also carry their (unchanged) additive bonus.
            f = 66.0 / marg.Register(PARAMS)["vehicles.free_flow_speed.hatchback"].mean
            cars = j.filter(pl.col("vehicle_type") == "hatchback")
            # v0 = f·drawn + bonus in the arm, drawn + bonus in the baseline, so the
            # implied bonus is (f·v0 − v0_b) / (f − 1): 0 or within the register's range.
            bonus = ((f * cars["v0"] - cars["v0_b"]) / (f - 1)).to_list()
            lo, hi = (x / 3.6 for x in (4.35, 5.78))  # vehicles.median_band.speed_bonus, km/h
            assert all(abs(b) < 1e-6 or lo - 1e-6 <= b <= hi + 1e-6 for b in bonus), bonus
            assert any(abs(b) < 1e-6 for b in bonus)
    # The cooperative switches are not read before P5/P8, so the twin is identical.
    k = exp.results("kpis")
    idx = exp.data().index()
    out = idx.select("arm", "rep", "output_hash")
    a = out.filter(pl.col("arm") == "observed").sort("rep")["output_hash"]
    c = out.filter(pl.col("arm") == "cooperative").sort("rep")["output_hash"]
    assert a.to_list() == c.to_list()
    assert k.height == 6


def test_common_random_numbers_reduce_paired_variance(tiny, tmp_path: Path) -> None:
    arms = {"base": {}, "fast_cars": {"vehicles.free_flow_speed.hatchback": marg.design.Mean(66.0)}}
    var = {}
    for crn in ("all", "none"):
        exp = marg.Experiment(
            f"vr-{crn}",
            tiny,
            arms=arms,
            replications=marg.Replications(n=8),
            crn=crn,
            root=tmp_path,
            params_dir=PARAMS,
        )
        exp.run(marg.backends.Local(workers=4))
        d = marg.stats.paired_differences(exp.results("kpis"), "delay_mean_s", "fast_cars", "base")
        var[crn] = d["diff"].var()
        s = marg.stats.paired_summary(exp.results("kpis"), "delay_mean_s", "fast_cars", "base")
        if crn == "all":
            assert s["variance_ratio"][0] < 0.25
    ratio = var["all"] / var["none"]
    print(f"paired-difference variance, CRN / independent = {ratio:.3f}")
    assert ratio < 0.25
