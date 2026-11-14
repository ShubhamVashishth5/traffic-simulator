#!/usr/bin/env python3
"""A stand-in for `sbatch` (plan P3.5: no cluster in CI).

Runs a job script synchronously on this machine, the way SLURM would: for a
`#SBATCH --array=A-B` script, each task runs with `SLURM_ARRAY_TASK_ID` and
`SLURM_ARRAY_JOB_ID` set. `--dependency=afterany:<id>` jobs run after the job
they depend on (here: immediately, since that job has finished).

To simulate `scancel` part-way through an array, set FAKE_SLURM_CANCEL_AFTER=k:
tasks 0..k-1 complete, task k is killed mid-chunk with SIGKILL, and later tasks
never start. With FAKE_SLURM_WATCH=<experiment dir>, the kill comes as soon as
task k has finished one run (so it dies while running the next one);
otherwise after FAKE_SLURM_KILL_AFTER_S seconds.

Prints `Submitted batch job <id>` like sbatch.
"""

from __future__ import annotations

import os
import re
import signal
import subprocess
import sys
import time
from pathlib import Path

STATE = Path(os.environ.get("FAKE_SLURM_STATE", "/tmp/fake-slurm-jobid"))


def next_job_id() -> int:
    n = int(STATE.read_text()) + 1 if STATE.exists() else 1000
    STATE.write_text(str(n))
    return n


def finished(exp_dir: str) -> int:
    """Finished runs under an experiment folder: run folders with a manifest. A run is written
    to a `run=<hash>.partial-*` sibling (which also matches `run=*`) and renamed into place when
    complete, so a partial folder already holding its manifest is not finished yet: killing the
    task then would leave the run partial."""
    return sum(
        1
        for m in Path(exp_dir).glob("scenario=*/point=*/arm=*/rep=*/run=*/manifest.json")
        if ".partial-" not in m.parent.name
    )


def main(argv: list[str]) -> int:
    script = Path([a for a in argv if not a.startswith("--")][-1])
    job = next_job_id()
    text = script.read_text()
    m = re.search(r"^#SBATCH --array=(\d+)-(\d+)", text, re.M)
    tasks = range(int(m.group(1)), int(m.group(2)) + 1) if m else [None]
    cancel_after = os.environ.get("FAKE_SLURM_CANCEL_AFTER")
    kill_after = float(os.environ.get("FAKE_SLURM_KILL_AFTER_S", "0.2"))
    for i, t in enumerate(tasks):
        env = dict(os.environ, SLURM_JOB_ID=str(job), SLURM_ARRAY_JOB_ID=str(job))
        env.pop("FAKE_SLURM_CANCEL_AFTER", None)
        if t is not None:
            env["SLURM_ARRAY_TASK_ID"] = str(t)
        log = script.parent / f"fake-{job}-{t}.out"
        with log.open("w") as out:
            p = subprocess.Popen(
                ["bash", str(script)],
                env=env,
                stdout=out,
                stderr=subprocess.STDOUT,
                start_new_session=True,
            )
            if cancel_after is not None and i == int(cancel_after):
                watch = os.environ.get("FAKE_SLURM_WATCH")
                if watch:
                    base = finished(watch)
                    while p.poll() is None and finished(watch) <= base:
                        time.sleep(0.002)
                else:
                    time.sleep(kill_after)
                if p.poll() is None:
                    os.killpg(p.pid, signal.SIGKILL)  # scancel: the task dies mid-chunk
                p.wait()
                break
            p.wait()
    print(f"Submitted batch job {job}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
