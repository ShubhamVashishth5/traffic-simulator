"""Experiment templates: ready-made experiments for questions users ask often
(plan P8.8). A template returns an ordinary :class:`marg.Experiment`, so it is
run, collected, reported and resumed like any other.

``persona_shares`` sweeps the two persona shares that G§3.2 grades C and asks
to report across their range: the delivery-rider share (of two-wheelers) and
the e-rickshaw share (of the vehicle stream). Each grid point is an arm, and
every arm runs on the same seeds (common random numbers, ADR-0003), so the
differences between points are paired. ``share_table`` gives each KPI, with
its 95% CI, as a function of both shares::

    exp = marg.templates.persona_shares("scenarios/signal_crossing.yaml", replications=3)
    exp.run()
    exp.collect()
    marg.templates.share_table(exp, ["delay_mean_s", "violations_per_1000_veh"])

``marg report experiment runs/persona_shares`` reports it by arm.
"""

from __future__ import annotations

import re
from collections.abc import Iterable, Mapping, Sequence
from pathlib import Path
from typing import Any

import polars as pl

from marg import stats
from marg.experiment import Experiment, Replications
from marg.results import ExperimentData
from marg.scenario import Scenario

#: The ten personas of G§3.2 (ADR-0018 part 6), and the shares the sweep keeps
#: fixed. A persona that is the only one eligible for its vehicle class takes
#: it whatever its share. The defaults are placeholders, as in
#: ``scripts/p8_milestone.py``: the guide gives no shares.
PERSONAS = (
    "average_commuter",
    "rushed_driver",
    "delivery_rider",
    "bike_taxi_rider",
    "e_rickshaw_driver",
    "auto_rickshaw_driver",
    "bus_driver",
    "truck_lcv_driver",
    "cautious_driver",
    "cyclist",
)
DEFAULT_SHARES: dict[str, float] = {
    "average_commuter": 0.70,
    "rushed_driver": 0.15,
    "bike_taxi_rider": 0.05,
    "cautious_driver": 0.10,
    "e_rickshaw_driver": 1.0,
    "auto_rickshaw_driver": 1.0,
    "bus_driver": 1.0,
    "truck_lcv_driver": 1.0,
    "cyclist": 1.0,
}
#: Personas a two-wheeler can have besides the delivery rider (G§3.2's
#: "typical vehicle"; `marg_params::Persona::eligible`).
TWO_WHEELER_OTHERS = ("average_commuter", "rushed_driver", "bike_taxi_rider")
#: Vehicle types of the e-rickshaw class in a composition.
E_RICKSHAW_TYPES = ("e_rickshaw",)

_ARM = re.compile(r"^delivery_(?P<d>[0-9.]+)__e_rickshaw_(?P<e>[0-9.]+)$")


def arm_name(delivery: float, e_rickshaw: float) -> str:
    """The arm of one grid point, e.g. ``delivery_0.15__e_rickshaw_0.1``."""
    return f"delivery_{delivery:g}__e_rickshaw_{e_rickshaw:g}"


def shares_of_arm(arm: str) -> tuple[float, float] | None:
    """``(delivery, e_rickshaw)`` from an arm name made by :func:`arm_name`."""
    m = _ARM.match(arm)
    return (float(m["d"]), float(m["e"])) if m else None


def persona_overrides(delivery: float, shares: Mapping[str, float] | None = None) -> dict:
    """All ten persona shares (a population is all ten or none), with the
    delivery rider's set so that it is exactly ``delivery`` of two-wheelers:
    shares are renormalised within each vehicle class, and only two-wheelers
    can be delivery riders, so the other classes are unchanged."""
    if not 0.0 <= delivery < 1.0:
        raise ValueError("the delivery-rider share must be in [0, 1)")
    s = {**DEFAULT_SHARES, **(shares or {})}
    s.pop("delivery_rider", None)
    missing = [p for p in PERSONAS if p != "delivery_rider" and p not in s]
    if missing:
        raise ValueError(f"persona shares missing: {', '.join(missing)}")
    others = sum(s[p] for p in TWO_WHEELER_OTHERS)
    if others <= 0.0 and delivery < 1.0:
        raise ValueError("two-wheelers need a persona other than the delivery rider")
    s["delivery_rider"] = delivery * others / (1.0 - delivery)
    return {f"personas.shares.{p}": float(s[p]) for p in PERSONAS}


def with_e_rickshaws(composition: Mapping[str, float], share: float) -> dict[str, float]:
    """A composition with e-rickshaws at ``share`` of the stream and the other
    types scaled to the rest, in their own proportions."""
    if not 0.0 <= share <= 1.0:
        raise ValueError("the e-rickshaw share must be in [0, 1]")
    rest = {k: float(v) for k, v in composition.items() if k not in E_RICKSHAW_TYPES}
    total = sum(rest.values())
    if total <= 0.0:
        raise ValueError("the composition has no vehicle types besides e-rickshaws")
    out = {k: v * (1.0 - share) / total for k, v in rest.items()}
    out[E_RICKSHAW_TYPES[0]] = float(share)
    return out


def persona_shares(
    base: Scenario | str | Path,
    *,
    delivery: Iterable[float] = (0.0, 0.15, 0.30),
    e_rickshaw: Iterable[float] = (0.03, 0.10, 0.20),
    shares: Mapping[str, float] | None = None,
    overrides: Mapping[str, Any] | None = None,
    replications: int | Replications = 3,
    name: str = "persona_shares",
    **kw: Any,
) -> Experiment:
    """The persona-share sweep of P8.8 (G§3.2): every pair of a delivery-rider
    share (of two-wheelers) and an e-rickshaw share (of every entry's
    stream), one arm each, on common seeds.

    ``shares`` replaces the fixed persona shares of :data:`DEFAULT_SHARES`;
    ``overrides`` is added to every arm (the rule's coefficients, for
    example); ``kw`` goes to :class:`marg.Experiment` (``root``, ``seed``,
    ``params_dir``, ``outputs``, ...).
    """
    scen = base if isinstance(base, Scenario) else Scenario.load(base)
    entries = scen.data.get("demand", {}).get("entries", [])
    if not entries:
        raise ValueError("the base scenario has no demand entries")
    arms: dict[str, dict[str, Any]] = {}
    for d in delivery:
        for e in e_rickshaw:
            arm: dict[str, Any] = dict(persona_overrides(d, shares))
            for k, entry in enumerate(entries):
                arm[f"scenario.demand.entries.{k}.composition"] = with_e_rickshaws(
                    entry["composition"], e
                )
            arm.update(overrides or {})
            arms[arm_name(d, e)] = arm
    reps = replications if isinstance(replications, Replications) else Replications(n=replications)
    return Experiment(name, scen, arms=arms, replications=reps, **kw)


def share_table(
    experiment: Experiment | ExperimentData | str | Path,
    metrics: Sequence[str] = ("throughput_veh_h", "delay_mean_s", "violations_per_1000_veh"),
    confidence: float = 0.95,
) -> pl.DataFrame:
    """Each metric (a ``kpis`` column) as a function of both shares: one row
    per (delivery, e_rickshaw, metric) with n, mean, SD and the Student-t CI
    over replications."""
    if isinstance(experiment, Experiment):
        data = experiment.data()
    elif isinstance(experiment, ExperimentData):
        data = experiment
    else:
        data = ExperimentData(experiment)
    k = data.results("kpis")
    rows = []
    for m in metrics:
        if m not in k.columns:
            raise ValueError(f"`{m}` is not a kpis column")
        s = stats.summarize(k, m, by=["arm"], confidence=confidence)
        for r in s.iter_rows(named=True):
            got = shares_of_arm(r["arm"])
            if got is None:
                continue
            rows.append(
                {
                    "delivery_share": got[0],
                    "e_rickshaw_share": got[1],
                    "metric": m,
                    "n": r["n"],
                    "mean": r["mean"],
                    "sd": r["sd"],
                    "lo": r["lo"],
                    "hi": r["hi"],
                }
            )
    if not rows:
        raise ValueError("no arm of this experiment comes from persona_shares")
    return pl.DataFrame(rows).sort(["metric", "delivery_share", "e_rickshaw_share"])
