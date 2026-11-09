"""P3.9: every report type renders (HTML; PDF via Quarto or Chrome) from saved
outputs, and tables are exported as CSV and LaTeX."""

from __future__ import annotations

import json
from pathlib import Path

import marg
import pytest
import yaml
from conftest import PARAMS
from marg import report
from marg.design import Grid, Morris

RUN = """
name: report_run
description: A short corridor for the run report test.
duration_s: 300
warmup_s: 60
network:
  ribbons:
    - {id: main, polyline: [[0, 0], [600, 0]], width: [{s: 0, y_min: -3.5, y_max: 3.5}]}
demand:
  entries:
    - id: west
      ribbon: main
      flow: [{from_s: 0, veh_per_h: 1800}]
      composition: {motorcycle: 0.4, hatchback: 0.4, auto_rickshaw: 0.2}
detectors:
  - {kind: screenline, id: s500, ribbon: main, s: 500, interval_s: 60}
  - {kind: area, id: a100_400, ribbon: main, s_from: 100, s_to: 400, interval_s: 60}
kpi:
  throughput_detector: s500
"""


# Rendering needs Quarto or pandoc (CI installs pandoc). Without one, the
# tests still build every report's source, figures and tables.
FORMATS = ["html"] if report.renderer() else []


def check(paths: list[Path], formats=None) -> str:
    """Check a built report; return its HTML if rendered, else its source."""
    formats = FORMATS if formats is None else formats
    qmd = paths[0]
    assert qmd.suffix == ".qmd" and "$" not in qmd.read_text().replace("$$", "")
    for f in formats:
        p = qmd.with_suffix(f".{f}")
        assert p in paths and p.stat().st_size > 2000, p
    assert list((qmd.parent / "tables").glob("*.tex")) and list(
        (qmd.parent / "tables").glob("*.csv")
    )
    return qmd.with_suffix(".html").read_text() if "html" in formats else qmd.read_text()


def test_run_report_html_and_pdf(tmp_path: Path) -> None:
    run = marg.run(RUN, seed=2, out=tmp_path / "run", params_dir=PARAMS)
    tool = report.renderer()
    if tool is None or (tool == "pandoc" and report.find_chrome() is None):
        pytest.skip("no PDF renderer (Quarto, or pandoc + Chrome)")
    html = check(report.render("run", run.path, formats=["html", "pdf"]), ("html", "pdf"))
    assert "Headline KPIs" in html and "throughput_veh_h" in html
    assert (run.path / "report" / "run" / "figures" / "screenline_flow.pdf").exists()


def test_params_appendix_for_a_run(tmp_path: Path) -> None:
    run = marg.run(RUN, seed=2, out=tmp_path / "run", params_dir=PARAMS)
    html = check(report.render("params", run.path, formats=FORMATS))
    assert "vehicles.free_flow_speed.hatchback" in html and "Balakrishnan" in html


def test_experiment_and_sensitivity_reports(tmp_path: Path, tiny) -> None:
    grid = marg.Experiment(
        "grid",
        tiny,
        design=Grid({"vehicles.free_flow_speed.hatchback": [45.0, 60.0]}),
        arms={"observed": {}, "cooperative": marg.bundles.cooperative()},
        replications=marg.Replications(n=3),
        root=tmp_path,
        params_dir=PARAMS,
    ).run(marg.backends.Local(workers=4))
    html = check(report.render("experiment", grid.dir, formats=FORMATS))
    assert "cooperative − observed" in html or "cooperative" in html
    check(report.render("params", grid.dir, formats=FORMATS))

    screen = marg.Experiment(
        "morris",
        tiny,
        design=Morris(
            {
                "vehicles.free_flow_speed.hatchback": (40.0, 70.0),
                "interaction.following.time_gap.car": (0.6, 1.4),
                "interaction.following.time_gap.two_wheeler": (0.2, 0.6),
            },
            trajectories=4,
            seed=1,
        ),
        root=tmp_path,
        params_dir=PARAMS,
    ).run(marg.backends.Local(workers=4))
    html = check(report.render("sensitivity", screen.dir, formats=FORMATS))
    assert "Screening summary" in html and "noise floor" in html
    assert (screen.dir / "report" / "sensitivity" / "tables" / "morris_screen.csv").exists()
    check(report.render("experiment", screen.dir, formats=FORMATS))


def test_calibration_stage_report(tmp_path: Path) -> None:
    doc = {
        "stage": 1,
        "status": "frozen",
        "method": "least_squares",
        "created": "2026-11-09",
        "frozen": "2026-11-09",
        "params": {
            "vehicles.free_flow_speed.hatchback": {
                "value": 55.0,
                "estimate": 55.0,
                "se": 1.0,
                "ci": [53.0, 57.0],
                "ci_level": 0.95,
                "unit": "km/h",
                "bounds": [40, 90],
            }
        },
        "data": [{"path": "obs.json", "blake2b": "00ff"}],
        "diagnostics": {
            "success": True,
            "cost": 0.1,
            "evaluations": 12,
            "observed": {"hatchback_speed_kmh": {"value": 50.0, "se": 1.0}},
            "simulated": {"hatchback_speed_kmh": {"value": 50.2, "se": 1.0}},
            "residuals": {"hatchback_speed_kmh": 0.14},
        },
        "recovery": {
            "observe_seeds": [1],
            "fit_seeds": [2],
            "all_covered": True,
            "params": [
                {
                    "path": "vehicles.free_flow_speed.hatchback",
                    "truth": 55.0,
                    "estimate": 55.2,
                    "ci_lo": 53.0,
                    "ci_hi": 57.0,
                    "covered": True,
                }
            ],
        },
    }
    f = tmp_path / "stage_1.yaml"
    f.write_text(yaml.safe_dump(doc))
    html = check(report.render("calibration-stage", f, formats=FORMATS))
    assert "every" in html and "Frozen" in html


def test_cli_entry_point(tmp_path: Path) -> None:
    from marg.__main__ import main

    run = marg.run(RUN, seed=3, out=tmp_path / "run", params_dir=PARAMS)
    args = ["report", "run", str(run.path), "--out", str(tmp_path / "rep")]
    args += ["--format", "html"] if FORMATS else ["--no-render"]
    assert main(args) == 0
    assert (tmp_path / "rep" / ("run.html" if FORMATS else "run.qmd")).exists()
    assert json.loads((run.path / "manifest.json").read_text())["status"] == "ok"


def test_missing_renderer_fails_clearly(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    run = marg.run(RUN, seed=4, out=tmp_path / "run", params_dir=PARAMS)
    monkeypatch.setattr(report.shutil, "which", lambda _name: None)
    with pytest.raises(report.RenderError, match="pandoc"):
        report.render("run", run.path, formats=["html"])
    paths = report.render("run", run.path, formats=[])  # the source still builds
    assert [p.suffix for p in paths] == [".qmd"]
