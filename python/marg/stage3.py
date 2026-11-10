"""Stage-3 calibration statistics: lateral behaviour (plan P5.8, G§8.3).

Everything here works on a **trajectory table**, so that simulated runs and
real trajectory data (for example the Chennai set of Kanagaraj et al.) go
through the same code. One row per vehicle and time sample:

    t, agent_id, class, s, y, length, width   (SI; `s` along the road,
                                               `y` across it, median side low)

:func:`trajectories` builds it from a run folder. For observed data, rename the
source's columns to these (vehicle dimensions by type are enough).

Statistics, by vehicle class:

- where vehicles cross a section, laterally (mean and SD);
- partial density in an area (Edie: time spent / (length × time), veh/km);
- lateral distance travelled per km driven in the area;
- and pooled over classes, the edge-to-edge gap to the nearest vehicle beside
  each vehicle (Budhkar & Maurya 2017 measured these, dropping pairs more than
  2.5 m apart; so does this): its mean, SD and the share below a small gap.

Each statistic is a function of sums over clusters (time windows × runs), and
its standard error is the delete-one-cluster jackknife, so ratios and standard
deviations get honest errors. :func:`statistic` packs them as
:class:`marg.calibrate.Moments` for :meth:`marg.calibrate.Stage.fit`.

Distances between simulated and observed lateral-position distributions
(Wasserstein-1, Kolmogorov–Smirnov) are in :func:`lateral_distances`.
"""

from __future__ import annotations

import math
from collections.abc import Callable, Sequence
from dataclasses import dataclass
from pathlib import Path

import numpy as np
import polars as pl

from marg.calibrate import Moments

#: Gaps beyond this (m) are not "beside" each other (Budhkar & Maurya 2017).
MAX_SIDE_GAP = 2.5


def trajectories(run_dir: str | Path, ribbon: int | None = None) -> pl.DataFrame:
    """The trajectory table of a run folder (needs ``outputs.trajectories``).

    ``ribbon`` keeps one ribbon (by index); stations are per ribbon.
    """
    p = Path(run_dir)
    tr = pl.read_parquet(p / "trajectories.parquet")
    if ribbon is not None:
        tr = tr.filter(pl.col("ribbon") == ribbon)
    ag = pl.read_parquet(p / "agents.parquet").select("agent_id", "class", "length", "width")
    return tr.select("t", "agent_id", "s", "y").join(ag, on="agent_id", how="left")


@dataclass(frozen=True)
class Section:
    """Where the statistics are measured: a crossing line and an area."""

    s_line: float
    s_from: float
    s_to: float
    #: Cluster length (s): standard errors treat each window of each run as
    #: one independent observation.
    window: float = 60.0
    #: Only samples at or after this time count (warm-up).
    t_from: float = 0.0
    #: A gap below this (m) counts as "tight" for the side-gap share.
    tight_gap: float = 1.0

    @property
    def length(self) -> float:
        return self.s_to - self.s_from


def _clustered(tr: pl.DataFrame, sec: Section, run: int) -> pl.DataFrame:
    return tr.filter(pl.col("t") >= sec.t_from).with_columns(
        cluster=pl.lit(run) * 1_000_000 + ((pl.col("t") - sec.t_from) // sec.window).cast(pl.Int64)
    )


def crossings(tr: pl.DataFrame, s_line: float) -> pl.DataFrame:
    """Lateral position (interpolated) where each vehicle crosses ``s_line``."""
    t = tr.sort("agent_id", "t").with_columns(
        s1=pl.col("s").shift(-1).over("agent_id"),
        y1=pl.col("y").shift(-1).over("agent_id"),
        t1=pl.col("t").shift(-1).over("agent_id"),
    )
    t = t.filter((pl.col("s") < s_line) & (pl.col("s1") >= s_line))
    f = (s_line - pl.col("s")) / (pl.col("s1") - pl.col("s"))
    return t.select(
        "agent_id",
        "class",
        t=pl.col("t") + f * (pl.col("t1") - pl.col("t")),
        y=pl.col("y") + f * (pl.col("y1") - pl.col("y")),
    )


def side_gaps(tr: pl.DataFrame, s_from: float, s_to: float) -> pl.DataFrame:
    """For each vehicle and sample in the area, the edge-to-edge lateral gap
    to the nearest vehicle beside it (bodies overlapping along the road) on
    its higher-``y`` side, if within :data:`MAX_SIDE_GAP`."""
    a = tr.filter((pl.col("s") >= s_from) & (pl.col("s") <= s_to)).select(
        "t", "agent_id", "s", "y", "length", "width"
    )
    pairs = a.join(a, on="t", suffix="_j").filter(
        (pl.col("y_j") > pl.col("y"))
        & ((pl.col("s_j") - pl.col("s")).abs() < 0.5 * (pl.col("length") + pl.col("length_j")))
    )
    gap = (pl.col("y_j") - pl.col("y")) - 0.5 * (pl.col("width") + pl.col("width_j"))
    return (
        pairs.with_columns(gap=gap)
        .group_by("t", "agent_id")
        .agg(pl.col("gap").min())
        .filter(pl.col("gap") <= MAX_SIDE_GAP)
    )


def cluster_sums(
    tr: pl.DataFrame, sec: Section, classes: Sequence[str], run: int = 0
) -> pl.DataFrame:
    """Sufficient statistics per cluster (one row per cluster)."""
    tr = _clustered(tr, sec, run)
    clusters = tr.select("cluster").unique()
    dt = _sample_interval(tr)
    parts = [clusters]
    cr = crossings(tr, sec.s_line)
    cr = cr.with_columns(
        cluster=pl.lit(run) * 1_000_000 + ((pl.col("t") - sec.t_from) // sec.window).cast(pl.Int64)
    )
    area = tr.filter((pl.col("s") >= sec.s_from) & (pl.col("s") <= sec.s_to))
    moves = (
        tr.sort("agent_id", "t")
        .with_columns(
            ds=pl.col("s").diff().over("agent_id"),
            dy=pl.col("y").diff().abs().over("agent_id"),
        )
        .filter(
            (pl.col("s") >= sec.s_from) & (pl.col("s") <= sec.s_to) & pl.col("ds").is_not_null()
        )
    )
    for c in classes:
        x = (
            cr.filter(pl.col("class") == c)
            .group_by("cluster")
            .agg(
                pl.len().alias(f"n_{c}"),
                pl.col("y").sum().alias(f"sy_{c}"),
                (pl.col("y") ** 2).sum().alias(f"syy_{c}"),
            )
        )
        d = (
            area.filter(pl.col("class") == c)
            .group_by("cluster")
            .agg((pl.len() * dt).alias(f"tts_{c}"))
        )
        m = (
            moves.filter(pl.col("class") == c)
            .group_by("cluster")
            .agg(pl.col("ds").sum().alias(f"ds_{c}"), pl.col("dy").sum().alias(f"dy_{c}"))
        )
        parts += [x, d, m]
    g = (
        _clustered(side_gaps(tr, sec.s_from, sec.s_to), sec, run)
        .group_by("cluster")
        .agg(
            pl.len().alias("n_gap"),
            pl.col("gap").sum().alias("s_gap"),
            (pl.col("gap") ** 2).sum().alias("ss_gap"),
            (pl.col("gap") < sec.tight_gap).sum().alias("n_tight"),
        )
    )
    parts.append(g)
    out = parts[0]
    for p in parts[1:]:
        out = out.join(p, on="cluster", how="left")
    return out.fill_null(0).with_columns(time=pl.lit(sec.window)).sort("cluster")


def _sample_interval(tr: pl.DataFrame) -> float:
    ts = tr.select(pl.col("t").unique().sort()).to_series().to_numpy()
    return float(np.median(np.diff(ts))) if len(ts) > 1 else math.nan


def _values(tot: dict[str, float], sec: Section, classes: Sequence[str]) -> dict[str, float]:
    """The statistics from summed sufficient statistics."""
    v: dict[str, float] = {}
    for c in classes:
        n = tot[f"n_{c}"]
        mean = tot[f"sy_{c}"] / n if n else math.nan
        v[f"lateral_mean_{c}"] = mean
        v[f"lateral_sd_{c}"] = math.sqrt(max(tot[f"syy_{c}"] / n - mean**2, 0.0)) if n else math.nan
        v[f"density_{c}"] = 1000.0 * tot[f"tts_{c}"] / (sec.length * tot["time"])
        ds = tot[f"ds_{c}"]
        v[f"lateral_per_km_{c}"] = 1000.0 * tot[f"dy_{c}"] / ds if ds else math.nan
    ng = tot["n_gap"]
    gm = tot["s_gap"] / ng if ng else math.nan
    v["side_gap_mean"] = gm
    v["side_gap_sd"] = math.sqrt(max(tot["ss_gap"] / ng - gm**2, 0.0)) if ng else math.nan
    v["side_gap_tight_share"] = tot["n_tight"] / ng if ng else math.nan
    return v


def moments(sums: pl.DataFrame, sec: Section, classes: Sequence[str]) -> Moments:
    """Statistics and jackknife standard errors from :func:`cluster_sums`."""
    cols = [c for c in sums.columns if c != "cluster"]
    m = sums.select(cols).to_numpy().astype(float)
    total = m.sum(axis=0)
    full = _values(dict(zip(cols, total, strict=True)), sec, classes)
    names = list(full)
    k = len(m)
    if k < 2:
        raise ValueError("need at least two clusters for standard errors")
    jack = np.array(
        [
            list(_values(dict(zip(cols, total - m[i], strict=True)), sec, classes).values())
            for i in range(k)
        ]
    )
    se = np.full(len(names), math.nan)
    for j in range(len(names)):
        x = jack[:, j][np.isfinite(jack[:, j])]
        if len(x) >= 2:
            se[j] = math.sqrt((k - 1) / k * float(np.sum((x - x.mean()) ** 2)))
    return Moments(names, [full[n] for n in names], se)


def statistic(
    sec: Section, classes: Sequence[str], names: Sequence[str] | None = None
) -> Callable[[list[Path]], Moments]:
    """A :class:`marg.calibrate.Simulator` statistic: stage-3 moments pooled
    over the run folders (one per seed), optionally only ``names``."""

    def stat(dirs: list[Path]) -> Moments:
        sums = pl.concat(
            [cluster_sums(trajectories(d), sec, classes, run=i) for i, d in enumerate(dirs)],
            how="diagonal",
        ).fill_null(0)
        m = moments(sums, sec, classes)
        if names is None:
            return m
        idx = [m.names.index(n) for n in names]
        return Moments(list(names), m.values[idx], m.se[idx])

    return stat


def observed_moments(tr: pl.DataFrame, sec: Section, classes: Sequence[str]) -> Moments:
    """Stage-3 moments of an observed trajectory table (see the module docs)."""
    return moments(cluster_sums(tr, sec, classes), sec, classes)


# ----- distances between lateral-position distributions -----


def wasserstein(a: np.ndarray, b: np.ndarray) -> float:
    """Wasserstein-1 (earth mover's) distance between two samples (m)."""
    from scipy.stats import wasserstein_distance

    return float(wasserstein_distance(a, b))


def ks(a: np.ndarray, b: np.ndarray) -> float:
    """Two-sample Kolmogorov–Smirnov distance."""
    from scipy.stats import ks_2samp

    return float(ks_2samp(a, b).statistic)


def lateral_distances(
    simulated: pl.DataFrame,
    observed: pl.DataFrame,
    s_line: float,
    classes: Sequence[str],
    metric: str = "wasserstein",
) -> dict[str, float]:
    """Per class, the distance between the simulated and observed lateral
    positions where vehicles cross ``s_line`` (trajectory tables)."""
    f = {"wasserstein": wasserstein, "ks": ks}[metric]
    cs, co = crossings(simulated, s_line), crossings(observed, s_line)
    out = {}
    for c in classes:
        a = cs.filter(pl.col("class") == c)["y"].to_numpy()
        b = co.filter(pl.col("class") == c)["y"].to_numpy()
        out[c] = f(a, b) if len(a) and len(b) else math.nan
    return out
