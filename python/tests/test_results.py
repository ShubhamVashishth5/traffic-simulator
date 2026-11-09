"""P3.6: the experiment folder as one dataset (index, filters, Polars and DuckDB
agree), and the summary helpers and plots."""

from __future__ import annotations

from pathlib import Path

import marg
import polars as pl
import pytest
from conftest import PARAMS
from marg.design import Grid

V0 = "vehicles.free_flow_speed.hatchback"


@pytest.fixture(scope="module")
def exp(tmp_path_factory) -> marg.Experiment:
    from conftest import TINY

    e = marg.Experiment(
        "results",
        marg.Scenario.from_yaml(TINY),
        design=Grid({V0: [45.0, 55.0, 65.0]}),
        arms={
            "observed": {},
            "slow_bikes": {"vehicles.free_flow_speed.motorcycle": marg.design.Mean(35.0)},
        },
        replications=marg.Replications(n=3),
        root=tmp_path_factory.mktemp("res"),
        params_dir=PARAMS,
    )
    return e.run(marg.backends.Local(workers=4))


def test_index_and_keys(exp) -> None:
    data = marg.open_experiment(exp.dir)
    idx = data.index()
    assert idx.height == 18 and set(idx["status"]) == {"ok"}
    k = data.results("kpis")
    for c in ("run_id", "scenario", "point", "arm", "rep", "seed", V0, "throughput_veh_h"):
        assert c in k.columns
    assert k.height == 18
    assert sorted(k[V0].unique().to_list()) == [45.0, 55.0, 65.0]


def test_filters_read_only_matching_runs(exp) -> None:
    data = exp.data()
    k = data.results("kpis", filters={"arm": "observed", V0: [45.0, 65.0]})
    assert k.height == 6 and set(k["arm"]) == {"observed"}
    agents = data.results("agents", filters={"point": 1, "rep": 0})
    assert set(agents["arm"]) == {"observed", "slow_bikes"} and agents.height > 20
    with pytest.raises(KeyError):
        data.results("kpis", filters={"nope": 1})


def test_duckdb_and_polars_agree(exp) -> None:
    data = exp.data()
    con = data.duckdb(["kpis", "agents"])
    duck = con.sql(
        "SELECT point, arm, avg(delay_mean_s) m, count(*) n FROM kpis GROUP BY ALL ORDER BY ALL"
    ).pl()
    pol = (
        data.results("kpis")
        .group_by("point", "arm")
        .agg(pl.col("delay_mean_s").mean().alias("m"), pl.len().alias("n"))
        .sort("point", "arm")
    )
    assert duck["m"].to_list() == pytest.approx(pol["m"].to_list())
    assert duck["n"].to_list() == [3] * 6
    n_agents = con.sql("SELECT count(*) FROM agents").fetchall()[0][0]
    assert n_agents == data.results("agents").height


def test_summaries_and_plots(exp, tmp_path: Path) -> None:
    k = exp.results("kpis")
    s = marg.stats.summarize(k, "delay_mean_s", by=[V0, "arm"])
    assert s.height == 6 and (s["n"] == 3).all() and (s["lo"] <= s["hi"]).all()
    p = marg.stats.paired_summary(k, "delay_mean_s", "slow_bikes", "observed", by=[V0])
    assert p.height == 3 and "variance_ratio" in p.columns
    e = marg.stats.main_effects(k, "throughput_veh_h", [V0])
    assert e["level"].to_list() == [45.0, 55.0, 65.0]
    from marg import plots, style

    for name, fig in {
        "ci": plots.ci_plot(s, V0, ylabel="mean delay (s)"),
        "paired": plots.paired_plot(p, V0),
        "effects": plots.effect_plot(e),
    }.items():
        out = style.save(fig, str(tmp_path / name), formats=("png", "pdf"))
        assert all(Path(f).stat().st_size > 1000 for f in out)
