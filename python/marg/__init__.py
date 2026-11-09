"""marg: research API for the lane-free mixed-traffic simulator.

The simulation itself runs in the Rust core (``marg._core``); this package
orchestrates runs and experiments and analyses their Parquet outputs.
"""

from __future__ import annotations

from marg import analysis, backends, bundles, design, sensitivity, stats
from marg._core import version
from marg.experiment import Experiment, Outputs, Replications
from marg.register import Register
from marg.results import ExperimentData, open_experiment
from marg.run import RunResult, run
from marg.scenario import Scenario

__all__ = [
    "Experiment",
    "ExperimentData",
    "Outputs",
    "Register",
    "Replications",
    "RunResult",
    "Scenario",
    "analysis",
    "backends",
    "bundles",
    "design",
    "open_experiment",
    "run",
    "sensitivity",
    "stats",
    "version",
]
