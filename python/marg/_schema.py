# Generated from schema/scenario.schema.json by scripts/gen-schema.sh. Do not edit.

from __future__ import annotations

from enum import StrEnum
from typing import Annotated, Literal

from pydantic import BaseModel, ConfigDict, Field, RootModel
from typing_extensions import TypeAliasType


class ArmSpec(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    id: str
    in_: str | None = Field(
        None,
        alias='in',
        description='The ribbon that arrives here; its end is the stop line.',
    )
    out: str | None = Field(None, description='The ribbon that leaves from here.')


class AttractorSpec(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    id: str
    ribbon: str
    s_from: float
    s_to: float
    stop_rate_multiplier: float = Field(
        ...,
        description='Factor on the rate of stops per vehicle-kilometre (not on flow or\ncapacity).',
    )


class BusStopSpec(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    id: str
    length: float = Field(..., description='Length of the platform along the road (m).')
    ribbon: str
    s: float = Field(..., description='Station of the middle of the platform (m).')
    serving_share: float = Field(
        ..., description='Share of passing buses that stop here, 0–1.'
    )
    stop_rate_multiplier: float | None = Field(
        None,
        description='Factor on the stop-anywhere rate along the platform (autos and\ne-rickshaws gather at stops); omit for none.',
    )
    waiting_passenger_width: float | None = Field(
        None,
        description="Width of carriageway taken by passengers waiting on the road along\nthe platform (m); omit for the register's\n`road.bus.waiting_passenger_width`, 0 for none.",
    )


class ClosedInterval(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    from_s: float
    to_s: float


class ControlSpec1(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    kind: Literal['uncontrolled']
    major: list[str] | None = []


class CountdownState(StrEnum):
    none = 'none'
    working = 'working'
    broken = 'broken'


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


class EffectiveWidthSpec(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    bin_m: float = Field(..., description='Length of a bin along the road (m).')
    id: str
    ribbon: str
    s_from: float
    s_to: float


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


class HumpSpec(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    id: str
    length: float
    ribbon: str
    s: float = Field(
        ..., description='Station of its middle and its extent along the road (m).'
    )
    y_from: float | None = Field(
        None,
        description='Lateral extent; omit either for the carriageway edge. A hump that\nstops short of an edge leaves a gap narrow vehicles can use.',
    )
    y_to: float | None = None


class JunctionKind(StrEnum):
    box = 'box'
    split = 'split'
    roundabout = 'roundabout'


class KpiSpec(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    throughput_detector: str | None = Field(
        None, description='Screenline whose count is the throughput KPI.'
    )


class ObjectKind(StrEnum):
    obstacle = 'obstacle'
    parked_vehicle = 'parked_vehicle'
    parking_manoeuvre = 'parking_manoeuvre'
    waiting_passengers = 'waiting_passengers'
    pedestrians = 'pedestrians'
    pothole = 'pothole'
    hump = 'hump'
    water = 'water'


class ObjectResponse(StrEnum):
    block = 'block'
    ignore = 'ignore'


class Response(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    auto_rickshaw: ObjectResponse | None = None
    bicycle: ObjectResponse | None = None
    bus: ObjectResponse | None = None
    car: ObjectResponse | None = None
    cycle_rickshaw: ObjectResponse | None = None
    e_rickshaw: ObjectResponse | None = None
    lcv: ObjectResponse | None = None
    tractor: ObjectResponse | None = None
    truck: ObjectResponse | None = None
    two_wheeler: ObjectResponse | None = None


class PedestrianCrossingSpec(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    from_left_share: float = Field(
        ...,
        description='Share that starts from the left edge (the kerb); the rest start from\nthe right edge (the median).',
    )
    id: str
    per_hour: float = Field(
        ..., description='Crossings (groups) per hour over the whole stretch.'
    )
    ribbon: str
    s_from: float
    s_to: float


class PhaseSpec(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    all_red_s: float | None = 0.0
    amber_s: float
    arms: list[str] | None = None
    green_s: float
    movements: list[str] | None = None


class PotholeFieldSpec(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    id: str
    kerb_bias: float = Field(
        ...,
        description='How much more often they lie towards the kerb (left): 0 is uniform\nacross the road, 1 is none at the median edge rising linearly to\ntwice the mean at the kerb.',
    )
    per_km: float | None = Field(
        None,
        description='Potholes per kilometre of road; omit for `road.pothole.density`.',
    )
    ribbon: str
    s_from: float
    s_to: float


PolylineItem = TypeAliasType(
    "PolylineItem", Annotated[list[float], Field(max_length=2, min_length=2)]
)


class RoadSide(StrEnum):
    left = 'left'
    right = 'right'


class SurfaceSpec(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    gradient_percent: float | None = Field(
        None, description='Upgrade in percent (0 or omitted: level).'
    )
    id: str
    ribbon: str
    roughness_iri: float | None = Field(
        None,
        description='Roughness in IRI units above a smooth surface (omitted: smooth).',
    )
    s_from: float
    s_to: float


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


class UpstreamSignalSpec(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    cycle_s: float
    green_s: float
    offset_s: float | None = Field(
        0.0,
        description='Time into the cycle at simulated time 0 (s); green comes first.',
    )


class VehicleClass(StrEnum):
    bicycle = 'bicycle'
    cycle_rickshaw = 'cycle_rickshaw'
    two_wheeler = 'two_wheeler'
    e_rickshaw = 'e_rickshaw'
    auto_rickshaw = 'auto_rickshaw'
    car = 'car'
    lcv = 'lcv'
    bus = 'bus'
    truck = 'truck'
    tractor = 'tractor'


class WaterSpec(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    closed: bool | None = Field(
        False, description='Too deep to drive through: the patch takes its width.'
    )
    id: str
    ribbon: str
    s_from: float
    s_to: float
    speed_cap_kmh: float | None = Field(
        None, description='Speed vehicles can keep through it (km/h); omit if `closed`.'
    )
    y_from: float | None = Field(
        None, description='Lateral extent; omit either for the carriageway edge.'
    )
    y_to: float | None = None


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


class ControlSpec2(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    countdown: CountdownState | None = Field(
        'none',
        description='The countdown display. It changes nothing while every driver\ncomplies (P7); violations read it from P8 on.',
    )
    kind: Literal['signal']
    offset_s: float | None = Field(
        0.0, description='Time into the cycle at simulated time 0 (s).'
    )
    phases: list[PhaseSpec] = Field(
        ..., description='Stages in order; the cycle is the sum of their times.'
    )


class ControlSpec(RootModel[ControlSpec1 | ControlSpec2]):
    root: ControlSpec1 | ControlSpec2 = Field(
        ..., description='Who goes first at a junction.'
    )


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
    upstream_signal: UpstreamSignalSpec | None = Field(
        None,
        description='Feed the entry from a signal upstream instead of letting arrivals\nin as they come (G§7.3).',
    )


class ObjectSpec(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    clearance_as: VehicleClass | None = Field(
        None,
        description='The vehicle class whose lateral-clearance line and following\nmultipliers apply to this object (a parked car is passed like a\ncar). Omit for the pooled clearance line.',
    )
    id: str
    kind: ObjectKind | None = 'obstacle'
    length: float = Field(
        ..., description='Extent of the footprint along the ribbon (m).'
    )
    present: list[ClosedInterval] | None = Field(
        None,
        description='Periods during which the object is on the road; empty means always.',
    )
    response: Response | None = Field(
        None,
        description='How each vehicle class responds; classes not listed are blocked.',
    )
    ribbon: str
    s: float = Field(
        ..., description="Station of the footprint's centre on the ribbon (m)."
    )
    width: float = Field(
        ..., description='Extent of the footprint across the ribbon (m).'
    )
    y: float = Field(..., description="Lateral position of the footprint's centre (m).")


class OutputSpec(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    effective_width: list[EffectiveWidthSpec] | None = Field(
        None,
        description='Effective-width observers: how much of the carriageway traffic\nuses, against what road objects leave free (G§6.2).',
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


class JunctionSpec(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    arms: list[ArmSpec]
    centre: list[float] | None = Field(
        None,
        description='Roundabout: centre of the island (world metres), radius of the\nmiddle of the circulating carriageway and its width (m).',
        max_length=2,
        min_length=2,
    )
    control: ControlSpec | None = Field(
        {'kind': 'uncontrolled', 'major': []},
        description='Who goes first.',
        validate_default=True,
    )
    id: str
    kind: JunctionKind | None = 'box'
    ring_radius: float | None = None
    ring_width: float | None = None
    sign_visibility_distance: float | None = Field(
        None,
        description="Split: how far before the split its sign can be read (m); omit for\nthe register's `split.sign_visibility_distance`.",
    )
    turns: dict[str, dict[str, float]] = Field(
        ...,
        description='Turning shares: arriving arm → leaving arm → share (normalised per\narriving arm; count them at the site, G§7.5). A pair that is absent\nor zero gets no movement.',
    )
    u_turn_path: Dist | None = Field(
        None,
        description="U-turns (a movement back out by the arm it came in by, or whose\nleaving ribbon runs against the arriving one): how far from the\nmedian edge of the receiving carriageway a U-turning vehicle joins\nit (m). Omit for the register's `gap_acceptance.u_turn.path.*`.",
    )


class MixtureComponent(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    dist: Dist
    weight: float


class NetworkSpec(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    attractors: list[AttractorSpec] | None = Field(
        None,
        description='Places where vehicles stop to pick up and set down more often (a\nmetro exit, a market): G§6.2, ADR-0014.',
    )
    barriers: list[BarrierSpec] | None = Field(
        None,
        description='Stop lines that are closed during given periods (a level crossing, a\npolice barrier). Signals arrive in phase P7.',
    )
    bus_stops: list[BusStopSpec] | None = Field(
        None, description='Kerbside bus stops (G§6.3).'
    )
    humps: list[HumpSpec] | None = Field(None, description='Speed humps (G§6.8).')
    junctions: list[JunctionSpec] | None = Field(
        None,
        description='Junctions: places where ribbons meet and vehicles choose where to\ngo (G§2.4, §6.6, §6.7; ADR-0009, ADR-0016).',
    )
    objects: list[ObjectSpec] | None = Field(
        None,
        description='Road objects: things with a position and a footprint that vehicles\nreact to (G§6, plan/03-model-design.md §10). They act only through\nusable width, speed caps, gap supply and incentives; there is no\ncapacity or flow factor to set (ADR-0005).',
    )
    parking: list[ParkingSpec] | None = Field(
        None,
        description='Stretches of kerb where vehicles park (G§6.2). Each parked vehicle\nis a road object; parking and unparking block space for a while.',
    )
    pedestrian_crossings: list[PedestrianCrossingSpec] | None = Field(
        None, description='Stretches where pedestrians cross anywhere (G§6.4).'
    )
    pothole_fields: list[PotholeFieldSpec] | None = Field(
        None, description='Stretches with potholes (G§6.1).'
    )
    ribbons: list[RibbonSpec]
    surfaces: list[SurfaceSpec] | None = Field(
        None,
        description='Stretches with a gradient or a rough surface (G§6.9, G§6.1): they\nlower desired speeds by class and do nothing else.',
    )
    waterlogging: list[WaterSpec] | None = Field(
        None, description='Standing water (G§6.10).'
    )


class ParkingSpec(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    composition: Composition = Field(
        ..., description='Vehicle types that park here, with weights.'
    )
    cruising_share: float | None = Field(
        None,
        description='Share of passing vehicles (of the types that park here) that are\nlooking for a space and slow down along this stretch.',
    )
    double_parking_probability: float | None = Field(
        None,
        description='Probability that a parked vehicle has another standing beside it.',
    )
    id: str
    manoeuvres_per_100m_h: float | None = Field(
        None, description='Parking and unparking manoeuvres per 100 m of kerb per hour.'
    )
    occupancy: float = Field(
        ...,
        description='Share of the kerb length taken by parked vehicles at the start, 0–1\n(measure it from video or imagery).',
    )
    offset_from_kerb: Dist | None = Field(
        None, description='Gap between the kerb and a parked vehicle (m).'
    )
    ribbon: str
    s_from: float = Field(..., description='The stretch of kerb, by station (m).')
    s_to: float
    side: RoadSide | None = Field(
        'left', description='Which edge of the carriageway the vehicles park at.'
    )


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
JunctionSpec.model_rebuild()
MixtureComponent.model_rebuild()
NetworkSpec.model_rebuild()
ParkingSpec.model_rebuild()
