"""P5.8: stage-3 (lateral behaviour) identifiability screen and synthetic recovery.

1. **Screen.** For the main stage-3 `fit` parameters, the change of every
   stage-3 statistic (marg.stage3) between −25% and +25% of the register value,
   in standard errors of the difference. A parameter that moves no statistic
   by more than a few standard errors cannot be recovered from these data.
2. **Recovery** (Carrella 2021). "Observed" statistics are simulated with
   known true values at one set of seeds and fitted at another; each true value
   should fall inside its 95% CI. The fit is the response-surface method of
   marg.calibrate: these statistics respond to the parameters through many
   interacting vehicles, so they are too rough for finite differences.

Stages 1 and 2 are frozen at the register's values in a temporary calibration
root (stage 3 may not start before them). Nothing is tuned toward a validation
target: the data are synthetic.

Usage: uv run python scripts/p5_stage3_recovery.py [--seeds 6] [--out reports/phases/P5_stage3.json]
"""

from __future__ import annotations

import argparse
import json
import tempfile
import time
from pathlib import Path

import numpy as np
import yaml
from marg import stage3
from marg.calibrate import Simulator, Stage, recovery_test
from marg.register import Register, value_mean
from marg.scenario import Scenario

REPO = Path(__file__).resolve().parents[1]
PARAMS = REPO / "params/base"
SECTION = stage3.Section(s_line=600, s_from=400, s_to=800, window=60, t_from=120)
CLASSES = ["two_wheeler", "car", "auto_rickshaw"]
L = "interaction.lateral."
SCREEN = [
    L + "clearance.min_offset",
    L + "clearance.pair.intercept.car_car",
    L + "clearance.pair.slope.car_car",
    L + "clearance.pair.intercept.two_wheeler_car",
    L + "braking_scale",
    L + "steering_scale",
    L + "steering_sensitivity",
    L + "time_constant",
    L + "max_heading",
    L + "politeness",
    L + "safe_decel",
    L + "switch_threshold",
    L + "comfort_weight",
    L + "move_cost",
    L + "edge_clearance",
    L + "creep_speed.two_wheeler",
]
#: Recovery: clearance coefficients and lateral scales, with true values away
#: from the register's (the fit starts at the register's values).
TRUTH = {
    L + "clearance.min_offset": 100.0,
    L + "clearance.pair.intercept.car_car": 130.0,
    L + "steering_scale": 0.38,
    L + "braking_scale": 0.12,
}


def frozen_root(tmp: Path) -> Path:
    root = tmp / "calibration"
    root.mkdir()
    for k in (1, 2):
        doc = {
            "stage": k,
            "status": "frozen",
            "params": {},
            "notes": "register values (synthetic test)",
        }
        (root / f"stage_{k}.yaml").write_text(yaml.safe_dump(doc))
    return root


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--seeds", type=int, default=6)
    ap.add_argument("--out", type=Path, default=REPO / "reports/phases/P5_stage3.json")
    ap.add_argument("--skip-screen", action="store_true")
    args = ap.parse_args()
    scenario = Scenario.load(REPO / "scenarios/stage3_recovery.yaml")
    stat = stage3.statistic(SECTION, CLASSES)
    reg = Register(PARAMS)
    result: dict = {"section": SECTION.__dict__, "classes": CLASSES, "seeds": args.seeds}
    with tempfile.TemporaryDirectory() as tmp:
        root = frozen_root(Path(tmp))

        def sim(seeds):
            return Simulator(
                scenario, seeds, stat, params_dir=PARAMS, calibration_dir=root, trajectories=True
            )

        fit_seeds = list(range(1, args.seeds + 1))
        if not args.skip_screen:
            screen = []
            run = sim(fit_seeds)
            for p in SCREEN:
                base = value_mean(reg[p].value)
                lo, hi = run({p: 0.75 * base}), run({p: 1.25 * base})
                z = (hi.values - lo.values) / np.sqrt(hi.se**2 + lo.se**2)
                order = np.argsort(-np.abs(z))
                screen.append(
                    {
                        "path": p,
                        "value": base,
                        "max_abs_z": float(np.max(np.abs(z))),
                        "top": [[hi.names[i], float(z[i])] for i in order[:3]],
                        "z": dict(zip(hi.names, map(float, z), strict=True)),
                    }
                )
                print(
                    f"{p.removeprefix(L):40s} max|z| {screen[-1]['max_abs_z']:5.1f}  {screen[-1]['top']}"
                )
            result["screen"] = screen
            result["statistics"] = run({}).to_dict()

        st = Stage(3, root, PARAMS)
        bounds = {
            p: (0.6 * value_mean(reg[p].value), 1.4 * value_mean(reg[p].value)) for p in TRUTH
        }
        t0 = time.time()
        res, report = recovery_test(
            st,
            TRUTH,
            sim,
            observe_seeds=list(range(101, 101 + args.seeds)),
            fit_seeds=fit_seeds,
            bounds=bounds,
            method="response_surface",
            max_evaluations=200,
        )
        report["minutes"] = (time.time() - t0) / 60
        report["evaluations"] = res.evaluations
        report["cost"] = res.cost
        report["jacobian_condition"] = res.jacobian_condition
        report["success"] = res.success
        report["message"] = res.message
        report["start"] = {p: value_mean(reg[p].value) for p in TRUTH}
        result["recovery"] = report
        for r in report["params"]:
            print(
                f"{r['path'].removeprefix(L):36s} truth {r['truth']:8.3f} est {r['estimate']:8.3f} "
                f"95% CI [{r['ci_lo']:.3f}, {r['ci_hi']:.3f}] covered {r['covered']}"
            )
    args.out.write_text(json.dumps(result, indent=1))
    print(f"wrote {args.out}")


if __name__ == "__main__":
    main()
