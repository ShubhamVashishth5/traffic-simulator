from pathlib import Path

import marg
import polars as pl

REPO = Path(__file__).resolve().parents[2]
TOY = """
name: toy
duration_s: 120
network:
  ribbons:
    - {id: r, polyline: [[0, 0], [1000, 0]], width: [{s: 0, y_min: -3.5, y_max: 3.5}]}
demand:
  entries:
    - {id: e, ribbon: r, flow: [{from_s: 0, veh_per_h: 1800}], composition: {hatchback: 1.0}}
"""


def test_version() -> None:
    assert marg.version().count(".") == 2


def test_run_writes_run_folder(tmp_path: Path) -> None:
    res = marg.run(TOY, seed=3, out=tmp_path / "run", params_dir=REPO / "params" / "base")
    for f in ["manifest.json", "spec.resolved.yaml", "params.parquet"]:
        assert (tmp_path / "run" / f).exists(), f
    assert res.manifest["status"] == "ok"
    assert res.manifest["seed"] == 3
    params = pl.read_parquet(tmp_path / "run" / "params.parquet")
    assert params.height > 200
    assert set(params["grade"].unique()) <= {"A", "B", "C", "A/B", "B/C"}


def test_deterministic_across_threads(tmp_path: Path) -> None:
    pdir = REPO / "params" / "base"
    a = marg.run(TOY, seed=5, out=tmp_path / "a", threads=1, params_dir=pdir)
    b = marg.run(TOY, seed=5, out=tmp_path / "b", threads=4, params_dir=pdir)
    assert a.manifest["output_hash"] == b.manifest["output_hash"]
    assert a.manifest["run_hash"] == b.manifest["run_hash"]
