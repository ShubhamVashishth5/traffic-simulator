"""Gap-acceptance estimators (P7.4) on synthetic drivers with known critical gaps."""

from __future__ import annotations

import numpy as np
import pytest
from marg import gaps


def survey(n: int, median: float, sigma: float, rate: float, seed: int):
    """Drivers with lognormal critical gaps facing Poisson gaps: each turns down gaps until one
    is at least its critical gap. Returns (accepted, largest rejected, all rejected)."""
    rng = np.random.default_rng(seed)
    tc = median * np.exp(sigma * rng.standard_normal(n))
    acc, big, rej = [], [], []
    for t in tc:
        largest = 0.0
        while True:
            g = rng.exponential(1.0 / rate)
            if g >= t:
                acc.append(g)
                break
            rej.append(g)
            largest = max(largest, g)
        big.append(largest)
    return np.array(acc), np.array(big), np.array(rej)


def test_mle_recovers_a_lognormal_population():
    acc, big, _ = survey(800, 4.4, 0.2, 1 / 5.0, 1)
    fit = gaps.mle(acc, big)
    assert fit.drivers == 800
    assert abs(fit.median - 4.4) < 3 * fit.median_se + 0.02, fit
    assert abs(fit.sigma - 0.2) < 3 * fit.sigma_se + 0.01, fit
    assert 0 < fit.median_se < 0.2


def test_mle_takes_an_unopposed_gap_as_censored():
    acc, big, _ = survey(600, 8.15, 0.2, 1 / 6.0, 2)
    # Half the drivers found nothing coming: their taken gap is unbounded.
    acc = acc.copy()
    acc[::2] = np.inf
    fit = gaps.mle(acc, big)
    assert abs(fit.median - 8.15) < 3 * fit.median_se + 0.05, fit


def test_raff_crossing_and_its_bias():
    # A step example: accepted 3, 5, 7; rejected 2, 4, 6. Below 4, one accepted is shorter
    # (3) and two rejected are longer (4, 6) at t in (3, 4); at t in (4, 5), one shorter and
    # one longer: they meet first at t = 4.
    assert gaps.raff([3, 5, 7], [2, 4, 6]) == pytest.approx(4.0)
    # On drivers with a fixed threshold Raff's value is near the population median when
    # gaps are plentiful, and biased when many short gaps are turned down (Brilon et al.
    # 1999): it is reported beside the MLE, not used for recovery.
    acc, _, rej = survey(800, 4.4, 0.2, 1 / 8.0, 3)
    assert 3.5 < gaps.raff(acc, rej) < 5.5
