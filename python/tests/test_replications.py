"""P3.4: fixed and sequential replications; the sequential rule stops within the
expected range on a synthetic metric and records why."""

from __future__ import annotations

import json
from pathlib import Path

import marg
import numpy as np
import pytest
from conftest import TINY, synthetic_register
from marg.design import Grid
from marg.experiment import Replications, stopping_decision

MU = 100.0


def noisy(e: dict) -> dict[str, float]:
    """A KPI with mean MU and SD t.sigma; the noise is a function of the seed."""
    z = np.random.default_rng(e["seed"]).standard_normal()
    return {"throughput_veh_h": MU + e["factors"]["t.sigma"] * z}


def stopping_quantiles(rule: Replications, sigma: float, sims: int = 2000) -> tuple[int, int]:
    """5th and 95th percentiles of the rule's stopping time on fresh normal data."""
    rng = np.random.default_rng(123)
    stops = []
    for _ in range(sims):
        x = list(MU + sigma * rng.standard_normal(rule.min))
        while stopping_decision(x, rule)["reason"] == "continue":
            x.append(MU + sigma * rng.standard_normal())
        stops.append(len(x))
    lo, hi = np.percentile(stops, [5, 95])
    return int(lo), int(hi)


def test_fixed_replications(tmp_path: Path) -> None:
    reg = synthetic_register(tmp_path, {"sigma": (0.1, 50.0)})
    exp = marg.Experiment(
        "fixed",
        marg.Scenario.from_yaml(TINY),
        design=Grid({"t.sigma": [5.0]}),
        replications=Replications(n=7),
        root=tmp_path,
        params_dir=reg,
    )
    exp.run(marg.backends.Function(noisy))
    assert exp.results("kpis").height == 7


def test_rule_validation() -> None:
    with pytest.raises(ValueError):
        Replications()
    with pytest.raises(ValueError):
        Replications(min=1, max=5, ci_halfwidth=0.1, metric="x")
    r = Replications(min=3, max=10, ci_halfwidth=0.5, metric="x", relative=False)
    assert stopping_decision([1.0, 1.0, 1.0], r)["reason"] == "ci_target_met"
    assert stopping_decision([1.0, 3.0], r)["reason"] == "continue"  # below min
    assert stopping_decision([0.0, 9.0] * 5, r)["reason"] == "max_reached"


def test_sequential_rule_stops_in_the_expected_range(tmp_path: Path) -> None:
    reg = synthetic_register(tmp_path, {"sigma": (0.1, 50.0)})
    rule = Replications(min=4, max=60, ci_halfwidth=0.02, metric="throughput_veh_h")
    sigmas = [2.0, 4.0, 6.0, 20.0]
    exp = marg.Experiment(
        "sequential",
        marg.Scenario.from_yaml(TINY),
        design=Grid({"t.sigma": sigmas}),
        replications=rule,
        root=tmp_path,
        params_dir=reg,
    )
    exp.run(marg.backends.Function(noisy))
    decisions = json.loads((exp.dir / "replications.json").read_text())
    k = exp.results("kpis")
    for p, sigma in enumerate(sigmas):
        d = decisions[f"tiny/{p}"]
        n = k.filter(k["point"] == p).height
        assert d["n"] == n
        if sigma == 20.0:  # needs ~(1.96 * 20 / 2)^2 = 384 replications: capped
            assert (d["reason"], n) == ("max_reached", rule.max)
            continue
        lo, hi = stopping_quantiles(rule, sigma)
        print(f"sigma={sigma}: stopped at n={n}, expected 90% range [{lo}, {hi}]")
        assert d["reason"] == "ci_target_met"
        assert lo <= n <= hi
        assert d["arms"]["base"]["halfwidth"] <= d["arms"]["base"]["target"]
    # The run list only grew by appending: ids are stable and match positions.
    runs = exp.run_list()
    assert [r["run_id"] for r in runs] == list(range(len(runs)))
