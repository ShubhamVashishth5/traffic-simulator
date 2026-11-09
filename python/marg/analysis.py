"""Metrics recomputed from trajectories, used to cross-check the online
observers (plan P2.7) and for analyses the observers don't cover."""

from __future__ import annotations

import polars as pl


def _with_previous(traj: pl.DataFrame) -> pl.DataFrame:
    """Add each agent's previous sample (same ribbon) as ``s_prev``/``t_prev``."""
    return (
        traj.sort(["agent_id", "t"])
        .with_columns(
            pl.col("s").shift(1).over("agent_id").alias("s_prev"),
            pl.col("t").shift(1).over("agent_id").alias("t_prev"),
            pl.col("ribbon").shift(1).over("agent_id").alias("ribbon_prev"),
        )
        .filter(pl.col("ribbon_prev") == pl.col("ribbon"))
    )


def screenline_counts(traj: pl.DataFrame, ribbon: str, s: float, interval: float) -> pl.DataFrame:
    """Crossings of station ``s`` per interval, from consecutive samples.

    Exact when trajectories are sampled every step.
    """
    d = _with_previous(traj).filter(
        (pl.col("ribbon") == ribbon) & (pl.col("s_prev") < s) & (pl.col("s") >= s)
    )
    frac = (s - pl.col("s_prev")) / (pl.col("s") - pl.col("s_prev"))
    tc = pl.col("t_prev") + frac * (pl.col("t") - pl.col("t_prev"))
    return (
        d.with_columns((tc / interval).floor().alias("k"))
        .group_by("k")
        .agg(pl.len().alias("count"))
        .with_columns((pl.col("k") * interval).alias("t_start"))
        .drop("k")
        .sort("t_start")
    )


def edie_area(
    traj: pl.DataFrame, ribbon: str, s_from: float, s_to: float, interval: float
) -> pl.DataFrame:
    """Total time spent and distance travelled in ``[s_from, s_to]`` per
    interval (Edie), from consecutive samples with linear motion between them.
    """
    d = _with_previous(traj).filter(pl.col("ribbon") == ribbon)
    lo = pl.max_horizontal(pl.col("s_prev"), pl.lit(s_from))
    hi = pl.min_horizontal(pl.col("s"), pl.lit(s_to))
    overlap = pl.max_horizontal(hi - lo, pl.lit(0.0))
    ds = pl.col("s") - pl.col("s_prev")
    dt = pl.col("t") - pl.col("t_prev")
    inside_still = (ds <= 0) & (pl.col("s") >= s_from) & (pl.col("s") <= s_to)
    tts = pl.when(ds > 0).then(dt * overlap / ds).when(inside_still).then(dt).otherwise(0.0)
    return (
        d.with_columns(
            overlap.alias("ttd"), tts.alias("tts"), (pl.col("t_prev") / interval).floor().alias("k")
        )
        .group_by("k")
        .agg(pl.col("tts").sum(), pl.col("ttd").sum())
        .with_columns((pl.col("k") * interval).alias("t_start"))
        .drop("k")
        .sort("t_start")
    )


def speed_by_type(
    detectors: pl.DataFrame, detector_id: str, include_warmup: bool = False
) -> pl.DataFrame:
    """Pooled spot-speed mean and SD (km/h) per vehicle type at a screenline.

    Detector rows hold per-interval counts and means; the pooled SD needs the
    per-crossing speeds, so this uses trajectories-free moments: it returns the
    count-weighted mean of the time-mean speeds and leaves SD to agent-level data.
    """
    d = detectors.filter((pl.col("detector_id") == detector_id) & (pl.col("kind") == "screenline"))
    if not include_warmup:
        d = d.filter(~pl.col("in_warmup"))
    return (
        d.group_by("vehicle_type")
        .agg(
            pl.col("count").sum(),
            (
                (pl.col("time_mean_speed") * pl.col("count")).sum() / pl.col("count").sum() * 3.6
            ).alias("mean_kmh"),
        )
        .sort("vehicle_type")
    )


def screenline_crossings(traj: pl.DataFrame, ribbon: str, s: float) -> pl.DataFrame:
    """One row per crossing of station ``s``: agent, crossing time, lateral
    position and the mean speed between the samples either side (m/s)."""
    d = _with_previous(traj).filter(
        (pl.col("ribbon") == ribbon) & (pl.col("s_prev") < s) & (pl.col("s") >= s)
    )
    frac = (s - pl.col("s_prev")) / (pl.col("s") - pl.col("s_prev"))
    return d.select(
        "agent_id",
        (pl.col("t_prev") + frac * (pl.col("t") - pl.col("t_prev"))).alias("t"),
        "y",
        ((pl.col("s") - pl.col("s_prev")) / (pl.col("t") - pl.col("t_prev"))).alias("speed"),
    )
