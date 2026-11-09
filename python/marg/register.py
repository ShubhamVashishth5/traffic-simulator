"""The parameter register from Python: entries, roles, ranges and run hashes.

The register itself is loaded and validated by the Rust core (``marg._core``),
so Python and the simulator can never disagree about what a path means.
"""

from __future__ import annotations

import json
import os
from collections.abc import Iterator, Mapping
from dataclasses import dataclass
from functools import cached_property
from pathlib import Path
from typing import Any

from marg import _core


def find_params_dir(start: str | Path | None = None) -> Path:
    """``$MARG_PARAMS_DIR``, else the nearest ``params/base`` at or above ``start``."""
    env = os.environ.get("MARG_PARAMS_DIR")
    if env:
        return Path(env)
    d = Path(start or Path.cwd()).resolve()
    for p in (d, *d.parents):
        if (p / "params/base").is_dir():
            return p / "params/base"
    raise FileNotFoundError("no parameter register: set MARG_PARAMS_DIR or run inside the repo")


@dataclass(frozen=True)
class Entry:
    """One register entry (values in the entry's own unit)."""

    path: str
    value: Any
    unit: str
    grade: str
    role: str
    stage: int | None
    range: tuple[float, float] | None
    source: str
    guide: str
    notes: str

    @property
    def is_dist(self) -> bool:
        return isinstance(self.value, dict)

    @property
    def mean(self) -> float | None:
        """Mean of the value in the register unit (None if unset)."""
        if self.value is None:
            return None
        return value_mean(self.value)


def value_mean(value: Any) -> float:
    """Mean of a scalar or distribution value."""
    if isinstance(value, int | float):
        return float(value)
    return _core.value_mean(json.dumps(value))


def value_scaled(value: Any, factor: float) -> Any:
    """``value`` with every location and scale field multiplied by ``factor``."""
    return json.loads(_core.value_scaled(json.dumps(value), float(factor)))


class Register(Mapping[str, Entry]):
    """The register at ``params_dir`` (default: :func:`find_params_dir`), with
    the frozen calibration stages in ``calibration_dir`` applied to run hashes."""

    def __init__(
        self, params_dir: str | Path | None = None, calibration_dir: str | Path | None = None
    ) -> None:
        self.params_dir = Path(params_dir) if params_dir else find_params_dir()
        self.calibration_dir = Path(calibration_dir) if calibration_dir else None
        self._resolver = _core.Resolver(
            str(self.params_dir), None if calibration_dir is None else str(calibration_dir)
        )

    @cached_property
    def _entries(self) -> dict[str, Entry]:
        raw = json.loads(self._resolver.register_json())
        return {
            p: Entry(
                path=p,
                value=e.get("value"),
                unit=e["unit"],
                grade=e["grade"],
                role=e["role"],
                stage=e.get("stage"),
                range=tuple(e["range"]) if e.get("range") else None,
                source=e["source"],
                guide=e["guide"],
                notes=e.get("notes", ""),
            )
            for p, e in raw.items()
        }

    def __getitem__(self, path: str) -> Entry:
        try:
            return self._entries[path]
        except KeyError:
            raise KeyError(f"`{path}` is not in the register") from None

    def __iter__(self) -> Iterator[str]:
        return iter(self._entries)

    def __len__(self) -> int:
        return len(self._entries)

    def select(
        self,
        role: str | None = None,
        stage: int | None = None,
        prefix: str | tuple[str, ...] | None = None,
        has_value: bool | None = None,
    ) -> list[str]:
        """Paths matching every given filter, in register order."""
        out = []
        for p, e in self._entries.items():
            if role is not None and e.role != role:
                continue
            if stage is not None and e.stage != stage:
                continue
            if prefix is not None and not p.startswith(prefix):
                continue
            if has_value is not None and (e.value is not None) != has_value:
                continue
            out.append(p)
        return out

    @property
    def calibration_stages(self) -> list[int]:
        return list(self._resolver.calibration_stages())

    def run_hash(
        self,
        scenario_yaml: str,
        overrides: Mapping[str, Any],
        seed: int,
        allow_fix_override: bool = False,
    ) -> str:
        """The run's identity, exactly as the core computes it. Raises
        ``ValueError`` if the overrides are invalid (unknown, out of range, fix)."""
        return self._resolver.run_hash(
            scenario_yaml, json.dumps(dict(overrides)), int(seed), allow_fix_override
        )

    def resolve(
        self, scenario_yaml: str, overrides: Mapping[str, Any] | None = None
    ) -> dict[str, Any]:
        """Resolved parameters (SI value, provenance) by path."""
        return json.loads(self._resolver.resolve_json(scenario_yaml, json.dumps(overrides or {})))
