# Generated from schema/register_file.schema.json by scripts/gen-schema.sh. Do not edit.

from __future__ import annotations

from enum import StrEnum
from typing import Annotated, Literal

from pydantic import BaseModel, ConfigDict, Field, RootModel, conint
from typing_extensions import TypeAliasType


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


class Grade1(StrEnum):
    A_B = 'A/B'
    B_C = 'B/C'


class Grade(RootModel[Grade1 | Literal['A'] | Literal['B'] | Literal['C']]):
    root: Grade1 | Literal['A'] | Literal['B'] | Literal['C'] = Field(
        ...,
        description='How far a value can be trusted (G: "give every parameter a trust grade").',
    )


class Role(StrEnum):
    fix = 'fix'
    fit = 'fit'
    sweep = 'sweep'
    scenario = 'scenario'


class Unit1(StrEnum):
    m = 'm'
    cm = 'cm'
    km = 'km'
    s = 's'
    min = 'min'
    h = 'h'
    day = 'day'
    m_s = 'm/s'
    km_h = 'km/h'
    m_s2 = 'm/s2'
    rad = 'rad'
    field_1_h = '1/h'
    field_1_km = '1/km'
    kW = 'kW'
    kg = 'kg'


class Unit(
    RootModel[
        Unit1
        | Literal['1']
        | Literal['%']
        | Literal['cm/(km/h)']
        | Literal['(km/h)/%']
        | Literal['(km/h)/IRI']
        | Literal['hour_of_day']
    ]
):
    root: (
        Unit1
        | Literal['1']
        | Literal['%']
        | Literal['cm/(km/h)']
        | Literal['(km/h)/%']
        | Literal['(km/h)/IRI']
        | Literal['hour_of_day']
    ) = Field(..., description='A register unit with its conversion to SI.')


class RegisterFile(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    description: str | None = ''
    entries: dict[str, ParamEntry] = Field(
        ...,
        description='Entries keyed by their path below the group, e.g. `following.time_gap.car`.',
    )
    group: str = Field(
        ..., description='Path prefix of every entry in the file, e.g. `interaction`.'
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


class ParamEntry(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    grade: Grade = Field(..., description='Trust grade.')
    guide: str = Field(..., description='Guide section(s), e.g. `"§2.1"`.')
    live: bool | None = Field(
        False, description='Whether the live viewer may change it during a run.'
    )
    notes: str | None = Field(
        None,
        description='Assumptions, caveats, how a range was turned into a distribution.',
    )
    range: list[float] | None = Field(
        None,
        description='Hard bounds `[min, max]` for overrides and sweeps, in `unit`.',
        max_length=2,
        min_length=2,
    )
    register_row: str | None = Field(
        None,
        description='Which row of the guide\'s master register (§9) this entry implements,\ne.g. `"9.2/desired_time_gap"`. Used to check every row is covered.',
    )
    role: Role = Field(..., description='Fix, Fit, Sweep or Scenario.')
    source: str = Field(..., description='Where the value comes from.')
    stage: conint(ge=0, le=255) | None = Field(
        None,
        description='Calibration stage 0–7 (G§8.3). Required for `fit`, absent otherwise.',
    )
    unit: Unit = Field(..., description='Unit the value (and `range`) are written in.')
    value: ParamValue | None = Field(
        None,
        description='The starting value or distribution, in `unit`. `null` means "not yet\nknown": the parameter must be measured, or set by a scenario or sweep,\nbefore any module may use it.',
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


RegisterFile.model_rebuild()
Dist13.model_rebuild()
MixtureComponent.model_rebuild()
ParamEntry.model_rebuild()
