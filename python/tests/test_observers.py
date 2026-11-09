"""The online observers agree with metrics recomputed from trajectories (plan P2.7)."""

from pathlib import Path

import marg
import polars as pl
import pytest
from marg import analysis

REPO = Path(__file__).resolve().parents[2]
SCENARIO = """
name: crosscheck
duration_s: 600
warmup_s: 60
network:
  ribbons:
    - {id: main, polyline: [[0, 0], [800, 0]], width: [{s: 0, y_min: -5.25, y_max: 5.25}]}
demand:
  entries:
    - id: west
      ribbon: main
      flow: [{from_s: 0, veh_per_h: 2400}]
      composition: {motorcycle: 0.3, scooter: 0.2, hatchback: 0.3, auto_rickshaw: 0.15, bus: 0.05}
detectors:
  - {kind: screenline, id: s400, ribbon: main, s: 400, interval_s: 60}
  - {kind: area, id: a200_600, ribbon: main, s_from: 200, s_to: 600, interval_s: 60}
kpi: {throughput_detector: s400}
outputs:
  trajectories: {every_s: 0.1, from_t: 0}
"""


@pytest.fixture(scope="module")
def result(tmp_path_factory: pytest.TempPathFactory) -> marg.RunResult:
    out = tmp_path_factory.mktemp("run")
    return marg.run(SCENARIO, seed=7, out=out, params_dir=REPO / "params" / "base")


def test_screenline_counts_match(result: marg.RunResult) -> None:
    traj = result.table("trajectories")
    py = analysis.screenline_counts(traj, "main", 400.0, 60.0)
    rs = (
        result.detectors.filter(pl.col("detector_id") == "s400")
        .group_by("t_start")
        .agg(pl.col("count").sum())
        .sort("t_start")
    )
    joined = rs.join(py, on="t_start", suffix="_py")
    assert joined.height >= 9
    assert (joined["count"] == joined["count_py"]).all()


def test_area_tts_ttd_match(result: marg.RunResult) -> None:
    traj = result.table("trajectories")
    py = analysis.edie_area(traj, "main", 200.0, 600.0, 60.0)
    rs = (
        result.detectors.filter(pl.col("detector_id") == "a200_600")
        .group_by("t_start")
        .agg(pl.col("tts").sum(), pl.col("ttd").sum())
        .sort("t_start")
    )
    joined = rs.join(py, on="t_start", suffix="_py").filter(pl.col("tts") > 0)
    assert joined.height >= 9
    for col in ("tts", "ttd"):
        rel = ((joined[col] - joined[f"{col}_py"]).abs() / joined[col]).max()
        assert rel < 0.005, f"{col} differs by {rel:.4%}"


def test_throughput_kpi_matches_detector(result: marg.RunResult) -> None:
    d = result.detectors.filter((pl.col("detector_id") == "s400") & ~pl.col("in_warmup"))
    per_hour = d["count"].sum() * 3600 / (600 - 60)
    assert abs(result.kpis["throughput_veh_h"] - per_hour) / per_hour < 0.02


def test_conservation(result: marg.RunResult) -> None:
    k = result.kpis
    assert k["spawned"] == k["exited"] + k["in_network_at_end"]
    assert k["collision_guard_events"] == 0
    assert result.agents.height == k["spawned"]
