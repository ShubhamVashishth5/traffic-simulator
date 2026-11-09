# ADR-0010: The lateral model (clearance, target choice, motion, guard, conflicts)

- **Status:** proposed: review gate P5.1, needs the user's decision
- **Date:** 2026-11-09
- **Phase / task:** P5.1 (governs P5.2–P5.8; changes one term of the P2 following model)

## Context

Plan/03 §3 marks the lateral formulation ⚑: a proposal to confirm before coding. P5.1 asks three
things: read Kanagaraj & Treiber (2018) against that proposal, settle whether Budhkar & Maurya's
`C` is centre-to-centre or edge-to-edge, and write the final equations with every parameter mapped
to the register. Both papers were read in full for this ADR (arXiv:1805.05076v1; J. Mod. Transport.
25(2):74–89, open access). Five findings change the proposal.

### What Budhkar & Maurya (2017) measured

1. **`C` is edge-to-edge.** It is the distance from ultrasonic sensors on the *side* of the test
   vehicle to the *side* of the vehicle passing it ("inter-vehicular lateral distance"; their Fig. 2
   draws "the path followed by the edge of the interacting vehicle"). The P2 code already treats it
   that way (`gap_y − c`), so nothing moves.
2. **`C` is a mean, not a minimum.** Each value is the average over one overtaking event in moving,
   uninterrupted mid-block traffic; pairs further apart than 2.5 m were dropped. The model is
   `C = S·v̄ + I + u` with a **beta-distributed residual** `u` on `[a, b] = [−117, +148] cm` (all
   pairs). The paper calls `C + a` and `C + b` the "minimum and maximum thresholds" that vehicles
   "would not venture below or above". The lower threshold is about 0.04 m at standstill and 0.29 m
   at 40 km/h.
3. **The pair effects in the guide are relative to same-type pairs, not to the pooled line.** The
   "+35% car–bike" and "−25% car–auto" slopes come from §5.4, where a mixed pair is compared with
   the *average of the two same-type pairs*. Table 4 gives the lines themselves:

   | Pair | n | Slope (cm per km/h) | Intercept (cm) | `C` at 40 km/h (m) |
   | --- | --- | --- | --- | --- |
   | All pairs (text, used by the guide) | 6,016 | 0.615 | 120.9 | 1.46 |
   | Bike–bike | 87 | 0.224 | 118.02 | 1.27 |
   | Car–car | 2,246 | 0.681 | 117.31 | 1.45 |
   | Auto–auto | 140 | 1.037 | 114.55 | 1.56 |
   | Auto–bike | 417 | 0.739 | 117.27 | 1.47 |
   | Car–bike | 1,372 | 0.612 | 125.16 | 1.50 |
   | Car–auto | 897 | 0.642 | 124.12 | 1.50 |
   | Car–heavy (bus, truck) | 538 | 0.675 | 126.35 | 1.53 |
   | Auto–heavy | 131 | 0.985 | 123.86 | 1.63 |
   | Car–LCV | 139 | 0.132 | 139.78 | 1.45 |

   Check: `1.35 × mean(0.681, 0.224) = 0.611` (car–bike) and `0.746 × mean(0.681, 1.037) = 0.641`
   (car–auto). The register's `slope_factor.car_two_wheeler = 1.35`, applied to the pooled 0.615 as
   plan/03 implies, would give 0.83: a misreading of the source.
4. **Two-wheelers are not "well under 1.2 m at standstill".** Their intercept is 118 cm; they keep
   the smallest clearance through the *slope*. Plan/03 says to fit smaller intercepts if this
   happened. There is no measurement to fit them to, and finding 2 shows why none is needed: what
   lets a two-wheeler filter through a queue is the minimum, not the mean.
5. **Passing between two vehicles** ("constrained") leaves the intercept unchanged and cuts the
   slope by 18–80% (Table 5). This is a check for the model, not an input.

### What Kanagaraj & Treiber (2018) specify

- A force model, not a decision model. Longitudinal: the underlying car-following acceleration
  toward the **single most constraining leader**, attenuated by `α = min(exp(−s_y/s_y0), 1)` where
  `s_y` is the **edge-to-edge** lateral gap and `s_y0 = 0.15 m`. There is **no clearance offset**:
  a leader 0.5 m to the side has weight 0.04.
- The car-following model must return its maximum deceleration (`b_max = 9 m/s²`) when the gap is
  not positive, so the interaction is bounded.
- Lateral: `dw/dt = (w0 − w)/τ`, with the desired lateral speed `w0` a sum over neighbours of
  `λ · α̃ · a_int · [1 − λ_Δw (w_j − w_i) sign(Δy)]` (`λ = 0.4 s`, `s̃_y0 = 0.30 m`,
  `λ_Δw = 0.7 s/m`, `τ = 1 s`), followers weighted by politeness `p = 0.2`, heading limited to
  `|w/v| ≤ tan θ`, `θ = 0.2 rad`.
- "Tactical" lateral moves (bottlenecks, turns, keep-left) are "not considered in this paper".
- The authors note the follower can get stuck between two leaders (a local optimum of the field).
- Calibration: nine model parameters against five aggregate statistics of 20 s of traffic on one
  47 m section. The values are a starting point; the guide's grade A for them is generous.

### Why the proposal in plan/03 cannot stand as written

Plan/03 §2 uses `ω = 1 if gap_y ≤ c_ij` with `c_ij` the B&M **mean**, and §3 allows a move only if
it keeps `f·c_ij` to side neighbours. With `c ≈ 1.2 m` at low speed and the P2 cut-off, a follower
brakes fully for any leader within 1.2 m of its side and partly out to 1.9 m. A 0.64 m motorcycle
would need a hole of about 3 m between two stopped cars. Filtering, P5.4's "two-wheelers reach the
front of a stopped queue", and the Chennai densities are out of reach. K&T's own model interacts
over 0.15–0.45 m. The P2 following model has had this problem since P2 (it is one reason bicycles
dominate the P3 screen).

## Options

**A. What B&M's `C` does in the model**

1. *Hard threshold (plan/03 as written).* One number. Blocks filtering, as above.
2. *Two levels (proposed).* The mean line is the **desired** clearance `c_des`: where agents choose
   to be. The lower threshold `c_des − Δ` is the **minimum** clearance `c_min`: what braking and
   feasibility use. Both come from the same paper; at low speed the model reduces to K&T.
3. *Drop `C` from the dynamics (pure K&T)*, keeping B&M as a validation target. Contradicts G§9.2,
   where the clearance line is a stage-3 `Fit` parameter, and loses the speed dependence.

**B. Lateral dynamics**

1. *Target choice plus tracking (plan/03, proposed).* Every `Δt_dec` an agent scores candidate
   lateral positions with a MOBIL-style utility and steers towards the best. Route side, speed
   bands, kerb and obstacle aversion and urgency (P6, P7, P9) enter as utility terms; the choice can
   be logged and inspected (rule 9); it does not get stuck in a local optimum between two leaders.
   Cost: a decision loop, and K&T's `λ` and `λ_Δw` do not carry over one-to-one.
2. *K&T's force field verbatim.* Calibrated values apply directly and there is no decision loop.
   But the tactical term the later phases need is exactly what the paper leaves out, squeezing has
   no explicit rule for ADR-0007 to switch, and the stuck-between-leaders case is real.

**C. Pair effects**

1. *Table 4 lines per pair (proposed)*, the pooled line for pairs the paper did not observe.
2. *The guide's two factors on the pooled line.* Misreads the source (finding 3).

## Decision (proposed, pending review): A2 + B1 + C1

Notation: subject `i`, other vehicle `j`; widths `w`, lengths `l`; `s` along the ribbon, `y`
lateral; `e_ij = |y_i − y_j| − (w_i + w_j)/2` the edge-to-edge gap; `v̄ = (v_i + v_j)/2`.

### 1. Clearance (P5.2)

```
c_des,ij(v̄) = k_i · (I_p + S_p · v̄)                    p = pair of classes (i, j), symmetric
c_min,ij(v̄) = k_i · max(c_floor, I_p + S_p · v̄ − Δ)
k_i         = exp(−γ_c · κ_i)                            κ_i ~ N(μ_κ, σ_κ), drawn at entry
```

`I_p, S_p` are Table 4 for observed pairs and the pooled line otherwise. Classes without their own
data borrow the nearest one (bus, truck, tractor = heavy; bicycle = two-wheeler; cycle- and
e-rickshaw = auto), as grade C entries. `Δ` is the pooled lower support of the residual; the
per-pair supports are too noisy to use (bike–bike has n = 87).

### 2. Following (changes plan/03 §2 in two places)

```
ω_ij   = 1 if e_ij ≤ c_min,ij else exp(−(e_ij − c_min,ij) / λ_brake)      # was: B&M mean
a_ij   = max(a_IDM(i, j), −b_emergency)                                    # K&T: bounded interaction
a_long = min(a_free, min_j [a_free + ω_ij · (a_ij − a_free)])
```

The `min` over leaders is K&T's "most constraining leader". Vehicles level with the subject
(`gap_s ≤ 0`) do not brake it unless the bodies overlap; keeping side clearance is the lateral
model's job. (K&T brake for them with `α·b_max`.)

### 3. Target choice (P5.3)

Runs every `Δt_dec`, staggered by agent id, and at once when the agent enters or its most
constraining leader changes. Candidates `y_k = y_i + k·Δy_cand` within `±R_cand` and the road.

**Feasible** candidates:

- stay on the carriageway;
- lie in the interval around `y_i` bounded by the nearest side neighbour on each side
  (`gap_s ≤ 0`), keeping `e_ij ≥ c_feas,ij = f(u) · c_min,ij(v̄; κ_sq)`. `f(u) = 1` until urgency
  exists (P7). **`κ_sq` is the squeezing rule's κ: the agent's own, or the population quantile
  `personas.cooperative.squeeze_kappa_quantile` when the cooperative twin sets it (ADR-0007);**
- do not make any follower `f` behind brake harder than `b_safe` (MOBIL's safety criterion).

**Utility** (m/s²):

```
U_k = [A_i(y_k) − A_i(y_i)]                                   # own advantage: a_long of i if it were at y
    + p0 · Σ_f [a_f(i at y_k) − a_f(i at y_i)]                # politeness, followers behind
    − w_c · Σ_j [σ_ij(y_k) − σ_ij(y_i)]                       # comfort: shortfall below the desired clearance
    − c_move · |y_k − y_i|
σ_ij(y) = clamp((c_des,ij − e_ij(y)) / (c_des,ij − c_min,ij), 0, 1)
```

`σ` is summed over side neighbours and over leaders and followers within the agent's own desired
gap; each road edge counts as a neighbour with desired clearance `c_edge` and minimum 0. The
median-band speed bonus (G§7.3) now follows the agent's lateral position, so the band enters
through `a_free` in `A_i`, not through a separate weight. Route pull is zero until P7.

The agent takes `y* = argmax U_k` over feasible candidates if `U_k* > Δa_thr`; otherwise it keeps
its target, pulled into the feasible interval. Ties go to the smaller move. The choice is
deterministic; `observers.lateral.log_decisions` writes each change of target with its terms.

### 4. Lateral motion (P5.4)

```
w0      = (y* − y) / (2τ) − Σ_side sign(y_j − y_i) · λ_s · b_emergency · exp(−max(e_ij − c_min,ij, 0) / λ_steer)
dv_y/dt = (w0 − v_y) / (τ/2)                                  # critically damped tracking, time constant τ
|v_y|   ≤ max(v_s · tan θ_max, v_creep,class if v_s < v_stop)
heading ψ = clamp(atan(v_y / v_s), ±θ_max)
```

A body is the rectangle `l × w` turned by `ψ`. The interaction model uses its axis-aligned extent
(`w·cos ψ + l·sin|ψ|` wide), so a vehicle that swings out is wider to its neighbours.

### 5. Collision guard (P5.5)

After integration, any pair whose oriented rectangles overlap is resolved by cutting back the
rear vehicle's step: its lateral move first, then its longitudinal move (as P2). Each resolution is
a `collision_guard` event naming the other vehicle, the overlap and the axis.

### 6. Conflicts (P5.7)

For each agent and each cached neighbour: the first time within `H` at which the two oriented
rectangles touch if both keep their velocity. Consecutive steps under the threshold for the same
pair are one episode in `conflicts.parquet`: start, end, minimum TTC, maximum DRAC
(`Δv² / (2·gap)`), type (`rear_end`, `sideswipe`, `head_on`) and position.

### Parameters

| Symbol | Register path | Value | Source | Grade, role |
| --- | --- | --- | --- | --- |
| pooled `I`, `S` | `interaction.lateral.clearance.intercept`, `.slope` | 120.9 cm, 0.615 cm/(km/h) | B&M 2017, text | A, fit 3 |
| `I_p`, `S_p` | `…clearance.pair.intercept.<a>.<b>`, `…pair.slope.<a>.<b>` | Table 4 | B&M 2017, Table 4 | A (borrowed classes C), fit 3 |
| `Δ` | `…clearance.min_offset` | 117 cm | B&M 2017, Table 4 (beta lower support, all pairs) | A, fit 3 |
| `c_floor` | `…clearance.min_floor` | 0 cm | bodies may not overlap | C, fit 3 |
| `γ_c` | `personas.links.clearance` | 0.2 | G§3.1 names the link, not its size. Bounded above by 0.29, the CV of B&M's residual if all of it were between drivers | C, sweep |
| `μ_κ`, `σ_κ` | `personas.traits.aggressiveness.mean`, `.sd` | 0, 1 | existing | B |
| `κ_sq` quantile | `personas.cooperative.squeeze_kappa_quantile` | unset | ADR-0007 | C, scenario |
| `λ_brake` | `interaction.lateral.braking_scale` | 0.15 m | K&T 2018 | A, fit 3 |
| `λ_steer` | `…steering_scale` | 0.30 m | K&T 2018 | A, fit 3 |
| `λ_s` | `…steering_sensitivity` | 0.4 s | K&T 2018 (`λ`) | A, fit 3 |
| `τ` | `…time_constant` | 1 s | K&T 2018 | A, fit 3 |
| `θ_max` | `…max_heading` | 0.2 rad | K&T 2018 | A, fit 3 |
| `p0` | `…politeness` | 0.2 | K&T 2018 | B, fit 3 |
| `b_safe` | `…safe_decel` | 4 m/s² | Kesting, Treiber & Helbing 2007 (MOBIL) | B, fit 3 |
| `Δa_thr` | `…switch_threshold` | 0.1 m/s² | Kesting, Treiber & Helbing 2007 | B, fit 3 |
| `w_c` | `…comfort_weight` | 0.5 m/s² | assumed: half a car's `a`, so a stuck two-wheeler squeezes and a stuck car does not | C, fit 3 |
| `c_move` | `…move_cost` | 0.05 m/s² per m | assumed | C, fit 3 |
| `c_edge` | `…edge_clearance` | 0.25 m | K&T 2018 (boundary steering scale) | B, fit 3 |
| `v_creep` | `…creep_speed.<class>` | 0.5 m/s for two-wheelers and bicycles, 0 otherwise | assumed | C, fit 3 |
| `Δt_dec`, `Δy_cand` | `core.decision_interval`, `core.lateral_candidate_step` | 0.5 s, 0.25 m | existing | C, fix |
| `R_cand` | `core.lateral_candidate_range` | 3.5 m | one notional lane each way | C, fix |
| TTC threshold, `H` | `observers.conflict.ttc_threshold`, `.horizon` | 1.5 s, 3 s | G§8.4; horizon numerical | B / C, fix |

The two `slope_factor.*` entries are removed: Table 4's lines contain them (finding 3).
`params/guide_rows.yaml` keeps row 9.2/lateral_clearance covered by the pair entries.

### Where this departs from the papers, and why

| Departure | Why |
| --- | --- |
| A speed-dependent minimum clearance inside K&T's attenuation | K&T's gap scale is constant; B&M measured that the lower threshold rises with speed. At standstill the two agree (`c_min ≈ 0.04 m`) |
| Target choice instead of K&T's summed lateral forces | Later phases need tactical terms K&T leave out; decisions must be loggable; no local optimum between leaders |
| K&T's `λ_Δw` is not used | It damps relative lateral speed inside the force sum; the tracking law is already critically damped |
| `τ` is the time constant of position tracking, not of lateral speed adaptation | One constant instead of two; a 3.5 m shift takes about 5 s, as observed lane changes do |
| Side neighbours do not brake the subject | K&T's `α·b_max` makes vehicles alongside each other slow down; here they steer apart |
| Road edges are hard limits plus a comfort term, not exponential forces | Same effect at the kerb with one parameter |
| B&M's mean used as a preference, their lower threshold as the limit | They report a distribution; plan/03 used its mean as a limit |
| IDM, not K&T's ACC variant | P2 decision; unchanged here |

## Consequences

- **P2 results move.** Following now ignores leaders that are beside the subject by more than
  `c_min` plus a few `λ_brake`, so the P2 milestone and the P3 Morris screen are re-run (the P5 card
  already asks for the screen). No calibration stage is frozen yet, so nothing frozen is touched.
- `plan/03` §2–3 are rewritten to these equations in the same change.
- Stage 3 fits `Δ`, `w_c`, `c_move`, the scales and `τ`; P5.8's recovery test reports which of them
  lateral-position distributions can identify. `w_c` and `Δ` may trade off.
- B&M's Table 5 (constrained passing) and the observed spread of `C` become validation targets.
- The cooperative twin stops being bitwise identical to the observed arm once `κ_sq` is read.
- Cost: one decision per agent per `Δt_dec` over at most `2·R_cand/Δy_cand + 1 = 29` candidates,
  plus lateral integration and the conflict scan every step. The corridor budget (555 ns per
  agent-step, plan/02) is re-checked in P5.9.
- New per-agent state: `κ`, `v_y`, target `y*`, heading. New outputs: `conflicts.parquet`, lateral
  columns in `trajectories`, `lateral_move` events.
