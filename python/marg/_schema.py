# Generated from schema/scenario.schema.json by scripts/gen-schema.sh. Do not edit.

from __future__ import annotations

from typing import Annotated, Literal

from pydantic import BaseModel, ConfigDict, Field, RootModel
from typing_extensions import TypeAliasType


class ClosedInterval(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    from_s: float
    to_s: float


class DetectorSpec1(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    id: str
    interval_s: float | None = 60.0
    kind: Literal['screenline']
    ribbon: str
    s: float


class DetectorSpec2(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    id: str
    interval_s: float | None = 60.0
    kind: Literal['area']
    ribbon: str
    s_from: float
    s_to: float


class DetectorSpec(RootModel[DetectorSpec1 | DetectorSpec2]):
    root: DetectorSpec1 | DetectorSpec2 = Field(..., description='A virtual detector.')


class Dist1(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    dist: Literal['constant']
    value: float


class Dist2(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    dist: Literal['normal']
    mean: float
    sd: float


class Dist3(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    dist: Literal['trunc_normal']
    max: float
    mean: float
    min: float
    sd: float


class Dist4(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    dist: Literal['log_normal']
    mu: float
    sigma: float


class Dist5(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    dist: Literal['log_normal_median']
    median: float
    sigma: float


class Dist6(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    dist: Literal['log_normal_moments']
    mean: float
    sd: float


class Dist7(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    dist: Literal['log_normal_quantiles']
    p_hi: float
    p_lo: float
    x_hi: float
    x_lo: float


class Dist8(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    dist: Literal['gamma']
    scale: float
    shape: float


class Dist9(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    dist: Literal['exponential']
    mean: float


class Dist10(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    dist: Literal['shifted_exponential']
    mean: float
    shift: float


class Dist11(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    dist: Literal['uniform']
    max: float
    min: float


Point = TypeAliasType(
    "Point", Annotated[list[float], Field(max_length=2, min_length=2)]
)


class Dist12(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    dist: Literal['empirical']
    points: list[Point]


class Composition(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    auto_rickshaw: float | None = None
    bicycle: float | None = None
    bus: float | None = None
    cycle_rickshaw: float | None = None
    e_rickshaw: float | None = None
    e_two_wheeler: float | None = None
    hatchback: float | None = None
    lcv: float | None = None
    moped: float | None = None
    motorcycle: float | None = None
    scooter: float | None = None
    sedan: float | None = None
    suv: float | None = None
    tractor: float | None = None
    truck: float | None = None


class FlowInterval(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    from_s: float
    veh_per_h: float


class KpiSpec(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    throughput_detector: str | None = Field(
        None, description='Screenline whose count is the throughput KPI.'
    )


PolylineItem = TypeAliasType(
    "PolylineItem", Annotated[list[float], Field(max_length=2, min_length=2)]
)


class TrajectorySpec(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    every_s: float = Field(
        ..., description='Sampling interval in seconds (a multiple of `dt_s`).'
    )
    from_t: float | None = Field(
        None, description='Only record from this time on (default: end of warm-up).'
    )
    s_from: float | None = Field(
        None,
        description='Only record agents with `s` in `[s_from, s_to]` (any ribbon).',
    )
    s_to: float | None = None


class WidthPoint(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    s: float
    y_max: float
    y_min: float


class BarrierSpec(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    closed: list[ClosedInterval] = Field(
        ..., description='Periods during which the barrier is closed.'
    )
    id: str
    ribbon: str
    s: float = Field(..., description='Station of the stop line on the ribbon.')


class EntrySpec(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    composition: Composition = Field(
        ...,
        description='Share of each vehicle type in the stream (normalised; count it at the\nsite — registration shares are not the stream, G§7.1).',
    )
    flow: list[FlowInterval] = Field(
        ...,
        description='Piecewise-constant flow: each interval applies from `from_s` until the next.',
    )
    id: str
    ribbon: str
    s: float | None = Field(0.0, description='Station of the entry on the ribbon.')


class OutputSpec(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    trajectories: TrajectorySpec | None = Field(
        None, description='Trajectory sampling; omit to write no trajectories.'
    )


class RibbonSpec(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    id: str
    next: str | None = Field(
        None, description='The ribbon traffic continues onto at the end, if any.'
    )
    polyline: list[PolylineItem] = Field(
        ...,
        description='Reference line vertices in world metres (UTM or local), at least two.',
    )
    width: list[WidthPoint] = Field(
        ...,
        description='Width profile: lateral bounds at stations along the ribbon\n(piecewise linear between stations, constant beyond the ends).',
    )


class DemandSpec(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    entries: list[EntrySpec]


class NetworkSpec(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    barriers: list[BarrierSpec] | None = Field(
        None,
        description='Stop lines that are closed during given periods (a level crossing, a\npolice barrier). Signals arrive in phase P7.',
    )
    ribbons: list[RibbonSpec]


class Scenario(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    demand: DemandSpec = Field(..., description='Arrivals at network entries.')
    description: str | None = Field('', description='Free-text description.')
    detectors: list[DetectorSpec] | None = Field(
        [],
        description='Virtual detectors (screenlines and areas).',
        validate_default=True,
    )
    dt_s: float | None = Field(0.1, description='Time step in seconds.')
    duration_s: float = Field(
        ..., description='Simulated duration including warm-up, in seconds.'
    )
    kpi: KpiSpec | None = Field(
        {'throughput_detector': None},
        description='Which detectors the headline KPIs use.',
        validate_default=True,
    )
    name: str = Field(..., description='Short identifier, used in run folder names.')
    network: NetworkSpec = Field(..., description='Road geometry.')
    outputs: OutputSpec | None = Field(
        {'trajectories': None}, description='Optional outputs.', validate_default=True
    )
    overrides: dict[str, ParamValue] | None = Field(
        {},
        description="Parameter overrides by register path, in the register entry's unit.",
        validate_default=True,
    )
    warmup_s: float | None = Field(
        0.0,
        description='Initial period excluded from KPIs and detector summaries, in seconds.',
    )


class Dist13(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    components: list[MixtureComponent]
    dist: Literal['mixture']


class MixtureComponent(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    dist: Dist
    weight: float


class Dist(
    RootModel[
        Dist1
        | Dist2
        | Dist3
        | Dist4
        | Dist5
        | Dist6
        | Dist7
        | Dist8
        | Dist9
        | Dist10
        | Dist11
        | Dist12
        | Dist13
    ]
):
    root: (
        Dist1
        | Dist2
        | Dist3
        | Dist4
        | Dist5
        | Dist6
        | Dist7
        | Dist8
        | Dist9
        | Dist10
        | Dist11
        | Dist12
        | Dist13
    ) = Field(
        ...,
        description='A univariate distribution.\n\nSeveral lognormal forms are accepted for readability in the register; they\nare converted to `(mu, sigma)` of `ln X` by [`Dist::canonical`].',
    )


class ParamValue(RootModel[float | Dist]):
    root: float | Dist = Field(
        ..., description='A parameter value: a plain number or a distribution.'
    )


Scenario.model_rebuild()
Dist13.model_rebuild()
MixtureComponent.model_rebuild()
