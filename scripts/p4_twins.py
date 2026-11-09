"""P4.8 check data: the P3.3 twin arms of the reference corridor, with 1 Hz trajectories,
for the viewer's Compare view.

Arms (one seed, common random numbers): `observed`; `cooperative`
(`marg.bundles.cooperative()`, ADR-0007: its switches are not read before P5/P8, so this arm
simulates exactly what `observed` does); and `fast_cars` (car desired-speed means x1.15), whose
trajectories do diverge. Open them with `marg serve --runs <root>` and the Compare tab.

Usage: uv run python scripts/p4_twins.py [--root runs] [--duration 1800] [--seed 7]
"""

from __future__ import annotations

import argparse
from pathlib import Path

import marg
from marg.design import Mean

REPO = Path(__file__).resolve().parents[1]
CARS = ["hatchback", "sedan", "suv"]
SPEEDUP = 1.15


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--root", default=str(REPO / "runs"))
    ap.add_argument("--duration", type=float, default=1800.0)
    ap.add_argument("--seed", type=int, default=7)
    ap.add_argument("--workers", type=int, default=3)
    a = ap.parse_args()
    sc = marg.Scenario.load(REPO / "scenarios/corridor_2km.yaml")
    sc.data["duration_s"] = a.duration
    reg = marg.Register(REPO / "params/base")
    fast = {
        f"vehicles.free_flow_speed.{t}": Mean(reg[f"vehicles.free_flow_speed.{t}"].mean * SPEEDUP)
        for t in CARS
    }
    exp = marg.Experiment(
        "p4_twins",
        sc,
        arms={"observed": {}, "cooperative": marg.bundles.cooperative(), "fast_cars": fast},
        replications=marg.Replications(n=1),
        outputs=marg.experiment.Outputs(trajectories={"every_s": 1.0}),
        seed=a.seed,
        root=a.root,
        params_dir=REPO / "params/base",
    )
    exp.run(marg.backends.Local(workers=a.workers))
    print(exp.data().index().select("arm", "path", "status", "output_hash"))


if __name__ == "__main__":
    main()
