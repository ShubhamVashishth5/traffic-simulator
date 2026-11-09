"""Experimental designs over register parameters (plan P3.2).

A design is a list of *points*; each point gives a number to every factor. A
factor is a register path (or a ``scenario.`` path into the scenario
document). Bounds come from the register ``range`` unless given.

How a number becomes a parameter value (ADR-0006):

- scalar parameters take the number itself;
- distribution-valued parameters (desired speeds, time gaps, …) are rescaled so
  that their **mean** equals the number: every location and scale field is
  multiplied by ``x / mean``, which keeps the distribution's shape and
  coefficient of variation. The register ``range`` bounds the mean, so the same
  bounds apply.

Parameters with role ``fix`` are measured and never calibrated or swept
(G§8.1): a design that samples one is refused.
"""

from __future__ import annotations

import itertools
import math
from collections.abc import Iterable, Mapping, Sequence
from dataclasses import dataclass, field
from typing import Any

import numpy as np

from marg.register import Register, value_scaled
from marg.scenario import SCENARIO_PREFIX


class DesignError(ValueError):
    """A design that cannot be built (unknown path, fix parameter, bad bounds)."""


@dataclass(frozen=True)
class Mean:
    """An arm value meaning "this distribution, rescaled to mean ``x``" (the
    ADR-0006 rule that design factors use). A bare number in an arm replaces a
    distribution by a constant, exactly like ``marg run --set``."""

    x: float


def resolve_arm_value(path: str, v: Any, register: Register) -> Any:
    """Turn an arm value into an override (``Mean`` → rescaled distribution)."""
    if not isinstance(v, Mean):
        return v
    if path.startswith(SCENARIO_PREFIX):
        raise DesignError(f"`{path}`: Mean() applies to register distributions only")
    return bind(Factor(path, v.x, v.x), register).value(v.x)


def plain(v: Any) -> Any:
    """JSON/YAML-safe form of an arm value."""
    return {"mean_of": v.x} if isinstance(v, Mean) else v


@dataclass(frozen=True)
class Factor:
    """A design factor. ``lo``/``hi`` default to the register range."""

    path: str
    lo: float | None = None
    hi: float | None = None

    @classmethod
    def relative(cls, path: str, frac: float, register: Register) -> Factor:
        """Bounds at ``(1 ± frac)`` × the register value's mean, clipped to its range."""
        e = register[path]
        if e.mean is None:
            raise DesignError(f"`{path}` has no value to take relative bounds around")
        lo, hi = sorted((e.mean * (1 - frac), e.mean * (1 + frac)))
        if e.range is not None:
            lo, hi = max(lo, e.range[0]), min(hi, e.range[1])
        return cls(path, lo, hi)


@dataclass(frozen=True)
class BoundFactor:
    """A factor checked against the register, with its bounds and transform."""

    path: str
    lo: float
    hi: float
    #: ``value`` (the number itself) or ``mean`` (rescale the distribution).
    transform: str
    base: Any = field(default=None, compare=False)
    base_mean: float | None = None
    unit: str = ""

    @property
    def is_scenario(self) -> bool:
        return self.path.startswith(SCENARIO_PREFIX)

    def value(self, x: float) -> Any:
        """The override (register unit) or scenario value for number ``x``."""
        x = float(x)
        if self.transform == "mean":
            assert self.base_mean is not None
            return value_scaled(self.base, x / self.base_mean)
        return x

    def describe(self) -> dict[str, Any]:
        return {
            "path": self.path,
            "lo": self.lo,
            "hi": self.hi,
            "transform": self.transform,
            "unit": self.unit,
        }


def bind(factor: Factor, register: Register) -> BoundFactor:
    """Check ``factor`` against the register and fill in bounds and transform."""
    if factor.path.startswith(SCENARIO_PREFIX):
        if factor.lo is None or factor.hi is None:
            raise DesignError(f"scenario factor `{factor.path}` needs explicit bounds")
        return _checked(BoundFactor(factor.path, factor.lo, factor.hi, "value"))
    if factor.path not in register:
        raise DesignError(f"`{factor.path}` is not in the register")
    e = register[factor.path]
    if e.role == "fix":
        raise DesignError(
            f"`{factor.path}` has role `fix` (measured, never calibrated or swept, G§8.1): "
            "it cannot be a design factor"
        )
    lo = factor.lo if factor.lo is not None else (e.range[0] if e.range else None)
    hi = factor.hi if factor.hi is not None else (e.range[1] if e.range else None)
    if lo is None or hi is None:
        raise DesignError(f"`{factor.path}` has no register range: give bounds")
    if e.range is not None and (lo < e.range[0] or hi > e.range[1]):
        raise DesignError(
            f"`{factor.path}` bounds [{lo}, {hi}] fall outside its register range {list(e.range)}"
        )
    if e.is_dist:
        mean = e.mean
        if mean is None or mean == 0 or not math.isfinite(mean):
            raise DesignError(f"`{factor.path}`: cannot rescale a distribution with mean {mean}")
        return _checked(BoundFactor(factor.path, lo, hi, "mean", e.value, mean, e.unit))
    return _checked(BoundFactor(factor.path, lo, hi, "value", e.value, e.mean, e.unit))


def _checked(b: BoundFactor) -> BoundFactor:
    if not (math.isfinite(b.lo) and math.isfinite(b.hi)) or b.lo > b.hi:
        raise DesignError(f"`{b.path}`: bad bounds [{b.lo}, {b.hi}]")
    return b


FactorSpec = str | Factor | tuple[str, float, float]


def _factors(spec: Iterable[FactorSpec] | Mapping[str, Sequence[float]]) -> list[Factor]:
    if isinstance(spec, Mapping):
        out = []
        for p, b in spec.items():
            if b is None:
                out.append(Factor(p))
            else:
                lo, hi = b
                out.append(Factor(p, float(lo), float(hi)))
        return out
    out = []
    for f in spec:
        if isinstance(f, Factor):
            out.append(f)
        elif isinstance(f, str):
            out.append(Factor(f))
        else:
            p, lo, hi = f
            out.append(Factor(p, float(lo), float(hi)))
    return out


class Design:
    """Base class: subclasses give the factors and the point matrix."""

    kind = "design"

    def __init__(self, factors: Iterable[FactorSpec] | Mapping[str, Any] = ()) -> None:
        self.factors = _factors(factors)
        paths = [f.path for f in self.factors]
        if len(set(paths)) != len(paths):
            raise DesignError(f"duplicate factors in {paths}")

    def bind(self, register: Register) -> list[BoundFactor]:
        return [bind(f, register) for f in self.factors]

    def matrix(self, bound: list[BoundFactor]) -> np.ndarray:
        """Points × factors, in factor units (register unit; mean for distributions)."""
        raise NotImplementedError

    def args(self) -> dict[str, Any]:
        """Design arguments (besides factors) for ``experiment.yaml``."""
        return {}

    def describe(self, bound: list[BoundFactor]) -> dict[str, Any]:
        return {"type": self.kind, **self.args(), "factors": [b.describe() for b in bound]}

    def problem(self, bound: list[BoundFactor]) -> dict[str, Any]:
        """The SALib problem dictionary."""
        return {
            "num_vars": len(bound),
            "names": [b.path for b in bound],
            "bounds": [[b.lo, b.hi] for b in bound],
        }


class Single(Design):
    """One point and no factors: the base scenario as it is."""

    kind = "single"

    def __init__(self) -> None:
        super().__init__(())

    def matrix(self, bound: list[BoundFactor]) -> np.ndarray:
        return np.zeros((1, 0))


class Grid(Design):
    """The Cartesian product of explicit levels: ``Grid({path: [v1, v2, …]})``.
    The last factor varies fastest."""

    kind = "grid"

    def __init__(self, levels: Mapping[str, Sequence[float]]) -> None:
        self.levels = {p: [float(v) for v in vs] for p, vs in levels.items()}
        for p, vs in self.levels.items():
            if not vs:
                raise DesignError(f"`{p}` has no levels")
        super().__init__({p: (min(vs), max(vs)) for p, vs in self.levels.items()})

    def matrix(self, bound: list[BoundFactor]) -> np.ndarray:
        rows = list(itertools.product(*(self.levels[b.path] for b in bound)))
        return np.array(rows, dtype=float).reshape(len(rows), len(bound))

    def args(self) -> dict[str, Any]:
        return {"levels": self.levels}


class FullFactorial(Design):
    """Every combination of ``levels`` evenly spaced levels per factor, from lo
    to hi. ``levels`` is one number or a ``{path: n}`` mapping."""

    kind = "full_factorial"

    def __init__(
        self,
        factors: Iterable[FactorSpec] | Mapping[str, Any],
        levels: int | Mapping[str, int] = 2,
    ) -> None:
        super().__init__(factors)
        self.levels = levels

    def _n(self, path: str) -> int:
        n = self.levels if isinstance(self.levels, int) else self.levels.get(path, 2)
        if n < 2:
            raise DesignError(f"`{path}`: a factorial needs at least 2 levels")
        return int(n)

    def matrix(self, bound: list[BoundFactor]) -> np.ndarray:
        axes = [np.linspace(b.lo, b.hi, self._n(b.path)) for b in bound]
        rows = list(itertools.product(*axes))
        return np.array(rows, dtype=float).reshape(len(rows), len(bound))

    def args(self) -> dict[str, Any]:
        return {"levels": self.levels if isinstance(self.levels, int) else dict(self.levels)}


class FractionalFactorial(Design):
    """Two-level fractional factorial at the factor bounds.

    With ``generators`` (``{"D": "AB", "E": "AC"}``: factor letters by position,
    A = first factor) it is the classic 2^(k−p) design: the base factors run a
    full factorial and each generated factor is the product of its generator's
    columns. Without, it is SALib's Hadamard design with fold-over (main effects
    clear of two-factor interactions).
    """

    kind = "fractional_factorial"

    def __init__(
        self,
        factors: Iterable[FactorSpec] | Mapping[str, Any],
        generators: Mapping[str, str] | None = None,
    ) -> None:
        super().__init__(factors)
        self.generators = dict(generators) if generators else None

    def coded(self, k: int) -> np.ndarray:
        """The design in ±1 coding."""
        if self.generators is None:
            from SALib.sample import ff

            problem = {"num_vars": k, "names": [f"x{i}" for i in range(k)], "bounds": [[-1, 1]] * k}
            return ff.sample(problem, seed=1)[:, :k]
        letters = [chr(ord("A") + i) for i in range(k)]
        gen = {g.upper(): w.upper() for g, w in self.generators.items()}
        unknown = set(gen) - set(letters)
        if unknown:
            raise DesignError(f"generators name factors {sorted(unknown)} beyond the {k} given")
        base = [x for x in letters if x not in gen]
        full = np.array(list(itertools.product([-1.0, 1.0], repeat=len(base))))
        cols = {x: full[:, i] for i, x in enumerate(base)}
        for g, word in gen.items():
            if not word or any(c not in cols for c in word):
                raise DesignError(f"generator {g}={word} must use base factors {base}")
            cols[g] = np.prod([cols[c] for c in word], axis=0)
        return np.column_stack([cols[x] for x in letters])

    def matrix(self, bound: list[BoundFactor]) -> np.ndarray:
        c = self.coded(len(bound))
        lo = np.array([b.lo for b in bound])
        hi = np.array([b.hi for b in bound])
        return lo + (c + 1) / 2 * (hi - lo)

    def args(self) -> dict[str, Any]:
        return {"generators": self.generators}


class LatinHypercube(Design):
    """``n`` points, one per stratum of every factor (SALib ``latin``)."""

    kind = "latin_hypercube"

    def __init__(
        self, factors: Iterable[FactorSpec] | Mapping[str, Any], n: int, seed: int = 1
    ) -> None:
        super().__init__(factors)
        if n < 1:
            raise DesignError("n must be positive")
        self.n, self.seed = int(n), int(seed)

    def matrix(self, bound: list[BoundFactor]) -> np.ndarray:
        from SALib.sample import latin

        return np.asarray(latin.sample(self.problem(bound), self.n, seed=self.seed), dtype=float)

    def args(self) -> dict[str, Any]:
        return {"n": self.n, "seed": self.seed}


class Morris(Design):
    """Morris elementary-effects screening: ``trajectories`` × (k + 1) points on
    a ``levels``-level grid (SALib ``morris``). Analyse with
    :func:`marg.sensitivity.morris`."""

    kind = "morris"

    def __init__(
        self,
        factors: Iterable[FactorSpec] | Mapping[str, Any],
        trajectories: int = 10,
        levels: int = 4,
        seed: int = 1,
        optimal_trajectories: int | None = None,
    ) -> None:
        super().__init__(factors)
        if levels % 2:
            raise DesignError("Morris levels must be even")
        self.trajectories, self.levels, self.seed = int(trajectories), int(levels), int(seed)
        self.optimal_trajectories = optimal_trajectories

    def matrix(self, bound: list[BoundFactor]) -> np.ndarray:
        from SALib.sample import morris

        return np.asarray(
            morris.sample(
                self.problem(bound),
                self.trajectories,
                num_levels=self.levels,
                optimal_trajectories=self.optimal_trajectories,
                seed=self.seed,
            ),
            dtype=float,
        )

    def args(self) -> dict[str, Any]:
        return {
            "trajectories": self.trajectories,
            "levels": self.levels,
            "seed": self.seed,
            "optimal_trajectories": self.optimal_trajectories,
        }


class Saltelli(Design):
    """Saltelli's scheme for Sobol indices: ``n`` × (k + 2) points, or
    ``n`` × (2k + 2) with second-order indices (SALib ``sobol``). ``n`` must be
    a power of two. Analyse with :func:`marg.sensitivity.sobol`."""

    kind = "saltelli"

    def __init__(
        self,
        factors: Iterable[FactorSpec] | Mapping[str, Any],
        n: int = 1024,
        second_order: bool = False,
        seed: int = 1,
    ) -> None:
        super().__init__(factors)
        if n < 2 or n & (n - 1):
            raise DesignError(f"Saltelli n must be a power of two, got {n}")
        self.n, self.second_order, self.seed = int(n), bool(second_order), int(seed)

    def matrix(self, bound: list[BoundFactor]) -> np.ndarray:
        from SALib.sample import sobol

        return np.asarray(
            sobol.sample(
                self.problem(bound), self.n, calc_second_order=self.second_order, seed=self.seed
            ),
            dtype=float,
        )

    def args(self) -> dict[str, Any]:
        return {"n": self.n, "second_order": self.second_order, "seed": self.seed}


def from_description(d: Mapping[str, Any]) -> Design:
    """Rebuild a design from its ``experiment.yaml`` description."""
    kind = d["type"]
    factors = [Factor(f["path"], f["lo"], f["hi"]) for f in d.get("factors", [])]
    if kind == "single":
        return Single()
    if kind == "grid":
        return Grid(d["levels"])
    if kind == "full_factorial":
        return FullFactorial(factors, d.get("levels", 2))
    if kind == "fractional_factorial":
        return FractionalFactorial(factors, d.get("generators"))
    if kind == "latin_hypercube":
        return LatinHypercube(factors, d["n"], d.get("seed", 1))
    if kind == "morris":
        return Morris(
            factors, d["trajectories"], d["levels"], d.get("seed", 1), d.get("optimal_trajectories")
        )
    if kind == "saltelli":
        return Saltelli(factors, d["n"], d.get("second_order", False), d.get("seed", 1))
    raise DesignError(f"unknown design type `{kind}`")
