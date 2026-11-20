"""Named override sets ("bundles") for experiment arms (plan P3.3, P9).

A bundle is a plain ``{register path: value}`` mapping, usable as an arm::

    arms = {"observed": {}, "cooperative": marg.bundles.cooperative()}

Arms share seeds (common random numbers), so the same agent ID gets the same
draws in both arms and paired differences isolate the bundle's effect.
"""

from __future__ import annotations

from typing import Any

#: Proposed default quantile of the population kappa used for squeezing in the
#: cooperative twin. Pending the user's decision (ADR-0007, open question 1).
COOPERATIVE_KAPPA_QUANTILE = 0.25


def cooperative(kappa_quantile: float = COOPERATIVE_KAPPA_QUANTILE) -> dict[str, Any]:
    """The cooperative twin of an arm (social efficiency deficit, G§8.4):

    - every violation hazard off (``violations.enabled = 0``);
    - squeezing decisions use the kappa at ``kappa_quantile`` of the population
      distribution instead of each agent's own
      (``personas.cooperative.squeeze_kappa_quantile``);
    - at a junction that is dark or blinking without a major arm, vehicles take
      turns at the stop lines, first come first served
      (``road.signal.dark_first_come = 1``; ADR-0019 decision D1).

    Everything else, including each agent's other draws, stays as in the
    observed arm. The violation module (P8), the squeezing rule (P5) and dark
    junctions (P8.7) read these switches. What "cooperative" means is pending
    (ADR-0007; its dark-junction part is decided, ADR-0019 D1).
    """
    if not 0.0 <= kappa_quantile <= 1.0:
        raise ValueError("kappa_quantile must be in [0, 1]")
    return {
        "violations.enabled": 0.0,
        "personas.cooperative.squeeze_kappa_quantile": float(kappa_quantile),
        "road.signal.dark_first_come": 1.0,
    }
