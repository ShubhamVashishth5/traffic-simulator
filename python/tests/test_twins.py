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
            # `v0` is the drawn desired speed (the median-band bonus applies on top of it
            # while an agent is in the band), so the rescaled distribution moves every
            # car's v0 by exactly the same factor.
            f = 66.0 / marg.Register(PARAMS)["vehicles.free_flow_speed.hatchback"].mean
            cars = j.filter(pl.col("vehicle_type") == "hatchback")
            assert cars.height > 5
            assert ((cars["v0_b"] - f * cars["v0"]).abs() < 1e-9).all()
    # Since P5 the squeezing rule reads the cooperative twin's κ quantile (ADR-0007,
    # ADR-0010 §3), so the twin can differ from the observed arm although every agent
    # has the same draws. `violations.enabled` is not read before P8.
    k = exp.results("kpis")
    idx = exp.data().index()
    out = idx.select("arm", "rep", "output_hash")
    a = out.filter(pl.col("arm") == "observed").sort("rep")["output_hash"]
    c = out.filter(pl.col("arm") == "cooperative").sort("rep")["output_hash"]
    assert a.to_list() != c.to_list()
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
