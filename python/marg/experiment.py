"""Experiments: designs × arms × replications, expanded into a run list
(plan P3.1, P3.3, P3.4; plan/04-data-outputs-analytics.md §1, §5).

An experiment folder ``<root>/<name>/`` holds:

- ``experiment.yaml``: the definition (deterministic: no timestamps);
- ``runs.jsonl``: the run list, one run per line, with run hashes;
- ``design.parquet``: the design matrix, one row per point;
- ``params/``: a snapshot of the register the run hashes were computed with;
- ``scenarios/``: the scenario documents the runs use;
- ``calibration/``: the calibration stages applied, if any;
- ``replications.json``: why each point stopped adding replications;
- run folders ``scenario=<s>/point=<p>/arm=<a>/rep=<r>/run=<hash>/``;
- ``index.parquet``: written by ``marg collect``.

**Common random numbers.** With ``crn="all"`` (default) every run of
replication ``r`` uses seed ``seed + r``, whatever its point and arm, so the
same agent gets the same draws everywhere (ADR-0003). ``crn="arms"`` shares
seeds only between the arms of one point; ``crn="none"`` makes every run
independent (to measure what CRN buys).
"""

from __future__ import annotations

import hashlib
import json
import math
import shutil
from collections.abc import Mapping
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING, Any

import polars as pl
import yaml

from marg import _core
from marg.design import BoundFactor, Design, Single, plain, resolve_arm_value
from marg.register import Register
from marg.scenario import Scenario, split_changes

if TYPE_CHECKING:
    from marg.backends import Backend
    from marg.results import ExperimentData

_SAFE = set("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789_-.")


@dataclass(frozen=True)
class Replications:
    """Fixed (``n``) or sequential replications.

    Sequential: start with ``min``, then add ``step`` replications to every
    point whose 95% CI half-width of ``metric`` (a ``kpis`` column) is above
    ``ci_halfwidth`` (relative to the mean if ``relative``), for any of its
    arms, until it is met or ``max`` is reached. Arms of one point always have
    the same replications, so they stay paired.
    """

    n: int | None = None
    min: int = 3
    max: int = 30
    ci_halfwidth: float | None = None
    metric: str | None = None
    relative: bool = True
    confidence: float = 0.95
    step: int = 1

    def __post_init__(self) -> None:
        if self.n is not None:
            if self.n < 1:
                raise ValueError("replications n must be at least 1")
            return
        if self.ci_halfwidth is None or self.metric is None:
            raise ValueError("give n=…, or ci_halfwidth=… and metric=… for sequential replications")
        if not (2 <= self.min <= self.max):
            raise ValueError("sequential replications need 2 <= min <= max")
        if self.ci_halfwidth <= 0 or self.step < 1:
            raise ValueError("ci_halfwidth must be positive and step at least 1")

    @property
    def sequential(self) -> bool:
        return self.n is None

    @property
    def initial(self) -> int:
        return self.n if self.n is not None else self.min


def ci_halfwidth(values: list[float], confidence: float = 0.95) -> float:
    """Student-t confidence-interval half-width of the mean (inf below 2 values)."""
    from scipy import stats

    n = len(values)
    if n < 2:
        return math.inf
    mean = sum(values) / n
    var = sum((v - mean) ** 2 for v in values) / (n - 1)
    return float(stats.t.ppf(0.5 + confidence / 2, n - 1) * math.sqrt(var / n))


def stopping_decision(values: list[float], rule: Replications) -> dict[str, Any]:
    """Whether a sequential rule stops on ``values`` (one metric, one arm)."""
    n = len(values)
    finite = [v for v in values if math.isfinite(v)]
    h = ci_halfwidth(finite, rule.confidence) if len(finite) == n else math.inf
    mean = sum(finite) / len(finite) if finite else math.nan
    assert rule.ci_halfwidth is not None
    target = rule.ci_halfwidth * abs(mean) if rule.relative else rule.ci_halfwidth
    if n >= rule.min and h <= target:
        reason = "ci_target_met"
    elif n >= rule.max:
        reason = "max_reached"
    else:
        reason = "continue"
    return {"n": n, "mean": mean, "halfwidth": h, "target": target, "reason": reason}


@dataclass(frozen=True)
class Outputs:
    """Which optional outputs every run writes. Sweeps default to no
    trajectories (plan 04 §2: every KPI comes from online observers)."""

    trajectories: dict[str, Any] | None = None

    def changes(self) -> dict[str, Any]:
        return {"scenario.outputs.trajectories": self.trajectories}


@dataclass
class Experiment:
    """A designed experiment over one or more base scenarios.

    ``arms`` maps arm names to override sets (register paths in register units,
    or ``scenario.`` paths); every arm runs on the same seeds. Default: one arm
    ``base`` with no overrides.
    """

    name: str
    base: Scenario | str | Path | list[Scenario | str | Path]
    design: Design = field(default_factory=Single)
    arms: Mapping[str, Mapping[str, Any]] | None = None
    replications: Replications = field(default_factory=lambda: Replications(n=1))
    outputs: Outputs | None = field(default_factory=Outputs)
    seed: int = 1
    root: str | Path = "runs"
    params_dir: str | Path | None = None
    calibration_dir: str | Path | None = None
    crn: str = "all"
    allow_fix_override: bool = False

    def __post_init__(self) -> None:
        if not self.name or not set(self.name) <= _SAFE:
            raise ValueError(f"experiment name {self.name!r} must be [A-Za-z0-9_.-]+")
        bases = self.base if isinstance(self.base, list) else [self.base]
        self.scenarios: list[Scenario] = [
            b if isinstance(b, Scenario) else Scenario.load(b) for b in bases
        ]
        names = [s.name for s in self.scenarios]
        if len(set(names)) != len(names):
            raise ValueError(f"base scenarios must have distinct names, got {names}")
        for n in names:
            if not set(n) <= _SAFE:
                raise ValueError(f"scenario name {n!r} must be [A-Za-z0-9_.-]+")
        self.arms = dict(self.arms) if self.arms else {"base": {}}
        for a in self.arms:
            if not a or not set(a) <= _SAFE:
                raise ValueError(f"arm name {a!r} must be [A-Za-z0-9_.-]+")
        if self.crn not in ("all", "arms", "none"):
            raise ValueError("crn must be 'all', 'arms' or 'none'")
        self.register = Register(self.params_dir, self.calibration_dir)
        self.factors: list[BoundFactor] = self.design.bind(self.register)
        self.points = self.design.matrix(self.factors)

    # ----- identity -----

    @property
    def dir(self) -> Path:
        return Path(self.root) / self.name

    def definition(self) -> dict[str, Any]:
        """Everything that determines the run list (stored as experiment.yaml)."""
        return {
            "name": self.name,
            "marg_version": _core.version(),
            "scenarios": {s.name: s.digest() for s in self.scenarios},
            "design": self.design.describe(self.factors),
            "points": len(self.points),
            "arms": {a: {k: plain(v) for k, v in o.items()} for a, o in self.arms.items()},
            "replications": asdict(self.replications),
            "outputs": None if self.outputs is None else asdict(self.outputs),
            "seed": self.seed,
            "crn": self.crn,
            "allow_fix_override": self.allow_fix_override,
            "register": register_digest(self.register.params_dir),
            "calibration_stages": self.register.calibration_stages,
        }

    def definition_hash(self) -> str:
        canon = json.dumps(self.definition(), sort_keys=True, separators=(",", ":"), default=str)
        return hashlib.blake2b(canon.encode(), digest_size=8).hexdigest()

    # ----- expansion -----

    def seed_for(self, scenario: str, point: int, arm: str, rep: int) -> int:
        if self.crn == "all":
            return self.seed + rep
        key = f"{self.seed}/{scenario}/{point}/{rep}"
        if self.crn == "none":
            key += f"/{arm}"
        return int.from_bytes(hashlib.blake2b(key.encode(), digest_size=8).digest()) >> 1

    def _variant(self, scenario: Scenario, point: int, arm: str) -> tuple[Scenario, dict, dict]:
        """The scenario document, register overrides and factor values of one run."""
        factor_values = {
            b.path: float(x) for b, x in zip(self.factors, self.points[point], strict=True)
        }
        changes: dict[str, Any] = {}
        if self.outputs is not None:
            changes.update(self.outputs.changes())
        for b in self.factors:
            changes[b.path] = b.value(factor_values[b.path])
        changes.update(
            {k: resolve_arm_value(k, v, self.register) for k, v in self.arms[arm].items()}
        )
        scen_changes, overrides = split_changes(changes)
        return scenario.with_changes(scen_changes), overrides, factor_values

    def expand(self, reps: Mapping[tuple[str, int], int] | None = None) -> list[dict[str, Any]]:
        """The run list for ``reps[(scenario, point)]`` replications each
        (default: the initial number). Deterministic: identical experiments give
        identical run lists. Raises ``ValueError`` on an invalid run."""
        n0 = self.replications.initial
        want = {(s.name, p): n0 for s in self.scenarios for p in range(len(self.points))}
        if reps:
            want.update(reps)
        return self._entries(want, start={})

    def _entries(
        self, want: Mapping[tuple[str, int], int], start: Mapping[tuple[str, int], int]
    ) -> list[dict[str, Any]]:
        """Entries for replications ``start[key] .. want[key]-1``, rep-major."""
        cache: dict[tuple[str, int, str], tuple[Scenario, dict, dict, str]] = {}
        out: list[dict[str, Any]] = []
        top = max(want.values(), default=0)
        for rep in range(top):
            for s in self.scenarios:
                for p in range(len(self.points)):
                    key = (s.name, p)
                    if not (start.get(key, 0) <= rep < want.get(key, 0)):
                        continue
                    for arm in self.arms:
                        if (s.name, p, arm) not in cache:
                            doc, ov, fv = self._variant(s, p, arm)
                            cache[(s.name, p, arm)] = (doc, ov, fv, doc.to_yaml())
                        doc, ov, fv, text = cache[(s.name, p, arm)]
                        seed = self.seed_for(s.name, p, arm, rep)
                        try:
                            h = self.register.run_hash(text, ov, seed, self.allow_fix_override)
                        except ValueError as e:
                            raise ValueError(f"point {p}, arm {arm}: {e}") from None
                        out.append(
                            {
                                "run_id": -1,
                                "run_hash": h,
                                "experiment": self.name,
                                "scenario": s.name,
                                "scenario_file": f"scenarios/{_scenario_file(doc, s)}",
                                "point": p,
                                "arm": arm,
                                "rep": rep,
                                "seed": seed,
                                "overrides": ov,
                                "allow_fix_override": self.allow_fix_override,
                                "factors": fv,
                                "path": f"scenario={s.name}/point={p}/arm={arm}/rep={rep}/run={h}",
                            }
                        )
        return out

    # ----- folder -----

    def write(self) -> Path:
        """Create (or reopen) the experiment folder and write the initial run
        list. Refuses to reuse a folder holding a different experiment."""
        d = self.dir
        defn = self.definition()
        defn["definition_hash"] = self.definition_hash()
        ef = d / "experiment.yaml"
        if ef.exists():
            old = yaml.safe_load(ef.read_text()) or {}
            if old.get("definition_hash") != defn["definition_hash"]:
                raise FileExistsError(
                    f"{d} holds a different experiment (definition hash "
                    f"{old.get('definition_hash')} != {defn['definition_hash']}); "
                    "use another name or remove the folder"
                )
            if (d / "runs.jsonl").exists():
                return d
        d.mkdir(parents=True, exist_ok=True)
        _snapshot(self.register.params_dir, d / "params")
        if self.calibration_dir is not None:
            _snapshot(Path(self.calibration_dir), d / "calibration")
        entries = self.expand()
        self._write_scenarios(entries)
        _write_runs(d / "runs.jsonl", entries, append=False)
        self._write_design()
        ef.write_text(yaml.safe_dump(defn, sort_keys=False, allow_unicode=True))
        return d

    def _write_scenarios(self, entries: list[dict[str, Any]]) -> None:
        sdir = self.dir / "scenarios"
        sdir.mkdir(parents=True, exist_ok=True)
        done: set[str] = set()
        for s in self.scenarios:
            for p in range(len(self.points)):
                for arm in self.arms:
                    doc, _, _ = self._variant(s, p, arm)
                    f = _scenario_file(doc, s)
                    if f not in done:
                        (sdir / f).write_text(doc.to_yaml())
                        done.add(f)

    def _write_design(self) -> None:
        cols: dict[str, Any] = {"point": list(range(len(self.points)))}
        for i, b in enumerate(self.factors):
            cols[b.path] = [float(x) for x in self.points[:, i]]
        pl.DataFrame(cols).write_parquet(self.dir / "design.parquet")

    def run_list(self) -> list[dict[str, Any]]:
        """The current run list (``runs.jsonl``)."""
        return read_runs(self.dir / "runs.jsonl")

    # ----- running -----

    def run(
        self,
        backend: Backend | None = None,
        max_rounds: int = 1000,
        allow_failures: bool = False,
    ) -> Experiment:
        """Write the folder, run every missing run, collect; with sequential
        replications, keep adding replications until every point stops.

        Raises ``RuntimeError`` if any run is not ``ok`` after a round (its
        reason is in ``index.parquet``), unless ``allow_failures``. A backend
        that only submits (SLURM without ``wait``) returns after submission;
        call :meth:`collect` once the jobs finish."""
        from marg.backends import Local

        backend = backend or Local()
        self.write()
        for _ in range(max_rounds):
            summary = backend.run(self.dir, self.run_list())
            if "submitted" in summary and not getattr(backend, "wait", False):
                return self  # asynchronous (or dry-run) backend: collect() when jobs finish
            report = self.collect()
            if report["ok"] < report["total"] and not allow_failures:
                idx = self.data().index()
                bad = idx.filter(idx["status"] != "ok").row(0, named=True)
                raise RuntimeError(
                    f"{report['total'] - report['ok']} of {report['total']} runs not ok; "
                    f"first: run {bad['run_id']} {bad['status']}: {bad['error']}"
                )
            if not self.replications.sequential or report["ok"] < report["total"]:
                break
            if not self._extend():
                break
        return self

    def collect(self, clean_partials: bool = False) -> dict[str, Any]:
        """Validate the run folders and write ``index.parquet``."""
        return json.loads(_core.collect(str(self.dir), clean_partials))

    def _extend(self) -> bool:
        """Apply the sequential rule; append replications. True if any added."""
        rule = self.replications
        assert rule.metric is not None
        k = self.data().results("kpis", columns=[rule.metric])
        decisions: dict[str, Any] = {}
        want: dict[tuple[str, int], int] = {}
        have: dict[tuple[str, int], int] = {}
        for s in self.scenarios:
            for p in range(len(self.points)):
                sub = k.filter((pl.col("scenario") == s.name) & (pl.col("point") == p))
                arms = {}
                for arm in self.arms:
                    vals = sub.filter(pl.col("arm") == arm).sort("rep")[rule.metric].to_list()
                    arms[arm] = stopping_decision([float(v) for v in vals], rule)
                n = max(a["n"] for a in arms.values())
                have[(s.name, p)] = n
                stop = all(a["reason"] != "continue" for a in arms.values())
                reason = (
                    "ci_target_met"
                    if all(a["reason"] == "ci_target_met" for a in arms.values())
                    else ("max_reached" if stop else "continue")
                )
                decisions[f"{s.name}/{p}"] = {"n": n, "reason": reason, "arms": arms}
                if not stop:
                    want[(s.name, p)] = min(n + rule.step, rule.max)
        (self.dir / "replications.json").write_text(
            json.dumps(decisions, indent=1, sort_keys=True, default=_json_num) + "\n"
        )
        if not want:
            return False
        new = self._entries(want, start=have)
        self._write_scenarios(new)
        _write_runs(self.dir / "runs.jsonl", new, append=True)
        return True

    # ----- results -----

    def data(self) -> ExperimentData:
        from marg.results import ExperimentData

        return ExperimentData(self.dir)

    def results(self, table: str = "kpis", filters: Mapping[str, Any] | None = None, **kw: Any):
        """One output table over every ok run, with the run's scenario, point,
        arm, rep, seed and factor values as columns (see ``ExperimentData``)."""
        return self.data().results(table, filters=filters, **kw)


def _json_num(x: Any) -> Any:
    return None if isinstance(x, float) and not math.isfinite(x) else str(x)


def _scenario_file(doc: Scenario, base: Scenario) -> str:
    return f"{base.name}.yaml" if doc.data == base.data else f"{base.name}-{doc.digest()}.yaml"


def _write_runs(path: Path, entries: list[dict[str, Any]], append: bool) -> None:
    start = len(read_runs(path)) if append and path.exists() else 0
    lines = []
    for i, e in enumerate(entries):
        e = dict(e, run_id=start + i)
        lines.append(json.dumps(e, sort_keys=True, separators=(",", ":")))
    with path.open("a" if append else "w") as f:
        for line in lines:
            f.write(line + "\n")


def read_runs(path: Path) -> list[dict[str, Any]]:
    """Read a ``runs.jsonl`` run list."""
    if not path.exists():
        return []
    return [json.loads(line) for line in path.read_text().splitlines() if line.strip()]


def register_digest(params_dir: Path) -> str:
    """Hash of the register files (content and names)."""
    h = hashlib.blake2b(digest_size=8)
    for f in sorted(Path(params_dir).glob("*.yaml")):
        h.update(f.name.encode())
        h.update(f.read_bytes())
    return h.hexdigest()


def _snapshot(src: Path, dst: Path) -> None:
    dst.mkdir(parents=True, exist_ok=True)
    for f in sorted(src.glob("*.yaml")):
        shutil.copyfile(f, dst / f.name)
