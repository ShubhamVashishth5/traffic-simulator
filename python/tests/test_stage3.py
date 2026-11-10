"""P5.8: stage-3 statistics on trajectory tables with known answers, the
distances between lateral distributions, and the response-surface fit on a
rough analytic model."""

from __future__ import annotations

import hashlib
import math
from pathlib import Path

import marg
import numpy as np
import polars as pl
import pytest
from conftest import PARAMS, synthetic_register
from marg import stage3
from marg.calibrate import Moments, Stage

SEC = stage3.Section(s_line=50.0, s_from=0.0, s_to=100.0, window=10.0)


def table(rows: list[tuple]) -> pl.DataFrame:
    return pl.DataFrame(
        rows, schema=["t", "agent_id", "class", "s", "y", "length", "width"], orient="row"
    )


def drive(agent: int, cls: str, y0: float, y1: float, v: float, t_end: float, dt: float = 0.5):
    """A vehicle from s = 0 at speed v, moving linearly from y0 to y1."""
    rows = []
    for k in range(int(t_end / dt) + 1):
        t = k * dt
        f = t / t_end
        rows.append((t, agent, cls, v * t, y0 + f * (y1 - y0), 4.0, 1.8))
    return rows


def test_crossings_interpolate_position_and_time() -> None:
    tr = table(drive(1, "car", 0.0, 2.0, 10.0, 10.0))
    c = stage3.crossings(tr, 50.0)
    assert c.height == 1
    assert c["t"][0] == pytest.approx(5.0)
    assert c["y"][0] == pytest.approx(1.0)


def test_side_gaps_take_the_nearest_vehicle_beside() -> None:
    # Three vehicles abreast at the same s: gaps 0.7 m and 1.2 m; one far ahead.
    rows = [
        (0.0, 1, "car", 10.0, 0.0, 4.0, 1.8),
        (0.0, 2, "car", 10.5, 2.5, 4.0, 1.8),
        (0.0, 3, "car", 9.0, 5.5, 4.0, 1.8),
        (0.0, 4, "car", 30.0, 1.0, 4.0, 1.8),
    ]
    g = stage3.side_gaps(table(rows), 0.0, 100.0).sort("agent_id")
    assert g["agent_id"].to_list() == [1, 2]
    assert g["gap"].to_list() == pytest.approx([0.7, 1.2])


def test_moments_have_known_values() -> None:
    # Ten cars at y = ±1 through a 100 m area at 10 m/s, one every 4 s, with
    # samples every 0.5 s: each spends 10 s in the area.
    rows = []
    for k in range(10):
        for r in drive(k, "car", 1.0 if k % 2 else -1.0, 1.0 if k % 2 else -1.0, 10.0, 12.0):
            rows.append((r[0] + 4.0 * k, *r[1:]))
    m = stage3.observed_moments(table(rows), SEC, ["car"])
    v = dict(zip(m.names, m.values, strict=True))
    assert v["lateral_mean_car"] == pytest.approx(0.0)
    assert v["lateral_sd_car"] == pytest.approx(1.0)
    assert v["lateral_per_km_car"] == pytest.approx(0.0)
    # Time spent: 10 cars × (samples in [0, 100] m) × 0.5 s, over 100 m × the window total.
    time = 10.0 * math.ceil((4.0 * 9 + 12.0 + 0.5) / 10.0)
    assert v["density_car"] == pytest.approx(1000.0 * 10 * 21 * 0.5 / (100.0 * time))
    assert np.all(np.isfinite(m.se[:3]))


def test_lateral_activity_counts_sideways_metres_per_km() -> None:
    # 3 m sideways over 100 m driven in the area: 30 m per km.
    tr = table(drive(1, "two_wheeler", -1.5, 1.5, 10.0, 10.0))
    sums = stage3.cluster_sums(tr, SEC, ["two_wheeler"])
    tot = sums.sum()
    assert 1000 * tot["dy_two_wheeler"][0] / tot["ds_two_wheeler"][0] == pytest.approx(30.0)


def test_distances() -> None:
    rng = np.random.default_rng(1)
    a = rng.normal(0.0, 1.0, 4000)
    assert stage3.wasserstein(a, a + 0.5) == pytest.approx(0.5)
    assert stage3.ks(a, a) == 0.0
    assert stage3.ks(a, a + 10) == 1.0
    sim = table(drive(1, "car", 0.0, 0.0, 10.0, 10.0) + drive(2, "car", 2.0, 2.0, 10.0, 10.0))
    obs = sim.with_columns(pl.col("y") + 0.3)
    d = stage3.lateral_distances(sim, obs, 50.0, ["car"])
    assert d["car"] == pytest.approx(0.3)


def test_statistic_reads_run_folders(tmp_path: Path) -> None:
    sc = marg.Scenario.load(Path(__file__).resolve().parents[2] / "scenarios/stage3_recovery.yaml")
    sc = sc.with_changes({"duration_s": 300})
    runs = [
        marg.run(sc.to_yaml(), seed=s, out=tmp_path / f"s{s}", params_dir=PARAMS).path
        for s in (1, 2)
    ]
    sec = stage3.Section(s_line=600, s_from=400, s_to=800, window=60, t_from=120)
    m = stage3.statistic(sec, ["two_wheeler", "car"])(runs)
    v = dict(zip(m.names, m.values, strict=True))
    assert 0.5 < v["side_gap_mean"] < 2.5
    assert v["density_car"] > 1.0 and v["density_two_wheeler"] > 1.0
    assert np.all(m.se > 0)
    only = stage3.statistic(sec, ["car"], names=["side_gap_mean"])(runs)
    assert only.names == ["side_gap_mean"]
    assert only.values[0] == pytest.approx(v["side_gap_mean"])


def rough(x: np.ndarray, scale: float) -> float:
    """Deterministic roughness: a pseudo-random offset that changes with any
    change of `x`, like a simulated statistic under common random numbers."""
    h = hashlib.blake2b(np.asarray(x, dtype=float).tobytes(), digest_size=8).digest()
    return scale * (int.from_bytes(h, "little") / 2**64 - 0.5) * math.sqrt(12)


def test_response_surface_recovers_a_rough_model(tmp_path: Path) -> None:
    reg = synthetic_register(tmp_path, {"a": (0.0, 10.0), "b": (0.0, 10.0)}, role="fit", stage=1)
    st = Stage(1, tmp_path / "cal", reg)
    truth = np.array([3.0, 7.0])
    se = 0.05

    def simulate(ov, noisy=True):
        x = np.array([ov["t.a"], ov["t.b"]])
        clean = np.array([x[0] + x[1], x[0] - x[1], 0.3 * x[0] * x[1]])
        off = np.array([rough(x + k, se) for k in range(3)]) if noisy else 0.0
        return Moments(["u", "v", "w"], clean + off, [se] * 3)

    observed = simulate({"t.a": truth[0], "t.b": truth[1]}, noisy=False)
    r = st.fit(["t.a", "t.b"], simulate, observed, method="response_surface", max_evaluations=200)
    assert r.estimate == pytest.approx(truth, abs=0.1)
    lo, hi = r.ci[:, 0], r.ci[:, 1]
    assert np.all((lo <= truth) & (truth <= hi)), r.ci
    assert r.evaluations <= 200
