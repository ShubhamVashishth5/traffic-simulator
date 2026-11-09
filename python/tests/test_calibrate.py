"""P3.8: staged calibration. A synthetic stage-1 recovery (desired-speed
distributions from simulated detector data) recovers the true values within
their CIs; the freeze rule is enforced; frozen stages reach every later run."""

from __future__ import annotations

import json
import shutil
from pathlib import Path

import marg
import numpy as np
import polars as pl
import pytest
import yaml
from conftest import PARAMS, synthetic_register
from marg.calibrate import (
    CalibrationError,
    Moments,
    Simulator,
    Stage,
    recovery_test,
    screenline_speed_moments,
)

FREE_FLOW = """
name: free_flow_recovery
duration_s: 3600
warmup_s: 120
network:
  ribbons:
    - {id: main, polyline: [[0, 0], [1200, 0]], width: [{s: 0, y_min: -5.25, y_max: 5.25}]}
demand:
  entries:
    - id: west
      ribbon: main
      flow: [{from_s: 0, veh_per_h: 150}]
      composition: {motorcycle: 0.4, hatchback: 0.4, auto_rickshaw: 0.2}
detectors:
  - {kind: screenline, id: s1000, ribbon: main, s: 1000, interval_s: 60}
"""
TYPES = ["motorcycle", "hatchback", "auto_rickshaw"]
V0 = {t: f"vehicles.free_flow_speed.{t}" for t in TYPES}


def simulator(root: Path):
    sc = marg.Scenario.from_yaml(FREE_FLOW)
    stat = screenline_speed_moments("s1000", TYPES)
    return lambda seeds: Simulator(sc, seeds, stat, params_dir=PARAMS, calibration_dir=root)


def test_stage1_synthetic_recovery(tmp_path: Path) -> None:
    root = tmp_path / "calibration"
    st = Stage(1, root, PARAMS)
    truth = {V0["motorcycle"]: 44.0, V0["hatchback"]: 68.0, V0["auto_rickshaw"]: 36.0}
    bounds = {
        V0["motorcycle"]: (30.0, 70.0),
        V0["hatchback"]: (40.0, 90.0),
        V0["auto_rickshaw"]: (25.0, 55.0),
    }
    res, report = recovery_test(
        st,
        truth,
        simulator(root),
        observe_seeds=range(101, 109),
        fit_seeds=range(1, 9),
        bounds=bounds,
    )
    for r in report["params"]:
        print(
            f"{r['path']}: truth {r['truth']:.1f}, estimate {r['estimate']:.2f} "
            f"[{r['ci_lo']:.2f}, {r['ci_hi']:.2f}] covered={r['covered']}"
        )
    assert report["all_covered"], report
    for r in report["params"]:  # informative: the CI is narrower than the search range
        lo, hi = bounds[r["path"]]
        assert r["ci_hi"] - r["ci_lo"] < 0.75 * (hi - lo)
    # Write the stage, with the "observed" data hashed.
    data = tmp_path / "observed.json"
    data.write_text(json.dumps(res.observed.to_dict()))
    p = st.write(res, data=[data], recovery=report)
    doc = yaml.safe_load(p.read_text())
    assert doc["status"] == "draft" and doc["stage"] == 1
    assert set(doc["params"]) == set(truth)
    assert doc["data"][0]["blake2b"] and doc["recovery"]["all_covered"]
    m = doc["params"][V0["hatchback"]]
    assert marg.register.value_mean(m["value"]) == pytest.approx(m["estimate"])


def fake_stage1(root: Path, value: float = 55.0) -> None:
    """A frozen stage-1 file setting the hatchback desired-speed mean."""
    st = Stage(1, root, PARAMS)
    b = st.bind([V0["hatchback"]])[0]
    root.mkdir(parents=True, exist_ok=True)
    doc = {"stage": 1, "status": "frozen", "params": {b.path: {"value": b.value(value)}}}
    (root / "stage_1.yaml").write_text(yaml.safe_dump(doc))


def test_freeze_rule(tmp_path: Path) -> None:
    root = tmp_path / "calibration"
    with pytest.raises(CalibrationError, match="stage 1 is not frozen"):
        Stage(2, root, PARAMS)
    fake_stage1(root)
    with pytest.raises(CalibrationError, match="never re-opened"):
        Stage(1, root, PARAMS)
    s2 = Stage(2, root, PARAMS)
    assert "interaction.following.time_gap.car" in s2.parameters
    with pytest.raises(CalibrationError, match="stage-1 `fit` parameter"):
        s2.check_writable([V0["motorcycle"]])
    with pytest.raises(CalibrationError, match="stage-1 `fit` parameter"):
        s2.fit(
            [V0["motorcycle"]],
            lambda ov: Moments(["a"], [0.0], [1.0]),
            Moments(["a"], [0.0], [1.0]),
        )
    with pytest.raises(CalibrationError, match="role `fix`"):
        s2.check_writable(["vehicles.length.hatchback"])
    # A hand-edited stage file that sneaks a stage-1 value into stage 2 is refused by the core.
    (root / "stage_2.yaml").write_text(
        yaml.safe_dump(
            {"stage": 2, "status": "draft", "params": {V0["motorcycle"]: {"value": 40.0}}}
        )
    )
    with pytest.raises(ValueError, match="may only set `fit` parameters of stage 2"):
        marg.Register(PARAMS, root)


def test_frozen_stages_reach_runs_and_experiments(tmp_path: Path, tiny) -> None:
    root = tmp_path / "calibration"
    fake_stage1(root, 55.0)
    res = marg.run(tiny.to_yaml(), out=tmp_path / "run", params_dir=PARAMS, calibration_dir=root)
    assert res.manifest["calibration_stages"] == [1]
    p = res.params.filter(pl.col("path") == V0["hatchback"]).row(0, named=True)
    assert p["set_by"] == "calibration"
    assert p["mean_si"] == pytest.approx(55.0 / 3.6)
    exp = marg.Experiment(
        "cal", tiny, root=tmp_path / "runs", params_dir=PARAMS, calibration_dir=root
    )
    exp.run(marg.backends.Local(workers=1))
    assert (exp.dir / "calibration" / "stage_1.yaml").exists()
    run = exp.run_list()[0]
    m = json.loads((exp.dir / run["path"] / "manifest.json").read_text())
    assert m["calibration_stages"] == [1]
    # The calibration layer is part of the run's identity.
    plain = marg.Experiment("plain", tiny, root=tmp_path / "runs", params_dir=PARAMS)
    assert plain.expand()[0]["run_hash"] != run["run_hash"]
    shutil.rmtree(root)


def test_cmaes_recovers_an_analytic_model(tmp_path: Path) -> None:
    reg = synthetic_register(tmp_path, {"a": (0.0, 10.0), "b": (0.0, 10.0)}, role="fit", stage=1)
    st = Stage(1, tmp_path / "cal", reg)
    truth = np.array([3.0, 7.0])

    def simulate(ov):  # a smooth model with known Jacobian
        a, b = ov["t.a"], ov["t.b"]
        return Moments(["u", "v", "w"], [a + b, a - b, 0.5 * a * b], [0.05, 0.05, 0.05])

    observed = simulate({"t.a": truth[0], "t.b": truth[1]})
    for method in ("cmaes", "least_squares"):
        r = st.fit(["t.a", "t.b"], simulate, observed, method=method, max_evaluations=3000)
        assert r.estimate == pytest.approx(truth, abs=1e-3), method
        assert np.all(r.se < 0.1)
