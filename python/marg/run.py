"""Run a single scenario from Python."""

from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from marg import _core


@dataclass(frozen=True)
class RunResult:
    """A finished run: its folder and manifest."""

    path: Path
    manifest: dict[str, Any]


def run(
    scenario: str | Path,
    seed: int = 1,
    out: str | Path = "runs/adhoc",
    threads: int = 1,
) -> RunResult:
    """Run ``scenario`` (a YAML file path or YAML text) and write its run folder."""
    text = Path(scenario).read_text() if _looks_like_path(scenario) else str(scenario)
    out_path = Path(out)
    manifest = json.loads(_core.run_yaml(text, seed, str(out_path), threads))
    return RunResult(path=out_path, manifest=manifest)


def _looks_like_path(s: str | Path) -> bool:
    if isinstance(s, Path):
        return True
    return "\n" not in s and s.endswith((".yaml", ".yml"))
