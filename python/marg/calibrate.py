"""Staged calibration with the freeze rule (plan P3.8, G§8.3).

Calibrate from the most physical to the most social, one stage at a time, and
freeze each stage before the next starts:

- ``Stage(k)`` refuses to start unless every earlier stage that has ``fit``
  parameters in the register is frozen, and refuses to re-open a frozen stage
  ("never re-open an earlier stage to fix a later mismatch", G§8.3);
- a stage may only write parameters with ``role: fit`` and ``stage: k``;
- results go to ``calibration/stage_<k>.yaml`` with values, uncertainty, data
  hashes and diagnostics; ``freeze()`` makes them part of every later run (the
  core applies frozen stages as the ``calibration`` layer).

Fitting is simulated method of moments: a statistic of simulated runs (with
standard errors) is matched to the observed statistic. Residuals are
standardised by the combined standard error, so the Jacobian at the optimum
gives the parameter covariance ``(JᵀJ)⁻¹``. Methods: ``least_squares``
(trust-region reflective, bounded) and ``cmaes`` (pycma). Simulations use fixed
seeds (common random numbers), so the objective is deterministic.

Distribution-valued parameters are fitted through their mean (ADR-0006).
"""

from __future__ import annotations

import datetime as _dt
import hashlib
import json
import math
import shutil
import tempfile
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import numpy as np
import yaml

from marg.design import BoundFactor, Factor, bind
from marg.experiment import register_digest
from marg.register import Register


class CalibrationError(RuntimeError):
    """A violation of the staged-calibration rules."""


@dataclass
class Moments:
    """Summary statistics with standard errors (one value per name)."""

    names: list[str]
    values: np.ndarray
    se: np.ndarray

    def __post_init__(self) -> None:
        self.values = np.asarray(self.values, dtype=float)
        self.se = np.asarray(self.se, dtype=float)
        if not (len(self.names) == len(self.values) == len(self.se)):
            raise ValueError("names, values and se must have the same length")

    def to_dict(self) -> dict[str, Any]:
        return {
            n: {"value": float(v), "se": float(s)}
            for n, v, s in zip(self.names, self.values, self.se, strict=True)
        }


@dataclass
class FitResult:
    """The outcome of :meth:`Stage.fit`."""

    params: list[BoundFactor]
    estimate: np.ndarray
    se: np.ndarray
    ci_level: float
    method: str
    success: bool
    message: str
    cost: float
    evaluations: int
    residuals: np.ndarray
    observed: Moments
    simulated: Moments
    jacobian_condition: float
    history: list[dict[str, Any]] = field(default_factory=list, repr=False)

    @property
    def ci(self) -> np.ndarray:
        from scipy import stats

        z = stats.norm.ppf(0.5 + self.ci_level / 2)
        return np.column_stack([self.estimate - z * self.se, self.estimate + z * self.se])

    def values(self) -> dict[str, Any]:
        """The fitted overrides, in register units (what runs use)."""
        return {b.path: b.value(x) for b, x in zip(self.params, self.estimate, strict=True)}

    def table(self) -> list[dict[str, Any]]:
        ci = self.ci
        return [
            {
                "path": b.path,
                "estimate": float(x),
                "se": float(s),
                "ci_lo": float(c[0]),
                "ci_hi": float(c[1]),
                "unit": b.unit,
                "transform": b.transform,
            }
            for b, x, s, c in zip(self.params, self.estimate, self.se, ci, strict=True)
        ]


def file_digest(path: str | Path) -> str:
    """blake2b-128 of a file's bytes."""
    h = hashlib.blake2b(digest_size=16)
    with Path(path).open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def stage_path(root: str | Path, k: int) -> Path:
    return Path(root) / f"stage_{k}.yaml"


def read_stage(root: str | Path, k: int) -> dict[str, Any] | None:
    p = stage_path(root, k)
    return yaml.safe_load(p.read_text()) if p.exists() else None


class Stage:
    """Calibration stage ``k`` (G§8.3), stored under ``root``."""

    def __init__(
        self,
        k: int,
        root: str | Path = "calibration",
        params_dir: str | Path | None = None,
    ) -> None:
        self.k = int(k)
        self.root = Path(root)
        self.base_register = Register(params_dir)
        self.params_dir = self.base_register.params_dir
        fit_stages = sorted(
            {
                e.stage
                for e in self.base_register.values()
                if e.role == "fit" and e.stage is not None
            }
        )
        if self.k not in fit_stages:
            raise CalibrationError(f"stage {k} has no `fit` parameters (stages: {fit_stages})")
        for j in (s for s in fit_stages if s < self.k):
            prev = read_stage(self.root, j)
            if prev is None or prev.get("status") != "frozen":
                raise CalibrationError(
                    f"stage {self.k} cannot start: stage {j} is not frozen "
                    f"({stage_path(self.root, j)} {'is a draft' if prev else 'does not exist'})"
                )
        own = read_stage(self.root, self.k)
        if own is not None and own.get("status") == "frozen":
            raise CalibrationError(
                f"stage {self.k} is frozen; frozen stages are never re-opened (G§8.3). "
                "Fix a later mismatch in a later stage, or start a new calibration root."
            )
        # Runs during this stage see every frozen earlier stage.
        self.register = Register(self.params_dir, self.root if self.root.is_dir() else None)

    @property
    def parameters(self) -> list[str]:
        """The parameters this stage may fit."""
        return self.base_register.select(role="fit", stage=self.k)

    def check_writable(self, paths: Sequence[str]) -> None:
        """Raise unless every path is a ``fit`` parameter of this stage."""
        for p in paths:
            if p not in self.base_register:
                raise CalibrationError(f"`{p}` is not in the register")
            e = self.base_register[p]
            if e.role != "fit" or e.stage != self.k:
                what = (
                    f"a stage-{e.stage} `fit` parameter" if e.role == "fit" else f"role `{e.role}`"
                )
                raise CalibrationError(
                    f"stage {self.k} may only write its own `fit` parameters; `{p}` is {what}"
                )

    def bind(self, params: Sequence[str | Factor]) -> list[BoundFactor]:
        factors = [p if isinstance(p, Factor) else Factor(p) for p in params]
        self.check_writable([f.path for f in factors])
        return [bind(f, self.base_register) for f in factors]

    # ----- fitting -----

    def fit(
        self,
        params: Sequence[str | Factor],
        simulate: Callable[[dict[str, Any]], Moments],
        observed: Moments,
        x0: Sequence[float] | None = None,
        method: str = "least_squares",
        ci_level: float = 0.95,
        sim_se: str = "start",
        max_evaluations: int = 200,
        seed: int = 1,
    ) -> FitResult:
        """Fit ``params`` so that ``simulate(overrides)`` matches ``observed``.

        ``simulate`` receives register overrides and returns :class:`Moments`
        in the order of ``observed.names``. Residuals are divided by
        ``sqrt(se_obs² + se_sim²)``, with ``se_sim`` taken at the start point
        (``sim_se="start"``) so the weights do not move during the fit.
        """
        bound = self.bind(params)
        lo = np.array([b.lo for b in bound])
        hi = np.array([b.hi for b in bound])
        x0a = np.array(
            x0
            if x0 is not None
            else [b.base_mean if b.base_mean is not None else (b.lo + b.hi) / 2 for b in bound],
            dtype=float,
        )
        x0a = np.clip(x0a, lo, hi)
        cache: dict[tuple[float, ...], Moments] = {}
        history: list[dict[str, Any]] = []

        def sim(x: np.ndarray) -> Moments:
            key = tuple(float(v) for v in x)
            if key not in cache:
                ov = {b.path: b.value(v) for b, v in zip(bound, x, strict=True)}
                m = simulate(ov)
                if m.names != observed.names:
                    raise ValueError("simulate() must return the observed statistics, in order")
                cache[key] = m
            return cache[key]

        start = sim(x0a)
        se_sim = start.se if sim_se == "start" else np.zeros_like(start.se)
        scale = np.sqrt(observed.se**2 + se_sim**2)
        if np.any(scale <= 0):
            raise ValueError("every statistic needs a positive standard error")

        def residuals(x: np.ndarray) -> np.ndarray:
            r = (sim(x).values - observed.values) / scale
            history.append({"x": [float(v) for v in x], "cost": float(0.5 * r @ r)})
            return r

        if method == "least_squares":
            from scipy.optimize import least_squares

            span = hi - lo
            res = least_squares(
                residuals,
                x0a,
                bounds=(lo, hi),
                x_scale=span,
                diff_step=1e-3,
                max_nfev=max_evaluations,
                method="trf",
            )
            x, ok, msg = res.x, bool(res.success), str(res.message)
        elif method == "cmaes":
            import cma

            span = hi - lo
            z0 = (x0a - lo) / span
            es = cma.CMAEvolutionStrategy(
                list(z0),
                0.25,
                {"bounds": [0, 1], "seed": seed, "maxfevals": max_evaluations, "verbose": -9},
            )
            es.optimize(lambda z: float(np.sum(residuals(lo + np.asarray(z) * span) ** 2)))
            x = lo + np.asarray(es.result.xbest) * span
            ok, msg = True, str(es.stop())
        else:
            raise ValueError("method must be 'least_squares' or 'cmaes'")

        r = residuals(x)
        jac = _jacobian(residuals, x, lo, hi)
        jtj = jac.T @ jac
        cond = float(np.linalg.cond(jtj)) if jtj.size else math.nan
        try:
            cov = np.linalg.inv(jtj)
            se = np.sqrt(np.clip(np.diag(cov), 0, None))
        except np.linalg.LinAlgError:
            se = np.full(len(bound), np.inf)
        return FitResult(
            params=bound,
            estimate=x,
            se=se,
            ci_level=ci_level,
            method=method,
            success=ok,
            message=msg,
            cost=float(0.5 * r @ r),
            evaluations=len(cache),
            residuals=r,
            observed=observed,
            simulated=sim(x),
            jacobian_condition=cond,
            history=history,
        )

    # ----- stage file -----

    def write(
        self,
        result: FitResult,
        data: Sequence[str | Path] = (),
        recovery: Mapping[str, Any] | None = None,
        notes: str = "",
    ) -> Path:
        """Write ``calibration/stage_<k>.yaml`` as a draft."""
        self.check_writable([b.path for b in result.params])
        self.root.mkdir(parents=True, exist_ok=True)
        params = {}
        for row, b in zip(result.table(), result.params, strict=True):
            v = b.value(row["estimate"])
            params[b.path] = {
                "value": v,
                "estimate": row["estimate"],
                "se": row["se"],
                "ci": [row["ci_lo"], row["ci_hi"]],
                "ci_level": result.ci_level,
                "unit": b.unit,
                "transform": b.transform,
                "bounds": [b.lo, b.hi],
            }
        doc = {
            "stage": self.k,
            "status": "draft",
            "created": _dt.datetime.now(_dt.UTC).isoformat(timespec="seconds"),
            "method": result.method,
            "register": register_digest(self.params_dir),
            "params": params,
            "data": [{"path": str(p), "blake2b": file_digest(p)} for p in data],
            "diagnostics": {
                "success": result.success,
                "message": result.message,
                "cost": result.cost,
                "evaluations": result.evaluations,
                "jacobian_condition": result.jacobian_condition,
                "residuals": dict(
                    zip(result.observed.names, map(float, result.residuals), strict=True)
                ),
                "observed": result.observed.to_dict(),
                "simulated": result.simulated.to_dict(),
            },
            "notes": notes,
        }
        if recovery is not None:
            doc["recovery"] = dict(recovery)
        p = stage_path(self.root, self.k)
        p.write_text(yaml.safe_dump(_plain(doc), sort_keys=False, allow_unicode=True))
        return p

    def freeze(self) -> Path:
        """Freeze this stage: its values apply to every later run and stage."""
        p = stage_path(self.root, self.k)
        doc = read_stage(self.root, self.k)
        if doc is None:
            raise CalibrationError(f"nothing to freeze: {p} does not exist")
        if doc.get("status") == "frozen":
            raise CalibrationError(f"stage {self.k} is already frozen")
        self.check_writable(list(doc.get("params", {})))
        doc["status"] = "frozen"
        doc["frozen"] = _dt.datetime.now(_dt.UTC).isoformat(timespec="seconds")
        p.write_text(yaml.safe_dump(doc, sort_keys=False, allow_unicode=True))
        Register(self.params_dir, self.root)  # the core validates the frozen set
        return p


def stage(k: int, root: str | Path = "calibration", params_dir: str | Path | None = None) -> Stage:
    """Start (or resume) calibration stage ``k``. See :class:`Stage`."""
    return Stage(k, root, params_dir)


def _jacobian(f: Callable[[np.ndarray], np.ndarray], x: np.ndarray, lo, hi) -> np.ndarray:
    """Central differences (one-sided at a bound), steps 1e-3 of each range."""
    f0 = f(x)
    jac = np.zeros((len(f0), len(x)))
    for i in range(len(x)):
        h = 1e-3 * (hi[i] - lo[i])
        up, dn = x.copy(), x.copy()
        up[i] = min(x[i] + h, hi[i])
        dn[i] = max(x[i] - h, lo[i])
        jac[:, i] = (f(up) - f(dn)) / (up[i] - dn[i])
    return jac


def _plain(v: Any) -> Any:
    if isinstance(v, dict):
        return {k: _plain(x) for k, x in v.items()}
    if isinstance(v, list | tuple):
        return [_plain(x) for x in v]
    if isinstance(v, np.generic):
        return v.item()
    if isinstance(v, float) and not math.isfinite(v):
        return None
    return v


# ----- simulation helpers -----


class Simulator:
    """Runs a scenario at fixed seeds with given overrides and reduces the
    outputs to :class:`Moments` (for :meth:`Stage.fit`).

    ``statistic(run_dirs) -> Moments`` reads the run folders (one per seed).
    Runs go to a temporary folder that is removed after each evaluation.
    """

    def __init__(
        self,
        scenario: Any,
        seeds: Sequence[int],
        statistic: Callable[[list[Path]], Moments],
        params_dir: str | Path | None = None,
        calibration_dir: str | Path | None = None,
    ) -> None:
        from marg.scenario import Scenario

        sc = scenario if isinstance(scenario, Scenario) else Scenario.load(scenario)
        self.yaml = sc.with_changes({"scenario.outputs.trajectories": None}).to_yaml()
        self.seeds = list(seeds)
        self.statistic = statistic
        self.params_dir = str(params_dir) if params_dir else None
        self.calibration_dir = str(calibration_dir) if calibration_dir else None
        self.evaluations = 0

    def __call__(self, overrides: Mapping[str, Any]) -> Moments:
        from marg import _core

        self.evaluations += 1
        cdir = self.calibration_dir
        if cdir is not None and not Path(cdir).is_dir():
            cdir = None  # no stage written yet
        tmp = Path(tempfile.mkdtemp(prefix="marg-cal-"))
        try:
            dirs = []
            for s in self.seeds:
                out = tmp / f"seed={s}"
                _core.run_yaml(
                    self.yaml,
                    s,
                    str(out),
                    1,
                    self.params_dir,
                    json.dumps(dict(overrides)),
                    cdir,
                )
                dirs.append(out)
            return self.statistic(dirs)
        finally:
            shutil.rmtree(tmp, ignore_errors=True)


def screenline_speed_moments(
    detector_id: str, vehicle_types: Sequence[str]
) -> Callable[[list[Path]], Moments]:
    """Stage-1 statistic: pooled mean spot speed (km/h) per vehicle type at a
    screenline, over the measured window, from ``detectors.parquet``.

    The standard error is cluster-robust over detector intervals (each
    interval × run is a cluster): ``var = k/(k−1) Σ c_j² (m_j − m)² / C²``.
    """
    import polars as pl

    def stat(dirs: list[Path]) -> Moments:
        d = pl.concat(
            [
                pl.read_parquet(p / "detectors.parquet").with_columns(pl.lit(i).alias("run"))
                for i, p in enumerate(dirs)
            ]
        ).filter(
            (pl.col("detector_id") == detector_id) & (~pl.col("in_warmup")) & (pl.col("count") > 0)
        )
        values, ses = [], []
        for t in vehicle_types:
            g = d.filter(pl.col("vehicle_type") == t)
            c = g["count"].cast(pl.Float64).to_numpy()
            m = g["time_mean_speed"].to_numpy() * 3.6
            if c.sum() == 0 or len(c) < 2:
                raise ValueError(f"no crossings of {t} at {detector_id}")
            mean = float((c * m).sum() / c.sum())
            k = len(c)
            var = k / (k - 1) * float(((c * (m - mean)) ** 2).sum()) / c.sum() ** 2
            values.append(mean)
            ses.append(math.sqrt(var))
        return Moments([f"{t}_speed_kmh" for t in vehicle_types], values, ses)

    return stat


def recovery_test(
    stage: Stage,
    truth: Mapping[str, float],
    simulator_for_seeds: Callable[[Sequence[int]], Simulator],
    observe_seeds: Sequence[int],
    fit_seeds: Sequence[int],
    method: str = "least_squares",
    x0: Sequence[float] | None = None,
    bounds: Mapping[str, tuple[float, float]] | None = None,
    ci_level: float = 0.95,
) -> tuple[FitResult, dict[str, Any]]:
    """Synthetic recovery (Carrella 2021): simulate "observed" data with known
    ``truth`` at ``observe_seeds``, fit at different ``fit_seeds``, and report
    whether each true value falls inside its CI."""
    params = [Factor(p, *bounds[p]) if bounds and p in bounds else Factor(p) for p in truth]
    bound = stage.bind(params)
    truth_ov = {b.path: b.value(truth[b.path]) for b in bound}
    observed = simulator_for_seeds(observe_seeds)(truth_ov)
    res = stage.fit(
        params, simulator_for_seeds(fit_seeds), observed, x0=x0, method=method, ci_level=ci_level
    )
    rows = []
    for row in res.table():
        t = truth[row["path"]]
        rows.append(dict(row, truth=t, covered=bool(row["ci_lo"] <= t <= row["ci_hi"])))
    report = {
        "observe_seeds": list(observe_seeds),
        "fit_seeds": list(fit_seeds),
        "method": method,
        "all_covered": all(r["covered"] for r in rows),
        "params": rows,
    }
    return res, report
