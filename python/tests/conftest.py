"""Shared fixtures: the repository register and a tiny, fast scenario."""

from __future__ import annotations

from pathlib import Path

import marg
import pytest

REPO = Path(__file__).resolve().parents[2]
PARAMS = REPO / "params" / "base"

TINY = """
name: tiny
duration_s: 90
warmup_s: 15
network:
  ribbons:
    - {id: main, polyline: [[0, 0], [400, 0]], width: [{s: 0, y_min: -3.5, y_max: 3.5}]}
demand:
  entries:
    - id: west
      ribbon: main
      flow: [{from_s: 0, veh_per_h: 1500}]
      composition: {motorcycle: 0.4, hatchback: 0.4, auto_rickshaw: 0.2}
detectors:
  - {kind: screenline, id: s300, ribbon: main, s: 300, interval_s: 30}
kpi:
  throughput_detector: s300
"""


@pytest.fixture
def tiny() -> marg.Scenario:
    return marg.Scenario.from_yaml(TINY)


@pytest.fixture
def register() -> marg.Register:
    return marg.Register(PARAMS)


def synthetic_register(
    root: Path, names: dict[str, tuple[float, float]], role: str = "sweep", stage: int | None = None
) -> Path:
    """A register of scalar parameters `t.<name>` with the given ranges (role
    `sweep`, or `fit` in `stage`), for runs by the Function (fake simulator) backend."""
    d = root / "register"
    d.mkdir(parents=True, exist_ok=True)
    lines = ["group: t", "entries:"]
    for n, (lo, hi) in names.items():
        lines += [
            f"  {n}:",
            f"    value: {(lo + hi) / 2}",
            '    unit: "1"',
            "    grade: C",
            f"    role: {role}",
            *([f"    stage: {stage}"] if stage is not None else []),
            "    source: synthetic test function",
            '    guide: "-"',
            f"    range: [{lo}, {hi}]",
        ]
    (d / "t.yaml").write_text("\n".join(lines) + "\n")
    return d
