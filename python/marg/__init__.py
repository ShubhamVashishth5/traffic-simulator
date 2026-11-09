"""marg: research API for the lane-free mixed-traffic simulator.

The simulation itself runs in the Rust core (``marg._core``); this package
orchestrates runs and experiments and analyses their Parquet outputs.
"""

from __future__ import annotations

from marg._core import version
from marg.run import RunResult, run

__all__ = ["RunResult", "run", "version"]
