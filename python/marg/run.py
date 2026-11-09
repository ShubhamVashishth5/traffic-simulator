"""Run a single scenario from Python and read its outputs."""

from __future__ import annotations

import json
from dataclasses import dataclass
from functools import cached_property
from pathlib import Path
from typing import Any

import polars as pl

from marg import _core

TABLES = ("params", "agents", "detectors", "events", "kpis", "trajectories")


@dataclass(frozen=True)
class RunResult:
    """A finished run: its folder, manifest and output tables (read lazily)."""

    path: Path
    manifest: dict[str, Any]

    @classmethod
    def open(cls, path: str | Path) -> RunResult:
        """Open an existing run folder."""
        p = Path(path)
        return cls(path=p, manifest=json.loads((p / "manifest.json").read_text()))

    def table(self, name: str) -> pl.DataFrame:
        """Read one output table (see docs/data-dictionary.md)."""
        if name not in TABLES:
            raise ValueError(f"unknown table {name!r}; expected one of {TABLES}")
        f = self.path / f"{name}.parquet"
        if not f.exists():
            raise FileNotFoundError(f"{f} (trajectories are written only if the scenario asks)")
        return pl.read_parquet(f)

    def scan(self, name: str) -> pl.LazyFrame:
        """Lazily scan a table, e.g. large trajectories."""
        return pl.scan_parquet(self.path / f"{name}.parquet")

    @cached_property
    def kpis(self) -> dict[str, float]:
        row = self.table("kpis").row(0, named=True)
        return {k: v for k, v in row.items() if k not in ("run_hash", "seed")}

    @property
    def agents(self) -> pl.DataFrame:
        return self.table("agents")

    @property
    def detectors(self) -> pl.DataFrame:
        return self.table("detectors")

    @property
    def params(self) -> pl.DataFrame:
        return self.table("params")


def run(
    scenario: str | Path,
    seed: int = 1,
    out: str | Path = "runs/adhoc",
    threads: int = 1,
    params_dir: str | Path | None = None,
) -> RunResult:
    """Run ``scenario`` (a YAML file path or YAML text) and write its run folder.

    ``params_dir`` is the parameter register directory; by default
    ``$MARG_PARAMS_DIR`` or the nearest ``params/base`` above the working directory.
    """
    text = Path(scenario).read_text() if _looks_like_path(scenario) else str(scenario)
    out_path = Path(out)
    pdir = None if params_dir is None else str(params_dir)
    manifest = json.loads(_core.run_yaml(text, seed, str(out_path), threads, pdir))
    return RunResult(path=out_path, manifest=manifest)


def _looks_like_path(s: str | Path) -> bool:
    if isinstance(s, Path):
        return True
    return "\n" not in s and s.endswith((".yaml", ".yml"))
