"""Where runs execute (plan P3.5).

Every backend runs entries of an experiment's ``runs.jsonl`` through
``marg batch``, which skips runs that already have an ``ok`` manifest with the
same hash and writes each run folder atomically. So any backend can be stopped
and restarted, and a resubmitted sweep only runs what is missing.

- :class:`Local`: a pool of ``marg batch`` processes on this machine.
- :class:`Slurm`: an ``sbatch`` array over chunks of the run list, then
  ``marg collect``; optionally inside an Apptainer image.
- :class:`Function`: a Python function stands in for the simulator (tests,
  analytic benchmarks such as Ishigami).
"""

from __future__ import annotations

import json
import math
import os
import re
import shlex
import shutil
import subprocess
import sys
import time
from collections.abc import Callable, Sequence
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

import polars as pl


class Backend:
    """Runs the not-yet-finished entries of a run list."""

    def run(self, exp_dir: Path, entries: list[dict[str, Any]]) -> dict[str, int]:
        raise NotImplementedError


def pending(exp_dir: Path, entries: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Entries without an ``ok`` manifest of the same hash."""
    out = []
    for e in entries:
        m = Path(exp_dir) / e["path"] / "manifest.json"
        try:
            j = json.loads(m.read_text())
            if j.get("status") == "ok" and j.get("run_hash") == e["run_hash"]:
                continue
        except (OSError, ValueError):
            pass
        out.append(e)
    return out


def find_marg_binary() -> list[str]:
    """The command that runs the ``marg`` CLI: ``$MARG_BIN``, ``marg`` on the
    PATH, the repository's release build, else ``python -m marg`` (same core)."""
    env = os.environ.get("MARG_BIN")
    if env:
        return shlex.split(env)
    on_path = shutil.which("marg")
    if on_path:
        return [on_path]
    here = Path(__file__).resolve()
    for p in here.parents:
        cand = p / "target" / "release" / "marg"
        if cand.is_file() and os.access(cand, os.X_OK):
            return [str(cand)]
    return [sys.executable, "-m", "marg"]


@dataclass
class Local(Backend):
    """A local process pool: ``workers`` concurrent ``marg batch`` processes
    (default: all cores), each given ``chunk`` runs at a time."""

    workers: int | None = None
    chunk: int | None = None
    binary: Sequence[str] | None = None
    threads_per_run: int = 1
    log: Callable[[str], None] | None = None

    def run(self, exp_dir: Path, entries: list[dict[str, Any]]) -> dict[str, int]:
        todo = pending(exp_dir, entries)
        workers = self.workers or os.cpu_count() or 1
        summary = {"selected": len(entries), "ran": 0, "skipped": len(entries) - len(todo)}
        summary |= {"failed": 0, "raced": 0}
        if not todo:
            return summary
        size = self.chunk or max(1, math.ceil(len(todo) / (workers * 4)))
        chunks = [todo[i : i + size] for i in range(0, len(todo), size)]
        cmd = list(self.binary or find_marg_binary())
        errors: list[str] = []

        def one(chunk: list[dict[str, Any]]) -> dict[str, int]:
            ids = ",".join(str(e["run_id"]) for e in chunk)
            args = cmd + ["batch", str(exp_dir), "--ids", ids]
            args += ["--threads", str(self.threads_per_run)]
            p = subprocess.run(args, capture_output=True, text=True)
            out = _last_json(p.stdout)
            if p.returncode != 0 and out is None:
                errors.append(p.stderr.strip()[-2000:])
            return out or {"failed": len(chunk)}

        with ThreadPoolExecutor(max_workers=workers) as pool:
            for i, s in enumerate(pool.map(one, chunks)):
                for k in ("ran", "failed", "raced"):
                    summary[k] += int(s.get(k, 0))
                summary["skipped"] += int(s.get("skipped", 0))
                if self.log:
                    self.log(f"chunk {i + 1}/{len(chunks)}: {s}")
        if errors:
            raise RuntimeError("marg batch failed:\n" + "\n---\n".join(errors[:3]))
        return summary


def _last_json(text: str) -> dict[str, Any] | None:
    for line in reversed(text.strip().splitlines()):
        try:
            v = json.loads(line)
            if isinstance(v, dict):
                return v
        except ValueError:
            continue
    return None


@dataclass
class Slurm(Backend):
    """A SLURM job array over chunks of the run list.

    Writes ``slurm/array.sh`` (task ``k`` runs chunk ``k`` of ``chunk`` runs
    with ``marg batch --chunk k --chunk-size chunk``) and ``slurm/collect.sh``
    (``marg collect``, submitted with ``afterany`` on the array). Chunks cover
    the whole run list, so resubmitting after a cancellation re-runs only the
    unfinished runs, and runs land in their folders exactly once.

    ``submit=False`` writes the scripts only (dry run). ``container`` runs every
    command through ``apptainer exec <image>``. ``sbatch`` is the submit command
    (a test harness can stand in for it).
    """

    partition: str | None = None
    time: str = "01:00:00"
    cpus: int = 1
    chunk: int = 50
    account: str | None = None
    max_concurrent: int | None = None
    mem: str | None = "2G"
    binary: str | Sequence[str] = "marg"
    container: str | None = None
    extra: list[str] = field(default_factory=list)
    submit: bool = True
    sbatch: Sequence[str] = ("sbatch",)
    wait: bool = False
    poll_s: float = 30.0
    job_ids: list[str] = field(default_factory=list)

    def _cmd(self) -> str:
        parts = [self.binary] if isinstance(self.binary, str) else list(self.binary)
        base = " ".join(shlex.quote(p) for p in parts)
        if self.container:
            return f"apptainer exec {shlex.quote(self.container)} {base}"
        return base

    def scripts(self, exp_dir: Path, n_entries: int) -> tuple[Path, Path]:
        """Write the array and collect scripts; return their paths."""
        exp_dir = Path(exp_dir).resolve()
        sdir = exp_dir / "slurm"
        sdir.mkdir(parents=True, exist_ok=True)
        n_chunks = max(1, math.ceil(n_entries / self.chunk))
        throttle = f"%{self.max_concurrent}" if self.max_concurrent else ""
        head = ["#!/bin/bash"]
        opts = {
            "partition": self.partition,
            "account": self.account,
            "time": self.time,
            "cpus-per-task": str(self.cpus),
            "mem": self.mem,
        }
        common = [f"#SBATCH --{k}={v}" for k, v in opts.items() if v]
        common += [f"#SBATCH {x}" for x in self.extra]
        exp = shlex.quote(str(exp_dir))
        array = head + [
            f"#SBATCH --job-name=marg-{exp_dir.name}",
            f"#SBATCH --array=0-{n_chunks - 1}{throttle}",
            f"#SBATCH --output={sdir}/array-%A_%a.out",
            *common,
            "set -euo pipefail",
            f"# {n_entries} runs in {n_chunks} chunks of {self.chunk}; finished runs are skipped.",
            f'{self._cmd()} batch {exp} --chunk "$SLURM_ARRAY_TASK_ID" '
            f"--chunk-size {self.chunk} --threads {self.cpus}",
            "",
        ]
        collect = head + [
            f"#SBATCH --job-name=marg-{exp_dir.name}-collect",
            f"#SBATCH --output={sdir}/collect-%j.out",
            "#SBATCH --time=00:30:00",
            *[c for c in common if not c.startswith(("#SBATCH --time", "#SBATCH --cpus"))],
            "set -euo pipefail",
            f"{self._cmd()} collect {exp} --allow-incomplete",
            "",
        ]
        a, c = sdir / "array.sh", sdir / "collect.sh"
        a.write_text("\n".join(array))
        c.write_text("\n".join(collect))
        a.chmod(0o755)
        c.chmod(0o755)
        return a, c

    def _submit(self, args: list[str]) -> str:
        p = subprocess.run(list(self.sbatch) + args, capture_output=True, text=True)
        if p.returncode != 0:
            raise RuntimeError(f"sbatch failed: {p.stderr.strip()}")
        m = re.search(r"(\d+)", p.stdout)
        if not m:
            raise RuntimeError(f"cannot read a job id from sbatch output {p.stdout!r}")
        return m.group(1)

    def run(self, exp_dir: Path, entries: list[dict[str, Any]]) -> dict[str, int]:
        todo = pending(exp_dir, entries)
        array, collect = self.scripts(exp_dir, len(entries))
        summary = {"selected": len(entries), "pending": len(todo), "submitted": 0}
        if not self.submit or not todo:
            return summary
        job = self._submit([str(array)])
        self._submit([f"--dependency=afterany:{job}", str(collect)])
        self.job_ids.append(job)
        summary["submitted"] = 1
        if self.wait:
            while _squeue_has(job):
                time.sleep(self.poll_s)
        return summary


def _squeue_has(job: str) -> bool:
    try:
        p = subprocess.run(["squeue", "-h", "-j", job], capture_output=True, text=True)
    except FileNotFoundError:
        return False
    return p.returncode == 0 and bool(p.stdout.strip())


@dataclass
class Function(Backend):
    """A Python function stands in for the simulator: ``fn(entry) -> {kpi: value}``.

    ``entry`` is the run-list entry (``factors``, ``seed``, ``arm``, ``rep``,
    ``overrides``, …). Each run folder gets ``kpis.parquet`` and an ``ok``
    manifest, written atomically, so the rest of the pipeline (collect, results,
    sensitivity) is exercised unchanged.
    """

    fn: Callable[[dict[str, Any]], dict[str, float]]
    workers: int = 1

    def run(self, exp_dir: Path, entries: list[dict[str, Any]]) -> dict[str, int]:
        todo = pending(exp_dir, entries)

        def one(e: dict[str, Any]) -> None:
            final = Path(exp_dir) / e["path"]
            tmp = final.with_name(final.name + f".partial-fn-{os.getpid()}")
            shutil.rmtree(tmp, ignore_errors=True)
            tmp.mkdir(parents=True)
            t0 = time.perf_counter()
            k = self.fn(e)
            row = {"run_hash": [e["run_hash"]], "seed": [int(e["seed"])]}
            row |= {name: [float(v)] for name, v in k.items()}
            pl.DataFrame(row).write_parquet(tmp / "kpis.parquet")
            manifest = {
                "run_format_version": 2,
                "run_hash": e["run_hash"],
                "scenario": e["scenario"],
                "seed": e["seed"],
                "rng_scheme": "function",
                "marg_version": "function",
                "git_sha": "",
                "git_dirty": False,
                "build_profile": "function",
                "threads": 1,
                "host": os.uname().nodename,
                "wall_time_s": time.perf_counter() - t0,
                "status": "ok",
                "output_hash": "",
                "steps": 0,
                "agent_steps": 0,
                "experiment": {
                    "experiment": e["experiment"],
                    "run_id": e["run_id"],
                    "point": e["point"],
                    "arm": e["arm"],
                    "rep": e["rep"],
                },
            }
            (tmp / "manifest.json").write_text(json.dumps(manifest, indent=1) + "\n")
            shutil.rmtree(final, ignore_errors=True)
            tmp.rename(final)

        if self.workers > 1:
            with ThreadPoolExecutor(max_workers=self.workers) as pool:
                list(pool.map(one, todo))
        else:
            for e in todo:
                one(e)
        return {"selected": len(entries), "ran": len(todo), "skipped": len(entries) - len(todo)}
