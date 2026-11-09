from pathlib import Path

import marg

TOY = """
name: toy
duration_s: 120
toy_arrivals_per_h: 1800
"""


def test_version() -> None:
    assert marg.version().count(".") == 2


def test_run_writes_manifest(tmp_path: Path) -> None:
    res = marg.run(TOY, seed=3, out=tmp_path / "run")
    assert (tmp_path / "run" / "manifest.json").exists()
    assert res.manifest["status"] == "ok"
    assert res.manifest["seed"] == 3


def test_deterministic_across_threads(tmp_path: Path) -> None:
    a = marg.run(TOY, seed=5, out=tmp_path / "a", threads=1)
    b = marg.run(TOY, seed=5, out=tmp_path / "b", threads=4)
    assert a.manifest["output_hash"] == b.manifest["output_hash"]
