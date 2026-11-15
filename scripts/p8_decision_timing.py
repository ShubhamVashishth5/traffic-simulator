"""P8.1: why a violation hazard needs a clock, in numbers (ADR-0018, "The problem in numbers").

Agents wait 60 s at red. The hazard rises with waiting time and falls with the red still to
come: eta(t) = -9 + 0.05 t + 0.02 (60 - t), lambda = 1/s * sigma(eta) (about 1 in 15 run it).
For each decision interval it compares the share that has run by the end of red under

- sigma(eta) read as a probability at every tick (the naive rule),
- T1: a Bernoulli draw each tick with p = 1 - exp(-lambda dt),
- T2: the integrated-hazard clock of `marg-core/src/model/decision.rs` (one Exp(1)
  threshold per agent, trapezoid integration),

and, for T1 and T2, how many of the agents who run at 0.1 s also run at the coarser interval
(common random numbers). The checked-in Rust test
`model::decision::tests::cumulative_probability_is_invariant_to_the_decision_interval` asserts
the T2 rows. Needs only numpy; takes a few seconds.

Usage: uv run python scripts/p8_decision_timing.py [--agents 200000]
"""

from __future__ import annotations

import argparse

import numpy as np

RED = 60.0
B0, BT, BW, LMAX = -9.0, 0.02, 0.05, 1.0
INTERVALS = [0.1, 0.25, 0.5, 1.0, 2.0]


def rate(t: np.ndarray) -> np.ndarray:
    eta = B0 + BW * t + BT * (RED - t)
    return LMAX / (1.0 + np.exp(-eta))


def clock(dt: float, threshold: np.ndarray) -> np.ndarray:
    n = int(round(RED / dt))
    t = np.arange(1, n + 1) * dt
    r = rate(np.concatenate([[0.0], t]))
    cum = np.cumsum(0.5 * (r[:-1] + r[1:]) * dt)
    k = np.searchsorted(cum, threshold)
    return np.where(k < n, t[np.minimum(k, n - 1)], np.nan)


def bernoulli(dt: float, n_agents: int, seed: int) -> np.ndarray:
    n = int(round(RED / dt))
    t = np.arange(1, n + 1) * dt
    p = 1.0 - np.exp(-rate(t) * dt)
    rng = np.random.default_rng(seed)
    act = np.full(n_agents, np.nan)
    waiting = np.ones(n_agents, bool)
    for k in range(n):
        go = waiting & (rng.random(n_agents) < p[k])
        act[go] = t[k]
        waiting &= ~go
    return act


def naive(dt: float) -> float:
    t = np.arange(1, int(round(RED / dt)) + 1) * dt
    return float(1.0 - np.prod(1.0 - rate(t) / LMAX))


def overlap(a: np.ndarray, b: np.ndarray) -> float:
    ra, rb = ~np.isnan(a), ~np.isnan(b)
    return float(np.mean(ra & rb) / np.mean(ra | rb))


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--agents", type=int, default=200_000)
    n = ap.parse_args().agents
    fine = 1e-3
    lam = float(np.sum(rate((np.arange(int(RED / fine)) + 0.5) * fine)) * fine)
    print(f"exact share {1 - np.exp(-lam):.4f}; Monte-Carlo SE {np.sqrt(0.066 * 0.934 / n):.4f}")
    threshold = -np.log1p(-np.random.default_rng(1).random(n))
    base_clock, base_t1 = clock(INTERVALS[0], threshold), bernoulli(INTERVALS[0], n, 100)
    print("dt     naive   T1      T2      same as 0.1 s: T1   T2")
    for i, dt in enumerate(INTERVALS):
        c, b = clock(dt, threshold), bernoulli(dt, n, 200 + i)
        share = lambda x: float(np.mean(~np.isnan(x)))  # noqa: E731
        print(
            f"{dt:<5}  {naive(dt):.4f}  {share(b):.4f}  {share(c):.4f}  "
            f"{overlap(base_t1, b):.4f}              {overlap(base_clock, c):.4f}"
        )


if __name__ == "__main__":
    main()
