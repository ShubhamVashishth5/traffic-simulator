"""P3.1: experiment expansion is deterministic; run hashes match the core."""

from __future__ import annotations

import json
from pathlib import Path

import marg
import pytest
from conftest import PARAMS
from marg.design import Grid


def make(tiny: marg.Scenario, root: Path, **kw) -> marg.Experiment:
    args = dict(
        design=Grid({"vehicles.free_flow_speed.hatchback": [45.0, 60.0]}),
        arms={"observed": {}, "slow_bikes": {"vehicles.free_flow_speed.motorcycle": 40.0}},
        replications=marg.Replications(n=2),
        root=root,
        params_dir=PARAMS,
    )
    args.update(kw)
    return marg.Experiment("exp", tiny, **args)


def test_identical_experiments_give_identical_run_lists(tiny, tmp_path: Path) -> None:
    a = make(tiny, tmp_path / "a")
    b = make(marg.Scenario.from_yaml(tiny.to_yaml()), tmp_path / "b")
    assert a.expand() == b.expand()
    a.write()
    b.write()
    assert (a.dir / "runs.jsonl").read_bytes() == (b.dir / "runs.jsonl").read_bytes()
    assert (a.dir / "experiment.yaml").read_bytes() == (b.dir / "experiment.yaml").read_bytes()
    assert a.definition_hash() == b.definition_hash()


def test_run_list_layout(tiny, tmp_path: Path) -> None:
    exp = make(tiny, tmp_path)
    exp.write()
    runs = exp.run_list()
    assert len(runs) == 2 * 2 * 2  # points x arms x replications
    assert [r["run_id"] for r in runs] == list(range(8))
    assert len({r["run_hash"] for r in runs}) == 8
    for r in runs:
        assert r["path"] == (
            f"scenario=tiny/point={r['point']}/arm={r['arm']}/rep={r['rep']}/run={r['run_hash']}"
        )
        assert r["seed"] == 1 + r["rep"]  # crn="all": seed depends on the replication only
        assert (exp.dir / r["scenario_file"]).exists()
    assert sorted(p.name for p in (exp.dir / "params").iterdir()) == sorted(
        p.name for p in PARAMS.glob("*.yaml")
    )


def test_changes_change_hashes(tiny, tmp_path: Path) -> None:
    base = {r["run_hash"] for r in make(tiny, tmp_path).expand()}
    other_seed = {r["run_hash"] for r in make(tiny, tmp_path, seed=7).expand()}
    other_reps = make(tiny, tmp_path, replications=marg.Replications(n=3)).expand()
    assert not base & other_seed
    assert base < {r["run_hash"] for r in other_reps}


def test_run_hashes_match_the_core(tiny, tmp_path: Path) -> None:
    exp = make(tiny, tmp_path, replications=marg.Replications(n=1))
    exp.run(marg.backends.Local(workers=2))
    for r in exp.run_list():
        m = json.loads((exp.dir / r["path"] / "manifest.json").read_text())
        assert m["status"] == "ok"
        assert m["run_hash"] == r["run_hash"]
        assert m["experiment"] == {
            "experiment": "exp",
            "run_id": r["run_id"],
            "point": r["point"],
            "arm": r["arm"],
            "rep": r["rep"],
        }


def test_tampered_run_list_fails_with_reason(tiny, tmp_path: Path) -> None:
    exp = make(tiny, tmp_path, replications=marg.Replications(n=1))
    exp.write()
    runs = exp.run_list()
    runs[0]["overrides"]["vehicles.free_flow_speed.hatchback"] = 50.0  # hash no longer matches
    (exp.dir / "runs.jsonl").write_text("".join(json.dumps(r) + "\n" for r in runs))
    with pytest.raises(RuntimeError):
        exp.run(marg.backends.Local(workers=1))
    idx = exp.data().index()
    row = idx.filter(idx["run_id"] == 0).row(0, named=True)
    assert row["status"] == "failed"
    assert "run hash mismatch" in row["error"]


def test_folder_holds_one_experiment(tiny, tmp_path: Path) -> None:
    make(tiny, tmp_path).write()
    make(tiny, tmp_path).write()  # same definition: reopened
    with pytest.raises(FileExistsError):
        make(tiny, tmp_path, seed=9).write()


def test_invalid_overrides_fail_at_expansion(tiny, tmp_path: Path) -> None:
    with pytest.raises(ValueError, match="fix"):
        make(tiny, tmp_path, arms={"a": {"vehicles.length.hatchback": 4.0}}).expand()
    with pytest.raises(ValueError, match="not in the register"):
        make(tiny, tmp_path, arms={"a": {"no.such.path": 1.0}}).expand()
