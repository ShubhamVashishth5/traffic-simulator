# Generated from schema/scenario.schema.json by scripts/gen-schema.sh. Do not edit.

from __future__ import annotations

from pydantic import BaseModel, ConfigDict, Field


class Scenario(BaseModel):
    model_config = ConfigDict(
        extra='forbid',
    )
    description: str | None = Field('', description='Free-text description.')
    dt_s: float | None = Field(0.1, description='Time step in seconds.')
    duration_s: float = Field(
        ..., description='Simulated duration including warm-up, in seconds.'
    )
    name: str = Field(..., description='Short identifier, used in run folder names.')
    toy_arrivals_per_h: float = Field(
        ...,
        description='Placeholder demand for the phase-P0 skeleton: Poisson arrivals per hour.',
    )
