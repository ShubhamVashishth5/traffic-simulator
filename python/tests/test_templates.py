"""P8.8: the persona-share experiment template (marg.templates)."""

from __future__ import annotations

from pathlib import Path

import marg
import polars as pl
import pytest
from conftest import PARAMS
from marg import templates


def test_the_delivery_share_is_exact_within_two_wheelers() -> None:
    for d in (0.0, 0.15, 0.3):
        o = templates.persona_overrides(d)
        assert len(o) == 10
        share = {k.removeprefix("personas.shares."): v for k, v in o.items()}
        others = sum(share[p] for p in templates.TWO_WHEELER_OTHERS)
        assert share["delivery_rider"] / (share["delivery_rider"] + others) == pytest.approx(d)
        # The classes delivery riders cannot ride are unchanged.
        assert share["cautious_driver"] == templates.DEFAULT_SHARES["cautious_driver"]
    with pytest.raises(ValueError):
        templates.persona_overrides(1.0)


def test_the_e_rickshaw_share_is_exact_and_the_rest_keeps_its_proportions() -> None:
    comp = {"motorcycle": 0.4, "hatchback": 0.4, "auto_rickshaw": 0.1, "e_rickshaw": 0.1}
    out = templates.with_e_rickshaws(comp, 0.2)
    assert sum(out.values()) == pytest.approx(1.0)
    assert out["e_rickshaw"] == 0.2
    assert out["motorcycle"] / out["hatchback"] == pytest.approx(1.0)
    assert out["auto_rickshaw"] / out["motorcycle"] == pytest.approx(0.25)


def test_persona_share_sweep_reports_kpis_by_both_shares(tiny, tmp_path: Path) -> None:
    exp = templates.persona_shares(
        tiny,
        delivery=(0.0, 0.3),
        e_rickshaw=(0.05, 0.2),
        replications=2,
        root=tmp_path,
        params_dir=PARAMS,
    )
    assert sorted(exp.arms) == sorted(
        templates.arm_name(d, e) for d in (0.0, 0.3) for e in (0.05, 0.2)
    )
    exp.run(marg.backends.Local(workers=2))
    exp.collect()
    table = templates.share_table(exp, ["throughput_veh_h", "delay_mean_s"])
    assert table.height == 2 * 4
    assert set(table["delivery_share"]) == {0.0, 0.3}
    assert set(table["e_rickshaw_share"]) == {0.05, 0.2}
    assert (table["n"] == 2).all()
    agents = exp.results("agents")
    assert set(agents["arm"]) == set(exp.arms)
    # Delivery riders appear only where their share is positive.
    riders = agents.filter(pl.col("persona") == "delivery_rider")
    assert set(riders["arm"]) <= {a for a in exp.arms if a.startswith("delivery_0.3")}
