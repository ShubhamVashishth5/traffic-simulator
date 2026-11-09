"""Reports v1 (plan P3.9, plan/04 §7): run, experiment, sensitivity,
calibration-stage and the parameter appendix.

Each report is built from the saved Parquet outputs only, never by hand:

1. figures in the house style (:mod:`marg.style`), as PNG (HTML) and PDF (papers);
2. tables as CSV and LaTeX (booktabs) next to the report, and Markdown in it;
3. a Quarto document (``<kind>.qmd``) from the template in ``report_templates/``;
4. rendering to HTML and PDF: with Quarto if it is installed (HTML and Typst
   PDF); otherwise pandoc for HTML and headless Chrome/Chromium to print it to
   PDF. The ``.qmd`` has no executable cells, so both routes give the same report.

``marg report <kind> <path>`` (or ``python -m marg report``) calls :func:`render`.
"""

from __future__ import annotations

import datetime as _dt
import glob
import json
import math
import shutil
import subprocess
from collections.abc import Sequence
from pathlib import Path
from string import Template
from typing import Any

import polars as pl
import yaml

from marg import plots, stats, style

TEMPLATES = Path(__file__).parent / "report_templates"
KINDS = ("run", "experiment", "sensitivity", "calibration-stage", "params")
DEFAULT_METRICS = ("throughput_veh_h", "delay_mean_s", "delay_p95_s", "network_mean_speed_m_s")

CSS = """
body { font-family: system-ui, -apple-system, "Segoe UI", sans-serif; max-width: 60rem;
       margin: 2rem auto; padding: 0 1rem; color: #0b0b0b; background: #fcfcfb; line-height: 1.45; }
h1, h2, h3 { line-height: 1.2; } h2 { margin-top: 2rem; border-bottom: 1px solid #e1e0d9; }
.subtitle { color: #52514e; }
table { border-collapse: collapse; margin: 0.8rem 0; font-size: 0.85rem; }
th, td { padding: 0.25rem 0.6rem; border-bottom: 1px solid #e1e0d9; text-align: left;
         font-variant-numeric: tabular-nums; }
th { border-bottom: 1px solid #c3c2b7; }
img { max-width: 100%; } figure { margin: 1rem 0; }
figcaption { color: #52514e; font-size: 0.85rem; }
code { font-size: 0.85em; }
@media print { body { max-width: none; margin: 0; } }
"""


# ----- formatting helpers -----


def fmt(v: Any, digits: int = 4) -> str:
    if v is None:
        return "–"
    if isinstance(v, bool):
        return "yes" if v else "no"
    if isinstance(v, float):
        if math.isnan(v):
            return "–"
        if v == 0:
            return "0"
        if abs(v) >= 1e5 or abs(v) < 1e-3:
            return f"{v:.{digits - 1}e}"
        return f"{v:.{digits}g}" if abs(v) < 10**digits else f"{v:,.0f}"
    return str(v)


def _md_cell(v: Any) -> str:
    return fmt(v).replace("|", "\\|").replace("\n", " ")


def _tex(s: str) -> str:
    for a, b in (
        ("\\", r"\textbackslash{}"),
        ("&", r"\&"),
        ("%", r"\%"),
        ("$", r"\$"),
        ("#", r"\#"),
        ("_", r"\_"),
        ("{", r"\{"),
        ("}", r"\}"),
        ("~", r"\~{}"),
        ("^", r"\^{}"),
    ):
        s = s.replace(a, b)
    return s


class Out:
    """A report output folder: figures/, tables/ and the document."""

    def __init__(self, path: Path, formats: Sequence[str]) -> None:
        self.path = path
        self.formats = formats
        (path / "figures").mkdir(parents=True, exist_ok=True)
        (path / "tables").mkdir(parents=True, exist_ok=True)

    def table(self, df: pl.DataFrame, name: str, caption: str = "", max_rows: int = 60) -> str:
        """Write CSV and LaTeX; return Markdown (at most ``max_rows`` rows shown)."""
        df.write_csv(self.path / "tables" / f"{name}.csv")
        cols = df.columns
        tex = [
            r"\begin{tabular}{" + "l" * len(cols) + "}",
            r"\toprule",
            " & ".join(_tex(c) for c in cols) + r" \\",
            r"\midrule",
        ]
        tex += [" & ".join(_tex(fmt(v)) for v in row) + r" \\" for row in df.iter_rows()]
        tex += [r"\bottomrule", r"\end{tabular}"]
        (self.path / "tables" / f"{name}.tex").write_text("\n".join(tex) + "\n")
        if df.is_empty():
            return "_(none)_"
        shown = df.head(max_rows)
        md = ["| " + " | ".join(cols) + " |", "|" + "---|" * len(cols)]
        md += ["| " + " | ".join(_md_cell(v) for v in row) + " |" for row in shown.iter_rows()]
        if df.height > max_rows:
            md.append(f"\n_{df.height - max_rows} more rows in `tables/{name}.csv`._")
        if caption:
            md.append(f"\n: {caption}")
        return "\n".join(md)

    def figure(self, fig: Any, name: str, caption: str) -> str:
        style.save(fig, str(self.path / "figures" / name), formats=("png", "pdf"))
        return f"![{caption}](figures/{name}.png)"


def _kv(rows: list[tuple[str, Any]]) -> pl.DataFrame:
    return pl.DataFrame({"item": [r[0] for r in rows], "value": [fmt(r[1]) for r in rows]})


def _now() -> str:
    return _dt.datetime.now(_dt.UTC).strftime("%Y-%m-%d %H:%M UTC")


# ----- run report -----


def build_run(path: Path, out: Out, metrics: Sequence[str] | None) -> dict[str, Any]:
    from marg.run import RunResult

    r = RunResult.open(path)
    m = r.manifest
    spec = yaml.safe_load((path / "spec.resolved.yaml").read_text())
    sc = spec["scenario"]
    ribbons = sc["network"]["ribbons"]
    length = sum(
        math.dist(rb["polyline"][i], rb["polyline"][i + 1])
        for rb in ribbons
        for i in range(len(rb["polyline"]) - 1)
    )
    widths = [w["y_max"] - w["y_min"] for rb in ribbons for w in rb["width"]]
    flows = [
        f"{e['id']}: {fmt(sum(f['veh_per_h'] for f in e['flow']) / len(e['flow']))} veh/h"
        for e in sc["demand"]["entries"]
    ]
    scen = _kv(
        [
            ("duration (s)", sc["duration_s"]),
            ("warm-up (s)", sc.get("warmup_s", 0)),
            ("time step (s)", sc.get("dt_s", 0.1)),
            ("ribbons", len(ribbons)),
            ("road length (m)", length),
            ("width (m)", f"{min(widths):g}–{max(widths):g}"),
            ("demand (mean)", "; ".join(flows)),
            ("detectors", ", ".join(d["id"] for d in sc.get("detectors", []))),
        ]
    )
    prov = _kv(
        [
            ("run hash", m["run_hash"]),
            ("seed", m["seed"]),
            ("marg", m["marg_version"]),
            ("git", m["git_sha"][:12] + ("-dirty" if m["git_dirty"] else "")),
            ("build", m["build_profile"]),
            ("threads", m["threads"]),
            ("wall time (s)", m["wall_time_s"]),
            ("agent-steps", m["agent_steps"]),
            ("output hash", m["output_hash"]),
            ("calibration stages", ", ".join(map(str, m.get("calibration_stages", []))) or "none"),
        ]
    )
    kpis = pl.DataFrame({"kpi": list(r.kpis), "value": [fmt(v) for v in r.kpis.values()]})

    det = r.detectors.filter(~pl.col("in_warmup"))
    parts = []
    screen = det.filter(pl.col("kind") == "screenline")
    if screen.height:
        ts = (
            screen.group_by("detector_id", "t_start")
            .agg(
                (
                    pl.col("count").sum() * 3600 / (pl.col("t_end") - pl.col("t_start")).first()
                ).alias("flow_veh_h")
            )
            .sort("detector_id", "t_start")
        )
        fig = plots.timeseries(
            ts,
            "t_start",
            "flow_veh_h",
            series="detector_id",
            ylabel="flow (veh/h)",
            title="Screenline flow",
            xlabel="interval start (s)",
        )
        parts.append(out.figure(fig, "screenline_flow", "Flow at each screenline, per interval"))
    area = det.filter(pl.col("kind") == "area")
    if area.height:
        ts = (
            area.group_by("detector_id", "t_start")
            .agg((pl.col("ttd").sum() / pl.col("tts").sum() * 3.6).alias("speed_kmh"))
            .sort("detector_id", "t_start")
        )
        fig = plots.timeseries(
            ts,
            "t_start",
            "speed_kmh",
            series="detector_id",
            ylabel="space-mean speed (km/h)",
            title="Area speed (Edie)",
            xlabel="interval start (s)",
        )
        parts.append(out.figure(fig, "area_speed", "Space-mean speed in each area (Edie)"))
    agents = r.agents
    by_type = (
        agents.group_by("vehicle_type")
        .agg(
            pl.len().alias("vehicles"),
            (pl.col("v0").mean() * 3.6).alias("mean v0 (km/h)"),
            ((pl.col("distance") / pl.col("travel_time")).mean() * 3.6).alias("mean speed (km/h)"),
            pl.col("delay").mean().alias("mean delay (s)"),
            pl.col("exited").mean().alias("share exited"),
        )
        .sort("vehicles", descending=True)
    )
    ev = r.table("events")
    events = (
        out.table(ev.group_by("kind").agg(pl.len().alias("count")).sort("kind"), "events")
        if ev.height
        else "No events were recorded."
    )
    p = r.params
    excerpt = p.filter((pl.col("set_by") != "base") | (pl.col("role") == "fit")).select(
        "path", "value", "unit", "grade", "role", "stage", "set_by"
    )
    return {
        "scenario": sc["name"],
        "subtitle": f"run {m['run_hash']} · seed {m['seed']} · {_now()}",
        "description": sc.get("description", "").strip(),
        "scenario_table": out.table(scen, "scenario"),
        "provenance_table": out.table(prov, "provenance"),
        "warmup_s": fmt(sc.get("warmup_s", 0)),
        "kpi_table": out.table(kpis, "kpis"),
        "detector_section": "\n\n".join(parts) or "No detectors.",
        "agents_table": out.table(by_type, "vehicles_by_type"),
        "events_section": events,
        "params_table": out.table(excerpt, "params_excerpt", max_rows=40),
    }


# ----- experiment report -----


def _metrics(df: pl.DataFrame, metrics: Sequence[str] | None) -> list[str]:
    want = list(metrics or DEFAULT_METRICS)
    return [c for c in want if c in df.columns]


def build_experiment(path: Path, out: Out, metrics: Sequence[str] | None) -> dict[str, Any]:
    from marg.results import ExperimentData

    data = ExperimentData(path)
    d = data.definition
    des = d["design"]
    factors = pl.DataFrame(
        des.get("factors", []),
        schema={
            "path": pl.Utf8,
            "lo": pl.Float64,
            "hi": pl.Float64,
            "transform": pl.Utf8,
            "unit": pl.Utf8,
        },
    )
    args = {k: v for k, v in des.items() if k not in ("type", "factors", "levels")}
    design_text = (
        f"Design **{des['type']}** with {len(des.get('factors', []))} factors and "
        f"{d.get('points', '?')} points"
        + (f" ({', '.join(f'{k} = {v}' for k, v in args.items())})" if args else "")
        + ". Distribution-valued factors are set through their mean (ADR-0006)."
    )
    arms = pl.DataFrame(
        {
            "arm": list(d["arms"]),
            "overrides": [
                json.dumps(v, sort_keys=True) if v else "(none)" for v in d["arms"].values()
            ],
        }
    )
    idx = data.index()
    status = idx.group_by("status").agg(pl.len().alias("runs")).sort("status")
    rep = d["replications"]
    rule = (
        f"fixed, n = {rep['n']}"
        if rep.get("n")
        else f"sequential on `{rep['metric']}`: {rep['min']}–{rep['max']} replications until the "
        f"{rep['confidence']:.0%} CI half-width ≤ {rep['ci_halfwidth']}"
        f"{' × |mean|' if rep['relative'] else ''}"
    )
    runs_text = f"{idx.height} runs; replications {rule}; seed {d['seed']}.\n\n" + out.table(
        status, "run_status"
    )
    reps_section = ""
    rj = path / "replications.json"
    if rj.exists():
        dec = json.loads(rj.read_text())
        rt = pl.DataFrame(
            {
                "point": list(dec),
                "n": [v["n"] for v in dec.values()],
                "reason": [v["reason"] for v in dec.values()],
            }
        )
        reps_section = "Why each point stopped adding replications:\n\n" + out.table(
            rt, "replications"
        )

    k = data.results("kpis")
    ms = _metrics(k, metrics)
    fpaths = data.factors
    sections = []
    many = data.design().height > 30
    for mtr in ms:
        sec = [f"### {mtr}"]
        by = [*fpaths, "arm"] if not many else ["arm"]
        s = stats.summarize(k, mtr, by=by)
        sec.append(
            out.table(
                s,
                f"summary_{mtr}",
                caption=f"{mtr}: mean and 95% CI over replications"
                + (" (pooled over design points)" if many else ""),
            )
        )
        if len(fpaths) == 1 and not many:
            sec.append(
                out.figure(
                    plots.ci_plot(s, fpaths[0], series="arm", ylabel=mtr),
                    f"ci_{mtr}",
                    f"{mtr} by {fpaths[0]} (mean ± 95% CI)",
                )
            )
        arm_names = list(d["arms"])
        if len(arm_names) > 1:
            base = arm_names[0]
            for a in arm_names[1:]:
                ps = stats.paired_summary(k, mtr, a, base, by=fpaths if not many else ())
                sec.append(
                    out.table(
                        ps, f"paired_{mtr}_{a}", caption=f"{mtr}: {a} − {base}, paired by seed"
                    )
                )
        if des["type"] in ("grid", "full_factorial", "fractional_factorial") and fpaths:
            e = stats.main_effects(k, mtr, fpaths)
            sec.append(
                out.figure(
                    plots.effect_plot(e, ylabel=mtr), f"effects_{mtr}", f"Main effects on {mtr}"
                )
            )
        sections.append("\n\n".join(sec))
    if des["type"] in ("morris", "saltelli"):
        sections.append(
            f"Sensitivity indices for this {des['type']} design are in the "
            "sensitivity report (`marg report sensitivity`)."
        )
    return {
        "name": d.get("name", path.name),
        "subtitle": f"{d.get('marg_version', '')} · register {d.get('register', '')} · {_now()}",
        "design_text": design_text,
        "factors_table": out.table(factors, "factors") if factors.height else "",
        "crn": d.get("crn", "all"),
        "arms_table": out.table(arms, "arms"),
        "runs_text": runs_text,
        "replications_section": reps_section,
        "results_section": "\n\n".join(sections) or "No metrics found.",
    }


# ----- sensitivity report -----


def build_sensitivity(path: Path, out: Out, metrics: Sequence[str] | None) -> dict[str, Any]:
    from marg import sensitivity
    from marg.results import ExperimentData

    data = ExperimentData(path)
    d = data.definition
    kind = d["design"]["type"]
    k = data.results("kpis")
    ms = [m for m in _metrics(k, metrics) if k[m].drop_nans().drop_nulls().n_unique() > 1]
    sections = []
    if kind == "morris":
        m = sensitivity.morris(data, ms)
        intro = (
            f"Morris elementary-effects screening (G§8.4) over {len(data.factors)} factors, "
            f"{d['design']['trajectories']} trajectories on a {d['design']['levels']}-level grid "
            f"({d.get('points')} points × replications). μ* ranks influence; σ above the diagonal "
            "σ = μ* marks non-linear or interacting effects. μ* is in output units per factor's "
            "whole design range (each elementary effect divides the output change by the step as "
            "a fraction of the range); `mu_star_rel` compares factors within one output."
        )
        screen = sensitivity.morris_screen(m)
        counts = screen.group_by("class").agg(pl.len().alias("factors")).sort("class")
        floors = m.group_by("output", maintain_order=True).agg(
            pl.col("noise_floor").first(), pl.col("mu_star").max().alias("top_mu_star")
        )
        sections.append(
            "## Screening summary\n\n"
            "Every μ* carries simulation noise: even with common random numbers, changing a "
            "parameter changes the realisation. The noise floor is the μ* a factor with no effect "
            "would show, estimated from the spread of each elementary effect across replications "
            "(`marg.sensitivity.morris_noise_floor`). Factors are *influential* when μ* exceeds "
            f"{sensitivity.SCREEN_SNR:g} × the floor for some output, *within noise* when μ* "
            "stays below the floor for every output, and *unresolved* in between (more "
            "replications shrink the floor as 1/√R). These cut-offs are reporting conventions, "
            "not model parameters.\n\n"
            + out.table(floors, "morris_noise_floor", caption="Noise floor of μ* by output")
            + "\n\n"
            + out.table(counts, "morris_classes")
            + "\n\n"
            + out.table(screen, "morris_screen", caption="Screen by factor", max_rows=80)
        )
        for o in ms:
            mo = m.filter(pl.col("output") == o)
            sections.append(
                f"## {o}\n\n"
                + out.figure(plots.morris_plot(mo, o), f"morris_{o}", f"Morris μ*–σ for {o}")
                + "\n\n"
                + out.table(mo.drop("output"), f"morris_{o}", max_rows=25)
            )
    elif kind == "saltelli":
        s = sensitivity.sobol(data, ms)
        intro = (
            "Sobol variance decomposition (G§8.4): first-order S1 and total ST with bootstrap "
            "95% CIs; ST − S1 is the share of variance from interactions."
        )
        for o in ms:
            so = s.filter(pl.col("output") == o)
            sections.append(
                f"## {o}\n\n"
                + out.figure(plots.sobol_plot(so, o), f"sobol_{o}", f"Sobol indices for {o}")
                + "\n\n"
                + out.table(so.drop("output"), f"sobol_{o}")
            )
    else:
        r = sensitivity.surrogate(data, ms, model="gp")
        intro = (
            "Sobol indices on a Gaussian-process surrogate fitted to this design "
            "(Ciuffo et al. 2013), with 5-fold cross-validated R²."
        )
        for o in ms:
            so = r.indices.filter(pl.col("output") == o)
            sections.append(
                f"## {o}\n\nSurrogate R² = {r.r2[o]:.3f}.\n\n"
                + out.figure(
                    plots.sobol_plot(so, o), f"surrogate_{o}", f"Surrogate Sobol indices for {o}"
                )
                + "\n\n"
                + out.table(so.drop("output"), f"surrogate_{o}")
            )
    skipped = [m for m in _metrics(k, metrics) if m not in ms]
    if skipped:
        sections.append(
            f"Outputs without variation across the design (not analysed): {', '.join(skipped)}."
        )
    return {
        "name": d.get("name", path.name),
        "subtitle": f"{kind} · {data.index().height} runs · {_now()}",
        "intro": intro,
        "sections": "\n\n".join(sections),
    }


# ----- calibration stage report -----


def build_calibration_stage(path: Path, out: Out, metrics: Sequence[str] | None) -> dict[str, Any]:
    doc = yaml.safe_load(path.read_text())
    rows = []
    for p, v in doc.get("params", {}).items():
        rows.append(
            {
                "path": p,
                "estimate": v.get("estimate"),
                "se": v.get("se"),
                "ci_lo": (v.get("ci") or [None, None])[0],
                "ci_hi": (v.get("ci") or [None, None])[1],
                "unit": v.get("unit", ""),
                "bounds": str(v.get("bounds", "")),
            }
        )
    params = pl.DataFrame(rows) if rows else pl.DataFrame({"path": []})
    diag = doc.get("diagnostics", {})
    dt = _kv([(k, v) for k, v in diag.items() if not isinstance(v, dict)])
    mom = ""
    if "observed" in diag:
        mt = pl.DataFrame(
            [
                {
                    "statistic": n,
                    "observed": diag["observed"][n]["value"],
                    "observed_se": diag["observed"][n]["se"],
                    "simulated": diag.get("simulated", {}).get(n, {}).get("value"),
                    "residual (SE units)": diag.get("residuals", {}).get(n),
                }
                for n in diag["observed"]
            ]
        )
        mom = out.table(mt, "moments", caption="Observed and simulated statistics at the optimum")
    rec = doc.get("recovery")
    if rec:
        rt = pl.DataFrame(
            [
                {k: r.get(k) for k in ("path", "truth", "estimate", "ci_lo", "ci_hi", "covered")}
                for r in rec["params"]
            ]
        )
        recovery = (
            f"Simulated with known values at seeds {rec['observe_seeds']}, fitted at "
            f"seeds {rec['fit_seeds']}: **{'every' if rec['all_covered'] else 'not every'} "
            "true value is inside its CI.**\n\n" + out.table(rt, "recovery")
        )
    else:
        recovery = "No synthetic recovery test is recorded for this stage."
    data = pl.DataFrame(doc.get("data") or [], schema={"path": pl.Utf8, "blake2b": pl.Utf8})
    ci_level = next((v.get("ci_level") for v in doc.get("params", {}).values()), 0.95) or 0.95
    return {
        "stage": doc["stage"],
        "subtitle": f"{path} · {_now()}",
        "status": doc.get("status", "?"),
        "method": doc.get("method", "?"),
        "created": doc.get("created", "?"),
        "frozen_text": (
            f"Frozen {doc['frozen']}: these values apply to every later run and stage."
            if doc.get("status") == "frozen"
            else "Draft: not applied to any run until frozen."
        ),
        "ci_pct": f"{ci_level * 100:g}",
        "params_table": out.table(params, "fitted"),
        "diagnostics_table": out.table(dt, "diagnostics"),
        "moments_table": mom,
        "recovery_section": recovery,
        "data_table": out.table(data, "data") if data.height else "No data files recorded.",
        "notes": doc.get("notes", ""),
    }


# ----- parameter appendix -----


def build_params(path: Path, out: Out, metrics: Sequence[str] | None) -> dict[str, Any]:
    factors_text = ""
    if (path / "params.parquet").exists():
        p = pl.read_parquet(path / "params.parquet")
        what, name = "run", json.loads((path / "manifest.json").read_text())["scenario"]
    else:
        from marg.results import ExperimentData

        data = ExperimentData(path)
        runs = data.runs()
        first = path / runs["path"][0]
        p = pl.read_parquet(first / "params.parquet")
        what, name = "experiment", data.name
        fs = data.definition["design"].get("factors", [])
        if fs:
            factors_text = (
                "Design factors vary across runs; the values below are those of the "
                "first run. Factor ranges:\n\n" + out.table(pl.DataFrame(fs), "appendix_factors")
            )
        arms = {a: v for a, v in data.definition.get("arms", {}).items() if v}
        if arms:
            factors_text += "\n\nArm overrides:\n\n" + out.table(
                pl.DataFrame(
                    {"arm": list(arms), "overrides": [json.dumps(v) for v in arms.values()]}
                ),
                "appendix_arms",
            )
    p = p.with_columns(
        pl.col("path").str.split(".").list.first().alias("group"),
        pl.when(pl.col("role") == "fit")
        .then(pl.format("fit/{}", pl.col("stage")))
        .otherwise(pl.col("role"))
        .alias("role_stage"),
    )
    summary = (
        p.group_by("grade")
        .agg(
            *[(pl.col("role") == r).sum().alias(r) for r in ("fix", "fit", "sweep", "scenario")],
            pl.len().alias("total"),
        )
        .sort("grade")
    )
    sections = []
    for g in p["group"].unique(maintain_order=True).to_list():
        sub = p.filter(pl.col("group") == g).select(
            "path", "value", "unit", "grade", "role_stage", "set_by", "source"
        )
        sections.append(f"## {g}\n\n" + out.table(sub, f"params_{g}", max_rows=500))
    return {
        "name": name,
        "subtitle": f"{p.height} parameters · {_now()}",
        "what": what,
        "summary_table": out.table(
            summary, "params_by_grade_role", caption="Parameters by trust grade and role"
        ),
        "factors_text": factors_text,
        "sections": "\n\n".join(sections),
    }


BUILDERS = {
    "run": build_run,
    "experiment": build_experiment,
    "sensitivity": build_sensitivity,
    "calibration-stage": build_calibration_stage,
    "params": build_params,
}


# ----- rendering -----


def find_chrome() -> str | None:
    for name in ("google-chrome", "chromium", "chromium-browser", "chrome"):
        p = shutil.which(name)
        if p:
            return p
    hits = sorted(glob.glob("/opt/pw-browsers/chromium-*/chrome-linux*/chrome"))
    return hits[-1] if hits else None


def _quarto(qmd: Path, formats: Sequence[str]) -> list[Path]:
    out = []
    for f in formats:
        to = "html" if f == "html" else "typst"
        subprocess.run(
            ["quarto", "render", qmd.name, "--to", to],
            cwd=qmd.parent,
            check=True,
            capture_output=True,
        )
        out.append(qmd.with_suffix(f".{'html' if f == 'html' else 'pdf'}"))
    return out


def _pandoc(qmd: Path, formats: Sequence[str]) -> list[Path]:
    css = qmd.parent / "report.css"
    css.write_text(CSS)
    html = qmd.with_suffix(".html")
    subprocess.run(
        [
            "pandoc",
            qmd.name,
            "--from",
            "markdown",
            "--to",
            "html5",
            "--standalone",
            "--embed-resources",
            "--css",
            css.name,
            "--toc",
            "--toc-depth=2",
            "--metadata",
            "lang=en",
            "-o",
            html.name,
        ],
        cwd=qmd.parent,
        check=True,
        capture_output=True,
    )
    out = [html] if "html" in formats else []
    if "pdf" in formats:
        chrome = find_chrome()
        if chrome is None:
            raise RuntimeError("PDF needs Quarto, or Chrome/Chromium to print the HTML")
        pdf = qmd.with_suffix(".pdf")
        subprocess.run(
            [
                chrome,
                "--headless",
                "--no-sandbox",
                "--disable-gpu",
                "--no-pdf-header-footer",
                f"--print-to-pdf={pdf}",
                html.resolve().as_uri(),
            ],
            check=True,
            capture_output=True,
            timeout=120,
        )
        out.append(pdf)
        if "html" not in formats:
            html.unlink()
    return out


def render(
    kind: str,
    path: str | Path,
    out: str | Path | None = None,
    formats: Sequence[str] | None = None,
    metrics: Sequence[str] | None = None,
) -> list[Path]:
    """Build report ``kind`` for ``path`` (a run folder, an experiment folder or
    a stage file) into ``out`` (default ``<path>/report``, or next to a stage
    file), and render it. Returns the ``.qmd`` and the rendered files."""
    if kind not in BUILDERS:
        raise ValueError(f"unknown report `{kind}`; expected one of {KINDS}")
    src = Path(path)
    default = src.parent / f"report-{src.stem}" if src.is_file() else src / "report" / kind
    o = Out(Path(out) if out else default, formats or ("html", "pdf"))
    ctx = BUILDERS[kind](src, o, metrics)
    text = Template((TEMPLATES / f"{kind}.qmd").read_text()).safe_substitute(ctx)
    qmd = o.path / f"{kind}.qmd"
    qmd.write_text(text)
    fmts = list(formats or ("html", "pdf"))
    done = _quarto(qmd, fmts) if shutil.which("quarto") else _pandoc(qmd, fmts)
    return [qmd, *done]
