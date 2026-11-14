"""Gap acceptance: estimating critical gaps from accepted and rejected gaps (P7.4, G§2.4).

Everything here works on a **gap table**, so that simulated runs and gap-acceptance surveys
go through the same code. One row per offered gap that a driver decided on:

    agent_id, class, gap_s, accepted, against_class

:func:`gap_table` builds it from a run folder's ``gap_accepted`` and ``gap_rejected`` events
(the core logs every gap a waiting driver turns down, at the moment it is first offered, and
the one it takes). For survey data, give one row per lag or gap with the same columns.

Two estimators of the critical gap:

- :func:`raff` (Raff & Hart 1950): the gap at which the number of accepted gaps shorter than it
  equals the number of rejected gaps longer than it. Abhigna et al. (2020, Warangal) report it
  beside their clearing-behaviour values, pooling lags and gaps.
- :func:`mle` (Troutbeck 1992; Brilon, Koenig & Troutbeck 1999): each driver's critical gap lies
  between the largest gap it turned down and the gap it took; the likelihood of a lognormal
  population of critical gaps is the product of ``F(accepted) - F(largest rejected)`` over
  drivers. It is consistent when each driver decides by a fixed threshold, as the core's drivers
  do (ADR-0016 part 3).

**The clearing-behaviour method is not implemented.** Abhigna et al.'s headline values, which
the register takes for ``interaction.gap_acceptance.critical_gap.*``, come from it: they read
the critical gap off where the cumulative curve of clearing times (the time each vehicle takes
to cross the intersection) meets the cumulative curve of accepted lags and gaps. That depends on
how long vehicles take to clear, not only on which gaps drivers take, and the paper itself calls
the result "the safest critical gap", higher than the gap-acceptance (Raff) values from the same
data (2.3-4.8 s against 4.0-11.5 s). See reports/phases/P7.md.
"""

from __future__ import annotations

import json
import math
from collections.abc import Sequence
from dataclasses import dataclass
from pathlib import Path

import numpy as np
import polars as pl
from scipy.optimize import minimize
from scipy.stats import norm

#: Classes whose gaps count as a heavy stream (G§2.4): a critical gap against them is the
#: driver's own times its heavy-stream factor.
HEAVY = ("bus", "truck", "tractor")


def gap_table(folder: str | Path) -> pl.DataFrame:
    """The gaps decided on in a run: one row per gap taken or turned down."""
    folder = Path(folder)
    ev = pl.read_parquet(folder / "events.parquet").filter(
        pl.col("kind").is_in(["gap_accepted", "gap_rejected"])
    )
    agents = pl.read_parquet(folder / "agents.parquet").select("agent_id", "class")
    rows = []
    for agent_id, kind, attrs in ev.select("agent_id", "kind", "attrs").iter_rows():
        a = json.loads(attrs)
        gap = a["gap_s"]
        rows.append(
            {
                "agent_id": agent_id,
                "gap_s": math.inf if gap is None else float(gap),
                "accepted": kind == "gap_accepted",
                "against_class": a["against_class"],
                "critical_gap_s": a["critical_gap_s"],
                "speed": float(a["speed"]),
            }
        )
    table = pl.DataFrame(
        rows,
        schema={
            "agent_id": pl.UInt64,
            "gap_s": pl.Float64,
            "accepted": pl.Boolean,
            "against_class": pl.Utf8,
            "critical_gap_s": pl.Float64,
            "speed": pl.Float64,
        },
    )
    return table.join(agents, on="agent_id", how="left")


def drivers(table: pl.DataFrame) -> pl.DataFrame:
    """Per driver that took a gap: the gap it took and the largest it turned down (0 if none)."""
    acc = table.filter(pl.col("accepted")).group_by("agent_id").agg(
        pl.col("gap_s").first().alias("accepted"), pl.col("class").first()
    )
    rej = table.filter(~pl.col("accepted")).group_by("agent_id").agg(
        pl.col("gap_s").max().alias("largest_rejected"), pl.len().alias("rejected")
    )
    return (
        acc.join(rej, on="agent_id", how="left")
        .with_columns(
            pl.col("largest_rejected").fill_null(0.0), pl.col("rejected").fill_null(0)
        )
        .sort("agent_id")
    )


def raff(accepted: Sequence[float], rejected: Sequence[float]) -> float:
    """Raff's critical gap: where #accepted shorter than t equals #rejected longer than t.

    Both counts are step functions of ``t``; the crossing is taken by linear interpolation
    between the gap values on either side of it. NaN if there is no crossing."""
    a = np.sort(np.asarray(accepted, dtype=float))
    r = np.sort(np.asarray(rejected, dtype=float))
    if a.size == 0 or r.size == 0:
        return math.nan
    grid = np.unique(np.concatenate([a[np.isfinite(a)], r[np.isfinite(r)]]))
    short = np.searchsorted(a, grid, side="left")  # accepted < t
    long = r.size - np.searchsorted(r, grid, side="right")  # rejected > t
    d = short - long
    k = np.nonzero(d >= 0)[0]
    if k.size == 0 or k[0] == 0:
        return math.nan
    k = int(k[0])
    d0, d1 = float(d[k - 1]), float(d[k])
    return float(grid[k - 1] + (grid[k] - grid[k - 1]) * (-d0) / (d1 - d0))


@dataclass(frozen=True)
class LognormalFit:
    """A lognormal population of critical gaps fitted by maximum likelihood."""

    median: float
    sigma: float
    #: Standard errors of the median and of sigma (delta method on the inverse Hessian).
    median_se: float
    sigma_se: float
    drivers: int


def mle(accepted: Sequence[float], largest_rejected: Sequence[float]) -> LognormalFit:
    """Troutbeck's maximum-likelihood estimate of a lognormal critical-gap population.

    Driver ``i`` took gap ``a_i`` and turned down nothing longer than ``r_i`` (0: it took the
    first gap it decided on); its critical gap lies in ``(r_i, a_i]``. A taken gap of infinity
    (nothing conflicting in reach) only says the critical gap is above ``r_i``."""
    a = np.asarray(accepted, dtype=float)
    r = np.asarray(largest_rejected, dtype=float)
    keep = a > r
    a, r = a[keep], r[keep]
    la = np.where(np.isfinite(a), np.log(np.maximum(a, 1e-12)), np.inf)
    lr = np.where(r > 0, np.log(np.maximum(r, 1e-12)), -np.inf)

    def nll(theta: np.ndarray) -> float:
        mu, log_s = theta
        s = math.exp(log_s)
        p = norm.cdf((la - mu) / s) - norm.cdf((lr - mu) / s)
        return -float(np.sum(np.log(np.maximum(p, 1e-300))))

    finite = a[np.isfinite(a)]
    start = np.array([math.log(np.median(finite)) if finite.size else 1.0, math.log(0.3)])
    res = minimize(nll, start, method="Nelder-Mead", options={"xatol": 1e-7, "fatol": 1e-9})
    mu, log_s = res.x
    # Inverse Hessian by central differences.
    h = 1e-4
    hess = np.zeros((2, 2))
    for i in range(2):
        for j in range(2):
            e_i, e_j = np.eye(2)[i] * h, np.eye(2)[j] * h
            hess[i, j] = (
                nll(res.x + e_i + e_j)
                - nll(res.x + e_i - e_j)
                - nll(res.x - e_i + e_j)
                + nll(res.x - e_i - e_j)
            ) / (4 * h * h)
    try:
        cov = np.linalg.inv(hess)
        se_mu, se_ls = math.sqrt(max(cov[0, 0], 0.0)), math.sqrt(max(cov[1, 1], 0.0))
    except np.linalg.LinAlgError:
        se_mu = se_ls = math.nan
    median, sigma = math.exp(mu), math.exp(log_s)
    return LognormalFit(
        median=median,
        sigma=sigma,
        median_se=median * se_mu,
        sigma_se=sigma * se_ls,
        drivers=int(a.size),
    )
