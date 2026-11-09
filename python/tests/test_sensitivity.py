"""P3.7: the Ishigami function reproduces its known Sobol indices through the
full pipeline (design → run list → fake simulator backend → collect → results
→ analysis), and Morris and the surrogate behave as expected on it."""

from __future__ import annotations

import math
from pathlib import Path

import marg
import numpy as np
import pytest
from conftest import TINY, synthetic_register
from marg.design import LatinHypercube, Morris, Saltelli

A, B = 7.0, 0.1
PI = math.pi
FACTORS = ["t.x1", "t.x2", "t.x3"]

# Analytic indices (Ishigami & Homma 1990; Sobol' & Levitan 1999), a = 7, b = 0.1.
_V1 = 0.5 * (1 + B * PI**4 / 5) ** 2
_V2 = A**2 / 8
_V13 = B**2 * PI**8 * (1 / 18 - 1 / 50)
_V = _V1 + _V2 + _V13
S1 = [_V1 / _V, _V2 / _V, 0.0]
ST = [(_V1 + _V13) / _V, _V2 / _V, _V13 / _V]


def ishigami(e: dict) -> dict[str, float]:
    x1, x2, x3 = (e["factors"][f] for f in FACTORS)
    y = math.sin(x1) + A * math.sin(x2) ** 2 + B * x3**4 * math.sin(x1)
    return {"y": y}


def experiment(tmp_path: Path, name: str, design) -> marg.Experiment:
    reg = synthetic_register(tmp_path, {"x1": (-PI, PI), "x2": (-PI, PI), "x3": (-PI, PI)})
    exp = marg.Experiment(
        name, marg.Scenario.from_yaml(TINY), design=design, root=tmp_path, params_dir=reg
    )
    return exp.run(marg.backends.Function(ishigami, workers=4))


def test_analytic_values() -> None:
    assert S1 == pytest.approx([0.3139, 0.4424, 0.0], abs=1e-4)
    assert ST == pytest.approx([0.5576, 0.4424, 0.2437], abs=1e-4)


def test_ishigami_sobol_through_the_pipeline(tmp_path: Path) -> None:
    exp = experiment(tmp_path, "ishigami-sobol", Saltelli(FACTORS, n=1024, seed=3))
    assert exp.collect()["ok"] == 1024 * 5
    s = marg.sensitivity.sobol(exp, ["y"]).sort("factor")
    print(s)
    assert s["S1"].to_list() == pytest.approx(S1, abs=0.05)
    assert s["ST"].to_list() == pytest.approx(ST, abs=0.05)
    # The bootstrap CIs cover the analytic values.
    for col, truth in (("S1", S1), ("ST", ST)):
        for est, conf, t in zip(s[col], s[f"{col}_conf"], truth, strict=True):
            assert abs(est - t) <= max(conf, 0.01) * 1.5
    # x3 acts only through its interaction with x1.
    x3 = s.filter(s["factor"] == "t.x3").row(0, named=True)
    assert x3["interaction"] > 0.15
    from marg import plots, style

    style.save(plots.sobol_plot(s, "y"), str(tmp_path / "sobol"))


def test_ishigami_morris(tmp_path: Path) -> None:
    exp = experiment(
        tmp_path, "ishigami-morris", Morris(FACTORS, trajectories=40, levels=6, seed=5)
    )
    m = marg.sensitivity.morris(exp, ["y"]).sort("factor")
    print(m)
    mu, mu_star, sigma = (np.array(m[c].to_list()) for c in ("mu", "mu_star", "sigma"))
    assert np.all(mu_star > 0.5)  # every factor matters
    assert mu_star[0] == mu_star.max()  # x1 dominates the screen
    assert abs(mu[1]) < 0.1 * mu_star[1]  # x2: sin² is symmetric, effects cancel in μ, not μ*
    assert sigma[2] > mu_star[2]  # x3 acts only through x1: non-linear / interacting
    assert sigma[0] > 0.5 * mu_star[0]  # x1 interacts with x3
    from marg import plots, style

    style.save(plots.morris_plot(m, "y"), str(tmp_path / "morris"))


@pytest.mark.parametrize("model", ["gp", "rf"])
def test_ishigami_surrogate(tmp_path: Path, model: str) -> None:
    exp = experiment(tmp_path, f"ishigami-lhs-{model}", LatinHypercube(FACTORS, n=400, seed=7))
    r = marg.sensitivity.surrogate(exp, ["y"], model=model, n_sobol=2048)
    s = r.indices.sort("factor")
    print(model, r.r2, s)
    if model == "gp":
        assert r.r2["y"] > 0.95
        assert s["S1"].to_list() == pytest.approx(S1, abs=0.06)
        assert s["ST"].to_list() == pytest.approx(ST, abs=0.06)
    else:  # a forest on 400 points is rougher, but ranks the factors and sees x3's interaction
        assert r.r2["y"] > 0.75
        assert s["ST"].to_list() == pytest.approx(ST, abs=0.12)


def test_analysis_checks_the_design(tmp_path: Path) -> None:
    exp = experiment(tmp_path, "wrong", LatinHypercube(FACTORS, n=20))
    with pytest.raises(ValueError, match="Morris design"):
        marg.sensitivity.morris(exp, ["y"])
    with pytest.raises(ValueError, match="Saltelli design"):
        marg.sensitivity.sobol(exp, ["y"])
