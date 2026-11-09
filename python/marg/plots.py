"""Standard figures for experiments and sensitivity analyses, in the house
style (:mod:`marg.style`). Each function returns a matplotlib Figure."""

from __future__ import annotations

from collections.abc import Sequence
from typing import Any

import polars as pl

from marg import style


def _fig(w: float = 6.0, h: float = 3.2, ncols: int = 1, nrows: int = 1, **kw: Any):
    import matplotlib.pyplot as plt

    style.use()
    return plt.subplots(nrows, ncols, figsize=(w, h), squeeze=False, **kw)


def ci_plot(
    summary: pl.DataFrame,
    x: str,
    series: str | None = "arm",
    ylabel: str = "",
    title: str = "",
) -> Any:
    """Mean ± CI (``summarize`` output) against ``x``, one series per ``series`` value."""
    fig, ax = _fig()
    ax = ax[0][0]
    groups = summary[series].unique(maintain_order=True).to_list() if series else [None]
    n = max(len(groups), 1)
    xs_all = sorted(summary[x].unique().to_list())
    step = (xs_all[1] - xs_all[0]) if len(xs_all) > 1 else 1.0
    for i, g in enumerate(groups):
        d = summary if g is None else summary.filter(pl.col(series) == g)
        d = d.sort(x)
        off = (i - (n - 1) / 2) * step * 0.06
        xs = [v + off for v in d[x].to_list()]
        hw = d["halfwidth"].fill_null(0).fill_nan(0).to_list()
        ax.errorbar(
            xs,
            d["mean"].to_list(),
            yerr=hw,
            fmt="o-",
            color=style.color(i),
            lw=1.6,
            ms=6,
            mec=style.SURFACE,
            mew=1.5,
            elinewidth=1.2,
            capsize=0,
            label=str(g) if g else None,
        )
    ax.set_xlabel(x)
    ax.set_ylabel(ylabel)
    if title:
        ax.set_title(title)
    if n > 1:
        ax.legend(loc="best")
    return fig


def paired_plot(paired: pl.DataFrame, x: str, ylabel: str = "", title: str = "") -> Any:
    """Paired difference (``paired_summary`` output) ± CI against ``x``, with zero."""
    fig, ax = _fig()
    ax = ax[0][0]
    d = paired.sort(x)
    ax.axhline(0, color=style.REFERENCE, lw=0.8)
    ax.errorbar(
        d[x].to_list(),
        d["mean"].to_list(),
        yerr=d["halfwidth"].fill_null(0).to_list(),
        fmt="o",
        color=style.color(0),
        ms=6,
        mec=style.SURFACE,
        mew=1.5,
        elinewidth=1.4,
        capsize=0,
    )
    ax.set_xlabel(x)
    ax.set_ylabel(ylabel)
    if title:
        ax.set_title(title)
    return fig


def effect_plot(effects: pl.DataFrame, ylabel: str = "", ncols: int = 3) -> Any:
    """Main effects (``main_effects`` output): one small multiple per factor,
    sharing the y axis so effect sizes compare directly."""
    factors = effects["factor"].unique(maintain_order=True).to_list()
    ncols = min(ncols, len(factors))
    nrows = -(-len(factors) // ncols)
    fig, axes = _fig(2.4 * ncols, 2.0 * nrows, ncols=ncols, nrows=nrows, sharey=True)
    for i, f in enumerate(factors):
        ax = axes[i // ncols][i % ncols]
        d = effects.filter(pl.col("factor") == f).sort("level")
        ax.plot(d["level"], d["mean"], "o-", color=style.color(0), ms=6, mec=style.SURFACE, mew=1.5)
        ax.set_title(_short(f), fontsize=8)
        if i % ncols == 0:
            ax.set_ylabel(ylabel)
    for j in range(len(factors), nrows * ncols):
        axes[j // ncols][j % ncols].set_visible(False)
    fig.tight_layout()
    return fig


def morris_plot(m: pl.DataFrame, output: str = "", label_top: int = 8) -> Any:
    """Morris screening: μ* against σ per factor; the top factors by μ* are
    labelled. The diagonal σ = μ* separates mostly-linear effects (below) from
    non-linear or interacting ones (above)."""
    fig, ax = _fig(6.0, 4.2)
    ax = ax[0][0]
    d = m.sort("mu_star", descending=True)
    hi = max(float(d["mu_star"].max() or 0), float(d["sigma"].max() or 0)) * 1.08 or 1.0
    ax.plot([0, hi], [0, hi], color=style.REFERENCE, lw=0.8)
    ax.text(hi * 0.98, hi * 0.98, "σ = μ*", color=style.MUTED, fontsize=7, ha="right", va="top")
    ax.errorbar(
        d["mu_star"].to_list(),
        d["sigma"].to_list(),
        xerr=d["mu_star_conf"].to_list(),
        fmt="o",
        color=style.color(0),
        ms=6,
        mec=style.SURFACE,
        mew=1.5,
        elinewidth=0.8,
        ecolor=style.AXIS,
        capsize=0,
    )
    for r in d.head(label_top).iter_rows(named=True):
        ax.annotate(
            _short(r["factor"]),
            (r["mu_star"], r["sigma"]),
            xytext=(4, 3),
            textcoords="offset points",
            fontsize=7,
            color=style.INK_2,
        )
    ax.set_xlim(0, hi)
    ax.set_ylim(0, hi)
    ax.set_xlabel(f"μ* (mean |elementary effect|){' of ' + output if output else ''}")
    ax.set_ylabel("σ (SD of elementary effects)")
    ax.set_title(f"Morris screening{': ' + output if output else ''}")
    return fig


def sobol_plot(s: pl.DataFrame, output: str = "", top: int | None = 20) -> Any:
    """First-order (S1) and total (ST) Sobol indices with CIs, factors sorted by ST.
    The gap ST − S1 is the share of variance from interactions."""
    import numpy as np

    d = s.sort("ST", descending=True)
    if top:
        d = d.head(top)
    n = d.height
    fig, ax = _fig(6.0, 0.8 + 0.32 * n)
    ax = ax[0][0]
    y = np.arange(n)[::-1]
    h = 0.36
    for i, (col, lab) in enumerate((("S1", "first order S1"), ("ST", "total ST"))):
        ax.barh(
            y + (h / 2 if i == 0 else -h / 2),
            d[col].to_list(),
            height=h * 0.92,
            xerr=d[f"{col}_conf"].to_list(),
            color=style.color(i),
            ecolor=style.INK_2,
            error_kw={"elinewidth": 0.8},
            label=lab,
        )
    ax.set_yticks(y, [_short(f) for f in d["factor"].to_list()])
    ax.axvline(0, color=style.AXIS, lw=0.8)
    ax.set_xlabel("share of output variance")
    ax.grid(axis="y", visible=False)
    ax.set_title(f"Sobol indices{': ' + output if output else ''}")
    ax.legend(loc="lower right")
    return fig


def timeseries(
    df: pl.DataFrame,
    t: str,
    y: str,
    series: str | None = None,
    ylabel: str = "",
    title: str = "",
    xlabel: str = "",
) -> Any:
    """A time series per ``series`` value (at most 4 lines; facet beyond that)."""
    fig, ax = _fig(6.0, 2.8)
    ax = ax[0][0]
    groups = df[series].unique(maintain_order=True).to_list() if series else [None]
    for i, g in enumerate(groups[:4]):
        d = (df if g is None else df.filter(pl.col(series) == g)).sort(t)
        ax.plot(d[t], d[y], color=style.color(i), lw=1.6, label=None if g is None else str(g))
    ax.set_xlabel(xlabel or t)
    ax.set_ylabel(ylabel or y)
    if title:
        ax.set_title(title)
    if len(groups) > 1:
        ax.legend(loc="best")
    return fig


def _short(path: str, keep: int = 2) -> str:
    """The last ``keep`` parts of a register path (axis labels)."""
    parts = path.split(".")
    return ".".join(parts[-keep:]) if len(parts) > keep else path


__all__: Sequence[str] = (
    "ci_plot",
    "effect_plot",
    "morris_plot",
    "paired_plot",
    "sobol_plot",
    "timeseries",
)
