# ADR-0011: Stage-3 parameters that lateral distributions cannot identify

- **Status:** proposed (needs your decision: the register is unchanged until then)
- **Date:** 2026-11-10
- **Phase / task:** P5.8 (stage-3 tooling); affects P10 (calibration campaign)

## Context

P5.8 asks for a synthetic recovery of the clearance coefficients and lateral scales, and says
that parameters that cannot be recovered are "reported and demoted via an ADR". The guide is
explicit that not everything can be fitted (G§8.1) and that stage 3 fits "clearance coefficients,
lateral scales, filtering thresholds" from "lateral position distributions by class; partial
densities" (G§8.3).

The stage-3 statistics (`marg.stage3`) are computed from trajectories, so simulated runs and real
trajectory data (the Chennai set) go through the same code: lateral position at a section (mean,
SD) and partial density by class, lateral metres per km by class, and the edge-to-edge gap to
the nearest vehicle alongside (mean, SD, share under 1 m; pairs within 2.5 m, as Budhkar &
Maurya). Their standard errors (delete-one-cluster jackknife) are honest: two disjoint seed sets
at the same parameters differ by z with SD 1.03.

**Screen** (`scripts/p5_stage3_recovery.py`, `scenarios/stage3_recovery.yaml`: 1 km, 10.5 m,
2,400 veh/h of two-wheelers, cars and autos; 10 seeds × 600 s measured): the change of the
statistics between −25% and +25% of each parameter's register value, in standard errors.
Ten runs of ten minutes is more trajectory data than most published sets (Kanagaraj & Treiber
calibrated on 20 s of one section).

| Parameter | max \|z\| | Moved mostly |
| --- | --- | --- |
| `clearance.min_offset` (Δ) | 30.6 | side-gap mean, SD |
| `steering_scale` (λ_steer) | 15.8 | side-gap mean |
| `clearance.pair.intercept.two_wheeler_car` | 10.6 | side-gap mean |
| `clearance.pair.intercept.car_car` | 7.4 | side-gap mean |
| `steering_sensitivity` (λ_s) | 6.2 | side-gap mean |
| `braking_scale` (λ_brake) | 5.8 | side-gap share under 1 m, SD |
| `comfort_weight` (w_c) | 3.4 | side-gap SD |
| `clearance.pair.slope.car_car` | 2.8 | lateral SD of cars |
| `time_constant` (τ) | 2.1 | lateral metres per km |
| `switch_threshold` (Δa_thr) | 1.9 | lateral metres per km |
| `move_cost` (c_move) | 1.9 | side-gap mean |
| `edge_clearance` (c_edge) | 1.4 | side-gap SD |
| `max_heading` (θ_max) | 1.1 | lateral SD of cars |
| `politeness` (p0) | 1.1 | side-gap SD |
| `safe_decel` (b_safe) | 0.7 | — |
| `creep_speed.*` | 0.0 | — (no queues in this scenario) |

**Recovery** (truth simulated at seeds 101–110, fitted at seeds 1–10 from the register's values,
response-surface fit): Δ 99.5 ± 5.5 cm (truth 100), λ_steer 0.404 ± 0.030 m (0.38), the car–car
intercept 104 ± 24 cm (130) and λ_brake 0.155 ± 0.042 m (0.12). All four truths are inside their
95% CIs; the last two only loosely (Δ and a pair intercept both set that pair's minimum
clearance; only the desired-clearance comfort term separates them).

## Options

1. **Demote the unidentifiable ones to `sweep`** (role change, value unchanged). Stage 3 fits Δ,
   the pair intercepts, λ_steer, λ_s, λ_brake and w_c; the others keep their literature or
   assumed values and enter the sensitivity analyses (P3.7) as swept factors, with results
   reported across their range. Honest about what the data can say; P10's stage-3 fit stays
   small and well-conditioned. Cost: eight parameters are never fitted to lateral data.
2. **Keep them `fit` and add manoeuvre-level statistics** that could identify them: frequency
   and duration of lateral moves (τ, c_move, Δa_thr), the heading distribution (θ_max), lateral
   moves accepted against close followers (p0, b_safe), the distance kept to the kerb (c_edge),
   and queue seepage at a stop line (creep speeds; the barrier reference scenario already shows
   it). Needs trajectory data that resolve individual manoeuvres, and a recovery test per
   statistic before any of them is trusted. Nothing is demoted, but nothing is fitted either
   until the data exist.
3. **Fit everything with informative priors** (ABC-SMC or Bayesian least squares, P3.8 has the
   hook). The posterior of an unidentifiable parameter equals its prior, so this is option 1
   with more machinery and a risk of reading the prior back as a result.

## Recommendation (not decided)

Option 1 for `clearance.pair.slope.*`, `time_constant`, `switch_threshold`, `move_cost`,
`edge_clearance`, `max_heading`, `politeness` and `safe_decel`; option 2 as the route back to
`fit` for any of them once a manoeuvre statistic and data exist. Keep `creep_speed.*` as `fit`
and add a queue-seepage statistic (places gained between joining a queue and its release, by
class; `crates/marg-core/tests/lateral.rs` already measures it) to the stage-3 set, since that
is cheap and the barrier scenario identifies it directly. `comfort_weight` (|z| 3.4) stays `fit`
but is flagged weak.

## Consequences

- If accepted: a register change (role `fit` → `sweep` for the listed entries, with a note
  citing this ADR), regenerated schema types, and the stage-3 fit list in P10 shrinks to the
  identifiable set. No simulated result changes.
- The P3 Morris screen, when re-run, should include the demoted parameters as sweep factors.
- `docs` and the P5 report cite this ADR for which stage-3 values are fitted and which are
  literature values.
