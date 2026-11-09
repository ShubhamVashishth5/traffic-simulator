"""An experiment folder as one queryable dataset (plan P3.6).

``index.parquet`` (written by ``marg collect``) lists every run with its
status; ``design.parquet`` gives each point's factor values. A table query
selects runs through the index first, then reads only their files, and returns
one frame with the run's ``scenario``, ``point``, ``arm``, ``rep``, ``seed``
and factor values as columns.
"""

from __future__ import annotations

import json
from collections.abc import Mapping, Sequence
from functools import cached_property
from pathlib import Path
from typing import Any

import polars as pl
import yaml

from marg import _core

RUN_KEYS = ("run_id", "run_hash", "scenario", "point", "arm", "rep", "seed")


class ExperimentData:
    """Read access to an experiment folder."""

    def __init__(self, path: str | Path) -> None:
        self.dir = Path(path)
        if not (self.dir / "runs.jsonl").exists():
            raise FileNotFoundError(f"{self.dir} is not an experiment folder (no runs.jsonl)")

    @cached_property
    def definition(self) -> dict[str, Any]:
        f = self.dir / "experiment.yaml"
        return yaml.safe_load(f.read_text()) if f.exists() else {}

    @property
    def name(self) -> str:
        return str(self.definition.get("name", self.dir.name))

    def collect(self, clean_partials: bool = False) -> dict[str, Any]:
        """Re-validate the run folders and rewrite ``index.parquet``."""
        self.__dict__.pop("_index", None)
        return json.loads(_core.collect(str(self.dir), clean_partials))

    @cached_property
    def _index(self) -> pl.DataFrame:
        f = self.dir / "index.parquet"
        if not f.exists():
            self.collect()
        return pl.read_parquet(f)

    def index(self) -> pl.DataFrame:
        """One row per run-list entry: identity, status, provenance."""
        return self._index

    def design(self) -> pl.DataFrame:
        """One row per design point: ``point`` and one column per factor."""
        f = self.dir / "design.parquet"
        return pl.read_parquet(f) if f.exists() else pl.DataFrame({"point": [0]})

    @property
    def factors(self) -> list[str]:
        return [c for c in self.design().columns if c != "point"]

    def problem(self) -> dict[str, Any]:
        """The SALib problem of the design (factor names and bounds)."""
        fs = self.definition["design"]["factors"]
        return {
            "num_vars": len(fs),
            "names": [f["path"] for f in fs],
            "bounds": [[f["lo"], f["hi"]] for f in fs],
        }

    def runs(
        self, status: str | Sequence[str] | None = "ok", filters: Mapping[str, Any] | None = None
    ) -> pl.DataFrame:
        """The index joined with the design, filtered by status and by
        ``filters`` (``{column: value or [values]}``, on any index or factor column)."""
        df = self._index
        if status is not None:
            df = df.filter(pl.col("status").is_in([status] if isinstance(status, str) else status))
        df = df.join(self.design(), on="point", how="left")
        for col, v in (filters or {}).items():
            if col not in df.columns:
                raise KeyError(f"cannot filter on `{col}`; columns are {df.columns}")
            vals = list(v) if isinstance(v, list | tuple | set) else [v]
            df = df.filter(pl.col(col).is_in(vals))
        return df

    def files(self, table: str, runs: pl.DataFrame | None = None) -> list[str]:
        runs = self.runs() if runs is None else runs
        return [str(self.dir / p / f"{table}.parquet") for p in runs["path"]]

    def scan(
        self,
        table: str,
        filters: Mapping[str, Any] | None = None,
        status: str | Sequence[str] | None = "ok",
        with_factors: bool = True,
    ) -> pl.LazyFrame:
        """Lazily scan one output table over the selected runs."""
        runs = self.runs(status, filters)
        if runs.is_empty():
            raise LookupError(f"no runs match status={status!r} filters={filters!r}")
        files = self.files(table, runs)
        keys = list(RUN_KEYS) + (self.factors if with_factors else [])
        meta = runs.select(keys).with_columns(pl.Series("__file", files))
        lf = pl.scan_parquet(files, include_file_paths="__file", missing_columns="insert")
        drop = [c for c in ("run_hash", "seed") if c in lf.collect_schema().names()]
        return meta.lazy().join(lf.drop(drop), on="__file", how="inner").drop("__file")

    def results(
        self,
        table: str = "kpis",
        filters: Mapping[str, Any] | None = None,
        columns: Sequence[str] | None = None,
        status: str | Sequence[str] | None = "ok",
        with_factors: bool = True,
    ) -> pl.DataFrame:
        """One output table over every selected run, with run keys and factors."""
        lf = self.scan(table, filters, status, with_factors)
        if columns is not None:
            keys = list(RUN_KEYS) + (self.factors if with_factors else [])
            lf = lf.select([*keys, *[c for c in columns if c not in keys]])
        return lf.collect()

    def duckdb(self, tables: Sequence[str] = ("kpis",), con: Any = None) -> Any:
        """A DuckDB connection with a ``runs`` view (ok runs: index ⋈ design)
        and one view per table, joined with its run's keys and factors.

        Each table view reads the run folders through a Hive-partitioned glob
        (``scenario=*/point=*/arm=*/rep=*/run=*``) and keeps only the files of
        ``ok`` runs in the current run list."""
        import duckdb

        con = con or duckdb.connect()
        con.register("runs", self.runs().to_arrow())
        hive = ("scenario", "point", "arm", "rep", "run", "run_hash", "seed")
        for t in tables:
            pattern = str(self.dir / "scenario=*/point=*/arm=*/rep=*/run=*" / f"{t}.parquet")
            if not any(self.dir.glob(f"scenario=*/point=*/arm=*/rep=*/run=*/{t}.parquet")):
                continue
            cols = con.sql(
                f"DESCRIBE SELECT * FROM read_parquet('{pattern}', hive_partitioning = true)"
            )
            names = [r[0] for r in cols.fetchall()]
            drop = ", ".join(c for c in hive if c in names)
            con.execute(
                f"""CREATE OR REPLACE VIEW {t} AS
                SELECT r.*, x.* EXCLUDE ({drop})
                FROM read_parquet('{pattern}', hive_partitioning = true) x
                JOIN runs r ON r.run_hash = x.run"""
            )
        return con


def open_experiment(path: str | Path) -> ExperimentData:
    """Open an experiment folder for analysis."""
    return ExperimentData(path)
