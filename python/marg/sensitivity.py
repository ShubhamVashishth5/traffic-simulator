"""Sensitivity analysis over experiment results (plan P3.7, G§8.4).

Two passes, as the guide recommends: screen every calibrated and swept
parameter with Morris elementary effects, then run a Sobol decomposition on the
survivors. When runs are expensive, fit a surrogate (GP or random forest) on a
few hundred runs and compute the indices on it (Ciuffo et al. 2013).

Every function takes an experiment (``Experiment``, ``ExperimentData`` or a
folder) and a list of ``kpis`` columns. Replications at a point are averaged
before the analysis; with several arms, ``arm`` picks one (default: the first).
"""

from __future__ import annotations

import warnings
from collections.abc import Sequence
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import numpy as np
import polars as pl

from marg.results import ExperimentData


def _data(exp: Any) -> ExperimentData:
    if isinstance(exp, ExperimentData):
        return exp
    if isinstance(exp, str | Path):
        return ExperimentData(exp)
    return exp.data()


def design_outputs(
    exp: Any, outputs: Sequence[str], arm: str | None = None, scenario: str | None = None
) -> tuple[np.ndarray, dict[str, np.ndarray], dict[str, Any]]:
    """The design matrix ``X`` (points × factors, in point order), each
    output's replication mean per point, and the SALib problem."""
    data = _data(exp)
    arms = list(data.definition.get("arms", {"base": {}}))
    arm = arm or arms[0]
    filters: dict[str, Any] = {"arm": arm}
    if scenario is not None:
        filters["scenario"] = scenario
    k = data.results("kpis", filters=filters, columns=list(outputs))
    if k["scenario"].n_unique() > 1:
        raise ValueError("several scenarios in this experiment: pass scenario=…")
    design = data.design().sort("point")
    problem = data.problem()
    means = k.group_by("point").agg([pl.col(o).mean() for o in outputs])
    joined = design.select("point").join(means, on="point", how="left").sort("point")
    missing = joined.filter(pl.any_horizontal(pl.col(o).is_null() for o in outputs))
    if missing.height:
        raise ValueError(f"{missing.height} design points have no ok runs; collect first")
    x = design.select(problem["names"]).to_numpy()
    ys = {o: joined[o].to_numpy().astype(float) for o in outputs}
    for o, y in ys.items():
        if not np.all(np.isfinite(y)):
            raise ValueError(f"output `{o}` has non-finite values at some points")
    return x, ys, problem


def morris(
    exp: Any,
    outputs: Sequence[str],
    arm: str | None = None,
    num_resamples: int = 1000,
    conf_level: float = 0.95,
    seed: int = 1,
) -> pl.DataFrame:
    """Morris indices per output and factor: ``mu``, ``mu_star`` (with CI
    half-width ``mu_star_conf``), ``sigma``, and ``mu_star_rel`` (μ* over the
    largest μ* for that output). Needs a :class:`marg.design.Morris` design."""
    from SALib.analyze import morris as sa

    data = _data(exp)
    d = data.definition["design"]
    if d["type"] != "morris":
        raise ValueError(f"Morris analysis needs a Morris design, not `{d['type']}`")
    x, ys, problem = design_outputs(data, outputs, arm)
    rows = []
    for o, y in ys.items():
        r = sa.analyze(
            problem,
            x,
            y,
            num_resamples=num_resamples,
            conf_level=conf_level,
            num_levels=d["levels"],
            seed=seed,
        )
        top = float(np.max(r["mu_star"])) or 1.0
        for i, name in enumerate(problem["names"]):
            rows.append(
                {
                    "output": o,
                    "factor": name,
                    "mu": float(r["mu"][i]),
                    "mu_star": float(r["mu_star"][i]),
                    "mu_star_conf": float(r["mu_star_conf"][i]),
                    "sigma": float(r["sigma"][i]),
                    "mu_star_rel": float(r["mu_star"][i]) / top,
                }
            )
    return pl.DataFrame(rows).sort(["output", "mu_star"], descending=[False, True])


def sobol(
    exp: Any,
    outputs: Sequence[str],
    arm: str | None = None,
    num_resamples: int = 1000,
    conf_level: float = 0.95,
    seed: int = 1,
) -> pl.DataFrame:
    """First-order (``S1``) and total (``ST``) Sobol indices with bootstrap CI
    half-widths, per output and factor. Needs a :class:`marg.design.Saltelli`
    design. ``interaction`` = ST − S1."""
    from SALib.analyze import sobol as sa

    data = _data(exp)
    d = data.definition["design"]
    if d["type"] != "saltelli":
        raise ValueError(f"Sobol analysis needs a Saltelli design, not `{d['type']}`")
    _, ys, problem = design_outputs(data, outputs, arm)
    rows = []
    for o, y in ys.items():
        r = sa.analyze(
            problem,
            y,
            calc_second_order=d["second_order"],
            num_resamples=num_resamples,
            conf_level=conf_level,
            seed=seed,
        )
        rows += _sobol_rows(o, problem, r)
    return pl.DataFrame(rows)


def _sobol_rows(output: str, problem: dict[str, Any], r: Any) -> list[dict[str, Any]]:
    return [
        {
            "output": output,
            "factor": name,
            "S1": float(r["S1"][i]),
            "S1_conf": float(r["S1_conf"][i]),
            "ST": float(r["ST"][i]),
            "ST_conf": float(r["ST_conf"][i]),
            "interaction": float(r["ST"][i] - r["S1"][i]),
        }
        for i, name in enumerate(problem["names"])
    ]


@dataclass
class SurrogateResult:
    """Sobol indices computed on a fitted surrogate, with its validation."""

    indices: pl.DataFrame
    #: Cross-validated R² per output (out-of-fold predictions).
    r2: dict[str, float]
    model: str
    models: dict[str, Any] = field(repr=False, default_factory=dict)

    def predict(self, output: str, x: np.ndarray) -> np.ndarray:
        return self.models[output].predict(np.asarray(x, dtype=float))


def _make_model(kind: str, k: int, seed: int) -> Any:
    from sklearn.pipeline import make_pipeline
    from sklearn.preprocessing import MinMaxScaler

    if kind == "gp":
        from sklearn.gaussian_process import GaussianProcessRegressor
        from sklearn.gaussian_process.kernels import RBF, ConstantKernel, WhiteKernel

        kernel = ConstantKernel(1.0, (1e-3, 1e5)) * RBF(
            length_scale=np.full(k, 0.5), length_scale_bounds=(1e-2, 1e2)
        ) + WhiteKernel(1e-3, (1e-8, 1.0))
        reg = GaussianProcessRegressor(
            kernel=kernel, normalize_y=True, n_restarts_optimizer=2, random_state=seed
        )
    elif kind == "rf":
        from sklearn.ensemble import RandomForestRegressor

        reg = RandomForestRegressor(n_estimators=400, min_samples_leaf=2, random_state=seed)
    else:
        raise ValueError("model must be 'gp' or 'rf'")
    return make_pipeline(MinMaxScaler(), reg)


def surrogate(
    exp: Any,
    outputs: Sequence[str],
    model: str = "gp",
    arm: str | None = None,
    n_sobol: int = 4096,
    folds: int = 5,
    num_resamples: int = 500,
    seed: int = 1,
) -> SurrogateResult:
    """Fit a GP or random-forest surrogate on any design (LHS is a good choice),
    check it by ``folds``-fold cross-validation (R²), and compute Sobol indices
    on a Saltelli sample of ``n_sobol`` base points evaluated on the surrogate."""
    from SALib.analyze import sobol as sa
    from SALib.sample import sobol as ss
    from sklearn.exceptions import ConvergenceWarning
    from sklearn.metrics import r2_score
    from sklearn.model_selection import KFold, cross_val_predict

    x, ys, problem = design_outputs(exp, outputs, arm)
    k = x.shape[1]
    sample = ss.sample(problem, n_sobol, calc_second_order=False, seed=seed)
    rows, r2, models = [], {}, {}
    for o, y in ys.items():
        m = _make_model(model, k, seed)
        cv = KFold(n_splits=folds, shuffle=True, random_state=seed)
        with warnings.catch_warnings():
            # Hyper-parameter optimiser warnings: the cross-validated R² is the check.
            warnings.simplefilter("ignore", ConvergenceWarning)
            pred = cross_val_predict(m, x, y, cv=cv)
            m.fit(x, y)
        r2[o] = float(r2_score(y, pred))
        models[o] = m
        r = sa.analyze(
            problem,
            m.predict(sample),
            calc_second_order=False,
            num_resamples=num_resamples,
            seed=seed,
        )
        rows += _sobol_rows(o, problem, r)
    return SurrogateResult(pl.DataFrame(rows), r2, model, models)
