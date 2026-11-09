"""Scenarios as data: load, patch by dotted path, and serialise."""

from __future__ import annotations

import copy
import hashlib
import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

import yaml

SCENARIO_PREFIX = "scenario."


@dataclass(frozen=True)
class Scenario:
    """A scenario document (see ``schema/scenario.schema.json``)."""

    data: dict[str, Any]

    @classmethod
    def load(cls, path: str | Path) -> Scenario:
        return cls.from_yaml(Path(path).read_text())

    @classmethod
    def from_yaml(cls, text: str) -> Scenario:
        data = yaml.safe_load(text)
        if not isinstance(data, dict) or "name" not in data:
            raise ValueError("a scenario is a YAML mapping with at least `name`")
        return cls(data)

    @property
    def name(self) -> str:
        return str(self.data["name"])

    def to_yaml(self) -> str:
        return yaml.safe_dump(self.data, sort_keys=False, allow_unicode=True)

    def digest(self) -> str:
        """Short hash of the scenario content (key order independent)."""
        canon = json.dumps(self.data, sort_keys=True, separators=(",", ":"))
        return hashlib.blake2b(canon.encode(), digest_size=8).hexdigest()

    def get(self, path: str) -> Any:
        node: Any = self.data
        for key in _split(path):
            node = node[key]
        return node

    def with_changes(self, changes: dict[str, Any]) -> Scenario:
        """A copy with values set at dotted paths, e.g.
        ``{"demand.entries.0.flow.0.veh_per_h": 4500, "outputs.trajectories": None}``.
        Integer path parts index lists. The ``scenario.`` prefix is optional."""
        if not changes:
            return self
        data = copy.deepcopy(self.data)
        for path, value in changes.items():
            keys = _split(path)
            node: Any = data
            for k in keys[:-1]:
                node = node[k] if isinstance(k, int) else node.setdefault(k, {})
            last = keys[-1]
            if value is None and isinstance(node, dict):
                node.pop(last, None)
            else:
                node[last] = value
        return Scenario(data)

    def with_overrides(self, overrides: dict[str, Any]) -> Scenario:
        """A copy with register overrides merged into ``overrides:``."""
        if not overrides:
            return self
        data = copy.deepcopy(self.data)
        data.setdefault("overrides", {}).update(overrides)
        return Scenario(data)


def _split(path: str) -> list[Any]:
    if path.startswith(SCENARIO_PREFIX):
        path = path[len(SCENARIO_PREFIX) :]
    return [int(p) if p.isdigit() else p for p in path.split(".")]


def split_changes(changes: dict[str, Any]) -> tuple[dict[str, Any], dict[str, Any]]:
    """Split a change set into scenario patches (``scenario.*``) and register
    overrides (every other path)."""
    scen = {k: v for k, v in changes.items() if k.startswith(SCENARIO_PREFIX)}
    reg = {k: v for k, v in changes.items() if not k.startswith(SCENARIO_PREFIX)}
    return scen, reg
