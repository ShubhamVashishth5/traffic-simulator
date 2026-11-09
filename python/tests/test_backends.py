"""P3.5: a 200-run experiment completes locally; on (simulated) SLURM it
survives a cancelled job and a resubmission with no duplicates."""

from __future__ import annotations

import sys
from pathlib import Path

import marg
import pytest
from conftest import PARAMS
from marg.backends import Slurm, find_marg_binary
from marg.design import Grid

HARNESS = Path(__file__).parent / "harness" / "fake_sbatch.py"


def run_dirs(exp: marg.Experiment) -> list[Path]:
    return sorted(exp.dir.glob("scenario=*/point=*/arm=*/rep=*/run=*"))


def test_200_runs_locally(tiny, tmp_path: Path) -> None:
    exp = marg.Experiment(
        "local200",
        tiny,
        design=Grid({"vehicles.free_flow_speed.hatchback": [40.0 + 2 * i for i in range(10)]}),
        arms={"observed": {}, "cooperative": marg.bundles.cooperative()},
        replications=marg.Replications(n=10),
        root=tmp_path,
        params_dir=PARAMS,
    )
    exp.run(marg.backends.Local(workers=4))
    report = exp.collect()
    assert (report["total"], report["ok"]) == (200, 200)
    assert exp.results("kpis").height == 200
    assert len(run_dirs(exp)) == 200
    # Running again does nothing: every run is skipped by hash.
    s = marg.backends.Local(workers=4).run(exp.dir, exp.run_list())
    assert (s["ran"], s["skipped"]) == (0, 200)


def test_slurm_dry_run_scripts(tiny, tmp_path: Path) -> None:
    exp = marg.Experiment(
        "dry", tiny, replications=marg.Replications(n=23), root=tmp_path, params_dir=PARAMS
    )
    backend = Slurm(
        partition="short", time="00:20:00", cpus=2, chunk=5, submit=False, container="marg.sif"
    )
    exp.run(backend)
    array = (exp.dir / "slurm" / "array.sh").read_text()
    assert "#SBATCH --array=0-4" in array  # 23 runs in chunks of 5
    assert "#SBATCH --partition=short" in array and "#SBATCH --time=00:20:00" in array
    assert "apptainer exec marg.sif marg batch" in array
    assert '--chunk "$SLURM_ARRAY_TASK_ID" --chunk-size 5' in array
    assert "collect" in (exp.dir / "slurm" / "collect.sh").read_text()
    assert not run_dirs(exp)  # nothing ran


@pytest.mark.parametrize("cancel_after", ["1", "3"])
def test_slurm_survives_cancel_and_resubmit(
    tiny, tmp_path: Path, monkeypatch: pytest.MonkeyPatch, cancel_after: str
) -> None:
    slow = tiny.with_changes({"duration_s": 2000})  # ~0.1 s per run
    exp = marg.Experiment(
        "slurm",
        slow,
        design=Grid({"vehicles.free_flow_speed.hatchback": [45.0, 50.0, 55.0, 60.0]}),
        replications=marg.Replications(n=4),
        root=tmp_path,
        params_dir=PARAMS,
    )
    monkeypatch.setenv("FAKE_SLURM_STATE", str(tmp_path / "jobid"))
    monkeypatch.setenv("FAKE_SLURM_CANCEL_AFTER", cancel_after)
    monkeypatch.setenv("FAKE_SLURM_WATCH", str(exp.dir))  # kill once the task finished a run
    backend = Slurm(chunk=4, binary=find_marg_binary(), sbatch=[sys.executable, str(HARNESS)])
    exp.run(backend)  # submits; the job is cancelled part-way
    first = exp.collect()
    assert 0 < first["ok"] < first["total"] == 16
    assert (
        first["ok"] == 4 * int(cancel_after) + 1
    )  # earlier chunks, plus one run of the killed one
    monkeypatch.delenv("FAKE_SLURM_CANCEL_AFTER")
    exp.run(backend)  # resubmit the same experiment
    final = exp.collect(clean_partials=True)
    assert final["ok"] == final["total"] == 16, final
    assert not final["duplicate_paths"] and not final["orphans"]
    dirs = run_dirs(exp)
    assert len(dirs) == 16 and not [d for d in dirs if ".partial-" in d.name]
    hashes = {r["run_hash"] for r in exp.run_list()}
    assert {d.name.removeprefix("run=") for d in dirs} == hashes
    assert len(backend.job_ids) == 2
