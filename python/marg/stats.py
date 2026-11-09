"""Summaries for experiment results: confidence intervals, paired differences
under common random numbers, main effects (plan P3.6)."""

from __future__ import annotations

from collections.abc import Sequence

import polars as pl
from scipy import stats as _st


def _t(confidence: float, df: pl.Expr) -> pl.Expr:
    return df.map_elements(
        lambda d: float(_st.t.ppf(0.5 + confidence / 2, d)) if d >= 1 else float("nan"),
        return_dtype=pl.Float64,
    )


def summarize(
    df: pl.DataFrame,
    metric: str,
    by: Sequence[str] = ("scenario", "point", "arm"),
    confidence: float = 0.95,
) -> pl.DataFrame:
    """Mean, SD, n and the Student-t CI of ``metric`` per group."""
    by = [b for b in by if b in df.columns]
    g = df.group_by(by, maintain_order=True) if by else df.select(pl.all())
    agg = [
        pl.col(metric).count().alias("n"),
        pl.col(metric).mean().alias("mean"),
        pl.col(metric).std().alias("sd"),
    ]
    out = g.agg(agg) if by else df.select(agg)
    out = out.with_columns(
        (_t(confidence, pl.col("n") - 1) * pl.col("sd") / pl.col("n").sqrt()).alias("halfwidth")
    ).with_columns(
        (pl.col("mean") - pl.col("halfwidth")).alias("lo"),
        (pl.col("mean") + pl.col("halfwidth")).alias("hi"),
    )
    return out.sort(by) if by else out


def paired_differences(
    df: pl.DataFrame,
    metric: str,
    arm: str,
    baseline: str,
    by: Sequence[str] = ("scenario", "point"),
) -> pl.DataFrame:
    """Per replication: ``metric`` in ``arm`` minus ``metric`` in ``baseline``,
    matched on ``by`` and ``rep`` (the pairs share a seed under CRN)."""
    keys = [k for k in (*by, "rep") if k in df.columns]
    a = df.filter(pl.col("arm") == arm).select(*keys, pl.col(metric).alias("a"))
    b = df.filter(pl.col("arm") == baseline).select(*keys, pl.col(metric).alias("b"))
    return (
        a.join(b, on=keys, how="inner")
        .with_columns((pl.col("a") - pl.col("b")).alias("diff"))
        .sort(keys)
    )


def paired_summary(
    df: pl.DataFrame,
    metric: str,
    arm: str,
    baseline: str,
    by: Sequence[str] = ("scenario", "point"),
    confidence: float = 0.95,
) -> pl.DataFrame:
    """CI of the paired difference, and the variance-reduction ratio
    ``var(a − b) / (var(a) + var(b))`` (1 for independent arms, below 1 when
    common random numbers induce positive correlation)."""
    d = paired_differences(df, metric, arm, baseline, by)
    keys = [k for k in by if k in d.columns]
    s = summarize(d, "diff", keys, confidence)
    exprs = [
        (pl.col("diff").var() / (pl.col("a").var() + pl.col("b").var())).alias("variance_ratio"),
        pl.corr("a", "b").alias("correlation"),
    ]
    v = d.group_by(keys, maintain_order=True).agg(exprs) if keys else d.select(exprs)
    return s.join(v, on=keys) if keys else pl.concat([s, v], how="horizontal")


def main_effects(df: pl.DataFrame, metric: str, factors: Sequence[str]) -> pl.DataFrame:
    """Mean of ``metric`` at each level of each factor (long format: factor,
    level, mean, n). Meant for grid and factorial designs."""
    parts = []
    for f in factors:
        parts.append(
            df.group_by(f)
            .agg(pl.col(metric).mean().alias("mean"), pl.len().alias("n"))
            .rename({f: "level"})
            .with_columns(pl.lit(f).alias("factor"), pl.col("level").cast(pl.Float64))
            .select("factor", "level", "mean", "n")
            .sort("level")
        )
    return pl.concat(parts) if parts else pl.DataFrame()
