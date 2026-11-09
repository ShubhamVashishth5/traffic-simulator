"""P3.2: designs give the right sample counts and bounds; fix parameters are refused."""

from __future__ import annotations

import numpy as np
import pytest
from marg.design import (
    DesignError,
    Factor,
    FractionalFactorial,
    FullFactorial,
    Grid,
    LatinHypercube,
    Morris,
    Saltelli,
    Single,
    bind,
    from_description,
)
from marg.register import value_mean

T_CAR = "interaction.following.time_gap.car"  # lognormal, range [0.05, 4.0] s
POLITE = "interaction.lateral.politeness"  # scalar
V0_BIKE = "vehicles.free_flow_speed.motorcycle"  # truncated normal


def points(design, register):
    bound = design.bind(register)
    return bound, design.matrix(bound)


def in_bounds(x: np.ndarray, bound) -> bool:
    lo = np.array([b.lo for b in bound])
    hi = np.array([b.hi for b in bound])
    return bool(np.all(x >= lo - 1e-12) and np.all(x <= hi + 1e-12))


def test_single(register) -> None:
    _, x = points(Single(), register)
    assert x.shape == (1, 0)


def test_grid(register) -> None:
    b, x = points(Grid({POLITE: [0.1, 0.3], T_CAR: [0.8, 1.0, 1.2]}), register)
    assert x.shape == (6, 2)
    assert {tuple(r) for r in x} == {(p, t) for p in (0.1, 0.3) for t in (0.8, 1.0, 1.2)}


def test_full_factorial_bounds_from_register(register) -> None:
    b, x = points(FullFactorial([POLITE, T_CAR], levels=3), register)
    assert x.shape == (9, 2)
    assert (b[1].lo, b[1].hi) == register[T_CAR].range
    assert sorted(set(x[:, 1])) == pytest.approx([0.05, 2.025, 4.0])
    assert in_bounds(x, b)


def test_fractional_factorial_with_generators(register) -> None:
    f = [Factor(POLITE, 0.1, 0.3), Factor(T_CAR, 0.8, 1.2), Factor(V0_BIKE, 40, 60)]
    d = FractionalFactorial(f, generators={"C": "AB"})
    b, x = points(d, register)
    assert x.shape == (4, 3)  # 2^(3-1)
    c = d.coded(3)
    assert np.array_equal(c[:, 2], c[:, 0] * c[:, 1])
    for i, fb in enumerate(b):
        assert set(x[:, i]) == {fb.lo, fb.hi}


def test_fractional_factorial_hadamard(register) -> None:
    f = [Factor(POLITE, 0.1, 0.3), Factor(T_CAR, 0.8, 1.2), Factor(V0_BIKE, 40, 60)]
    b, x = points(FractionalFactorial(f), register)
    assert x.shape[0] == 8  # 2 x Hadamard(4) with fold-over
    assert np.all(np.isin(x[:, 0], [0.1, 0.3]))
    assert all(np.sum(x[:, i] == b[i].hi) == x.shape[0] // 2 for i in range(3))  # balanced


def test_latin_hypercube(register) -> None:
    b, x = points(LatinHypercube({POLITE: (0.0, 1.0), T_CAR: (0.5, 1.5)}, n=20, seed=3), register)
    assert x.shape == (20, 2)
    assert in_bounds(x, b)
    for i, fb in enumerate(b):
        strata = np.floor((x[:, i] - fb.lo) / (fb.hi - fb.lo) * 20).clip(0, 19)
        assert sorted(strata) == list(range(20))  # one point per stratum


def test_morris(register) -> None:
    f = {POLITE: (0.0, 0.6), T_CAR: (0.6, 1.4), V0_BIKE: (40, 60), "demand.min_headway.car": None}
    b, x = points(Morris(f, trajectories=6, levels=4, seed=2), register)
    assert x.shape == (6 * (4 + 1), 4)
    assert in_bounds(x, b)
    for i, fb in enumerate(b):
        grid = fb.lo + np.arange(4) / 3 * (fb.hi - fb.lo)
        assert np.all(np.isclose(x[:, i][:, None], grid[None, :]).any(axis=1))


def test_saltelli(register) -> None:
    f = {POLITE: (0.0, 0.6), T_CAR: (0.6, 1.4), V0_BIKE: (40, 60)}
    b, x = points(Saltelli(f, n=64), register)
    assert x.shape == (64 * (3 + 2), 3)
    assert in_bounds(x, b)
    _, x2 = points(Saltelli(f, n=64, second_order=True), register)
    assert x2.shape == (64 * (2 * 3 + 2), 3)
    with pytest.raises(DesignError, match="power of two"):
        Saltelli(f, n=100)


def test_designs_are_deterministic(register) -> None:
    f = {POLITE: (0.0, 0.6), T_CAR: (0.6, 1.4)}
    for d in (LatinHypercube(f, 10, seed=4), Morris(f, 5, seed=4), Saltelli(f, 16, seed=4)):
        _, a = points(d, register)
        _, b = points(from_description(d.describe(d.bind(register))), register)
        assert np.array_equal(a, b)


def test_fix_parameters_are_refused(register) -> None:
    assert register["vehicles.length.hatchback"].role == "fix"
    for design in (
        Grid({"vehicles.length.hatchback": [3.5, 4.0]}),
        Morris(["vehicles.length.hatchback", POLITE]),
        Saltelli(["vehicles.length.hatchback"], n=8),
    ):
        with pytest.raises(DesignError, match="role `fix`"):
            design.bind(register)


def test_bounds_are_checked(register) -> None:
    with pytest.raises(DesignError, match="outside its register range"):
        bind(Factor(T_CAR, 0.01, 1.0), register)
    with pytest.raises(DesignError, match="not in the register"):
        bind(Factor("no.such.param", 0, 1), register)
    with pytest.raises(DesignError, match="explicit bounds"):
        bind(Factor("scenario.demand.entries.0.flow.0.veh_per_h"), register)


def test_distribution_factors_set_the_mean(register) -> None:
    for path, x in ((V0_BIKE, 42.0), (T_CAR, 1.3)):
        b = bind(Factor(path), register)
        assert b.transform == "mean"
        v = b.value(x)
        assert isinstance(v, dict)
        assert value_mean(v) == pytest.approx(x, rel=1e-9)
    assert bind(Factor(POLITE), register).value(0.4) == 0.4


def test_relative_bounds(register) -> None:
    f = Factor.relative(V0_BIKE, 0.2, register)
    m = register[V0_BIKE].mean
    assert (f.lo, f.hi) == pytest.approx((0.8 * m, 1.2 * m))
