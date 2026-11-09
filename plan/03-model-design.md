# 03 · Model design: from the parameter guide to code

**This file turns each section of the parameter guide into data structures and equations that an
implementer can code directly.** Where the guide leaves the formulation open, the choice here is a
*proposal*. It is marked ⚑, and the phase that implements it must confirm it in an ADR in
[decisions/](decisions/) before coding. All numbers come from the register (`params/base/`), never
from code. Guide sections are written as "G§n".

Conventions: SI inside the core (m, s, m/s). km/h and cm exist only in the register and are converted
by the resolver. `s` is distance along a ribbon and `y` is lateral offset, positive to the left of
the direction of travel.

## 1. Agent kinds and vehicle classes (G§1)

| Class key | Sub-classes | Notes |
| --- | --- | --- |
| `bicycle` | — | |
| `cycle_rickshaw` | — | grade C dimensions |
| `two_wheeler` | `moped`, `scooter`, `motorcycle`, `e_two_wheeler` | the bike taxi is a motorcycle with a persona; e-2W kinematics differ (G§7.1). Moped and scooter are separate because their free-flow speeds differ (36.0 vs 43.6 km/h) |
| `e_rickshaw` | — | legal envelope ≤ 2.6 × 1.0 m, ≤ 25 km/h, ≤ 2 kW |
| `auto_rickshaw` | — | CNG |
| `car` | `hatchback`, `sedan`, `suv` | |
| `lcv` | — | |
| `bus` | `city_bus` | |
| `truck` | `heavy_commercial` | |
| `tractor` | — | appears in the critical-gap table |

There are 10 classes and 15 vehicle types in all (sub-classes count separately), which meets the guide's "at least 12".
Pedestrians and animals are separate agent kinds that share the neighbour grid but not the vehicle
arrays.

**Per-agent draws at spawn.** Each draw comes from the sub-class distribution in the register:
dimensions, desired speed `v0`, time gap `T`, standstill gap `s0`, max acceleration `a`, comfortable
deceleration `b`, reaction time, and critical gaps by conflicting class. Trait links (§6) then scale
them.

**Checks, not inputs.** Discharge acceleration at signals (0.4–0.6 m/s²) and dynamic PCU are
emergent. They have observers and validation targets, not parameters (G§1 cautions).

## 2. Longitudinal following (G§2.1) ⚑

IDM with a **lane-free leader set**. For follower `i` and every candidate leader `j` ahead within the
look-ahead distance:

```
gap_s(i,j)   = s_j − s_i − (l_i + l_j)/2
gap_y(i,j)   = |y_j − y_i| − (w_i + w_j)/2
c_ij(v̄)      = pair lateral clearance (§3), v̄ = (v_i + v_j)/2
d_ij         = gap_y(i,j) − c_ij(v̄)                      # < 0 → laterally "in the way"
ω(d)         = 1 if d ≤ 0 else exp(−d / λ_brake)          # λ_brake = 0.15 m (Kanagaraj & Treiber)

s*_ij        = s0_i·M_s0[ci,cj] + v_i·T_i·M_T[ci,cj] + v_i·(v_i − v_j) / (2·sqrt(a_i·b_i))
a_IDM(i,j)   = a_i · [1 − (v_i/v0_i)^δ − (s*_ij / gap_s)^2]
a_free(i)    = a_i · [1 − (v_i/v0_i)^δ]
a_long(i)    = min( a_free, min_j [ a_free + ω(d_ij)·(a_IDM(i,j) − a_free) ] )
a_long       = clamp(a_long, −b_emergency, a_i)               # b_emergency = 9 m/s²
```

- **Partial following comes for free.** Several leaders with partial lateral overlap all contribute;
  the strongest constraint wins, and the constraint fades smoothly as lateral clearance opens. This
  produces staggered following and following two leaders at once (Asaithambi 2016, Kanagaraj 2015).
- **Leader-type dependence.** `M_T` and `M_s0` are follower × leader class matrices, initialised to 1
  and fitted in stage 2 (Munigety 2018).
- **Reaction delay** is optional (a per-agent ring buffer of perceived states). It stays off unless
  start-up waves or string stability look wrong (G§2.3).
- **Local caps.** Humps, waterlogging, roughness and pothole crossings set the local `v0` cap used in
  `a_free`. That is the only way road objects touch longitudinal speed.

## 3. Lateral behaviour (G§2.2) ⚑

**Clearance.** `c_ij(v̄) = (α_pair + β_pair · v̄_kmh) / 100` in metres. The baseline is
`α = 120.9, β = 0.615` (Budhkar & Maurya 2017). Pair modifiers:

- the slope is +35% for car–bike pairs and −25% for car–auto pairs;
- same-type pairs get a smaller intercept than mixed pairs;
- two-wheelers get the smallest clearances overall.

P5 must first settle whether `C` is centre-to-centre or edge-to-edge in the source. It must also check
that two-wheeler pairs end up well under 1.2 m at standstill. If they don't, it fits pair-specific
intercepts.

**Target choice** runs every `Δt_dec`, or on an event. Candidate lateral positions `y_k` span the
drivable width (step `Δy_cand`, default 0.25 m). Utility, MOBIL-style:

```
U_k = [a_long(i | y_k) − a_long(i | y_now)]                 # own advantage at y_k
    + p_i · Σ_f∈new followers [a_f(after) − a_f(before)]    # politeness, p0 = 0.2 scaled by κ
    + w_route(u) · r(y_k)                                    # route-side pull, grows with urgency u (§9)
    + w_band · bonus(y_k)                                    # e.g. median-side speed band (G§7.3)
    − w_edge · e(y_k)                                        # kerb, obstacle, pothole aversion
    − c_move · |y_k − y_now|
choose y* = argmax U_k  if  U_k* > Δa_threshold,  else keep the current target
```

A move is feasible only if the path to `y*` keeps clearances of at least `f(u, κ) · c_ij` to the side
neighbours it passes. `f` falls as urgency `u` and aggressiveness κ rise, which is how squeezing
happens.

**Lateral motion** happens every step:

```
v_y = (y* − y)/τ_lat − Σ_j sign(y_j − y) · A_steer · exp(−max(gap_y(i,j),0) / λ_steer)
v_y = clamp(v_y, −max(v_s·tan θ_max, v_creep), +max(v_s·tan θ_max, v_creep))
```

The defaults are `τ_lat = 1 s`, `θ_max = 0.2 rad` and `λ_steer = 0.30 m` (Kanagaraj & Treiber 2018).
`v_creep` is a small lateral speed for two-wheelers in near-stopped queues, which gives seepage to the
front of a queue at a red light.

**Collision guard.** After integration, overlap between oriented bounding boxes is resolved by
cutting back the step. Each resolution is logged as a `collision_guard` event. Tests require zero
such events in calibrated scenarios.

**Not violations (G§4.4).** Gap filling, staggered following and moving between notional lanes are
baseline physics. Violation counters never count them.

## 4. Gap acceptance and forced entry (G§2.4, §6.7)

- **Critical gap** `t_c(i, conflicting class)`: lognormal per agent around Abhigna et al. (2020) by
  entering class. It is scaled by 1.30–1.35 against heavy-vehicle streams and by `exp(−γ_g·κ_i)`.
- **Accepting a gap.** The agent accepts when the available gap ≥ `t_c`.
- **Multiple entries.** Several waiting vehicles may take one gap if each sees its own gap as
  acceptable. This reproduces the "several merge on one gap" pattern.
- **Forced entry** is a violation (§7). It is a hazard that rises with waiting time and κ. The
  conflicting major-stream vehicle sees the entrant as a leader and brakes.
- **U-turns at median openings:** a path from a gamma distribution on 4-lane roads, or a mixture on
  6-lane roads. Critical gaps are smaller than developed-country values (Mohapatra et al. 2016).

## 5. Pedestrians and animals (G§6.4–6.5)

- **Pedestrians.**
  - Crossing demand is Poisson per 100 m per hour. Each crossing point is drawn from a distribution.
  - Groups are drawn by size; a group needs a wider gap.
  - Crossings roll band by band, with a possible wait on the centreline.
  - Each pedestrian has an accepted-gap distribution.
  - At signals they cross on red with a hazard that rises through the red phase (44.6% of red
    arrivals cross, Kumar & Ghosh 2022).
  - Blocked footpaths push a share of them onto the carriageway as slow lateral obstacles.
- **Cattle** are slow or static obstacles that dwell on medians and verges.
- **Dogs** are Poisson crossing events, more frequent at night. They trigger the same
  swerve-or-brake response as potholes.

## 6. Traits, personas and within-trip variation (G§3)

Every driver carries a trait vector `z = (κ, π, ρ, c, η, σ_skill, δ)`:

| Trait | Meaning |
| --- | --- |
| κ | aggressiveness |
| π | time pressure |
| ρ | risk tolerance |
| c | conformity |
| η | norm sensitivity |
| σ_skill | skill |
| δ | distraction |

- **Personas** are mixture components, not rulebooks: `z ~ N(μ_p, Σ_p)` for persona `p`. The 10
  starter personas from G§3.2 are mean shifts. Persona *shares* are grade C sweep factors, set per
  population spec (§12).
- **Trait links** (the γ values are Fit or Sweep):

```
v0_i   = v0_draw · (1 + γ_v·κ)           T_i  = T_draw · exp(−γ_T·κ)
s0_i   = s0_draw · exp(−γ_s0·κ)          c_ij scaled by exp(−γ_c·κ_i)
p_i    = p0 · exp(−γ_p·κ)                t_c  scaled by exp(−γ_g·κ)
reaction_i = rt_draw · (1 + γ_rt·δ_i) · skill factor;   lateral noise ∝ 1/σ_skill
```

- **Within-trip κ** follows an Ornstein–Uhlenbeck process plus a jump when accumulated delay passes a
  threshold:

```
dκ = θ_κ·(κ̄_i − κ)·dt + σ_κ·dW + J·𝟙[delay_i > d*_i]
```

- **Phone use / distraction episodes** are Poisson, frequent for delivery riders. During an episode
  reaction time rises and lateral wander grows. It is a state, not a decision.

## 7. Violations: one decision rule (G§4) ⚑

The linear predictor, the same for every violation type `v`:

```
η_v,i = β0_v + βκ_v·κ_i + βt_v·time_saved + βw_v·waiting_time
      + βs⁺_v·log(1 + N_violating) − βs⁻_v·log(1 + N_complying_waiting)
      − βe_v·perceived_enforcement_i − βr_v·perceived_risk_i
```

**Δt-invariance.** A probability evaluated every tick would pile up with simulation resolution. So
each trigger is one of two kinds:

- **One-shot triggers** (arriving at amber, approaching a split, a hail event):
  `P = σ(η)` once per episode.
- **Continuous triggers** (waiting at red, queued beside a footpath, waiting at a junction):
  hazard `λ = λ_max,v · σ(η)` per second, so `P(Δt_dec) = 1 − exp(−λ·Δt_dec)`.

**Feature sources**

| Feature | Where it comes from |
| --- | --- |
| Time saved | Remaining red (exact if a countdown is visible, otherwise the agent's expectation from the cycle); detour saved ÷ speed for wrong-way riding; queue delay saved for footpath riding |
| Imitation counts | Agents inside the field-of-view cone (radius and angle are register entries). Same violation type by default; "any type" is a G§4.3 sensitivity switch (Keizer et al.) |
| Perceived enforcement | `η_i · D(x,t) · penalty_eff`, where `D` is the zone's detection field (§11) |
| Perceived risk | Exposure if the agent acts now: TTC to the conflicting stream and the size of the conflicting vehicle |

**Intercepts and checks.** Intercepts `β0` are fitted to observed rates (stage 6). For example, the
red-light share is about 1 in 17 in Mumbai. The two timing regimes — just after red starts, and just
before green — are a **check**, not a fit: they should emerge from commitment at amber and from
countdown-driven time saved.

**Catalogue** (one module each):

| Type | Trigger | Action when taken | Grade |
| --- | --- | --- | --- |
| `red_light` | at or near the stop line during red | ignore the stop line, cross with gap acceptance against cross traffic | A |
| `early_start` | front of queue, countdown visible | start before green | A |
| `amber_go` | at amber onset | go instead of stop (one-shot, uses distance, speed and class) | B |
| `stopline_seepage` | queued with lateral gaps | creep past the stop line (raises measured saturation flow) | A |
| `wrong_way` | legal detour is long; a service road or opposite strip is available | route through the opposite-direction strip | B |
| `footpath` | queue beside a footpath with no bollards | move onto the footpath strip | C |
| `forced_entry` | waiting at an uncontrolled junction or median opening | enter with a gap below `t_c` | B |
| `late_merge` | lane drop or split, queue-length difference between streams | delay the merge, cut in near the deadline | B |
| `box_blocking` | green but no exit space | enter the junction anyway | C |
| `opposite_encroachment` | own queue long, oncoming gap available (undivided road) | use the opposite side | C |
| `stop_anywhere` | hail, metro exit, market (attractors) | stop at the kerb or in the stream for a dwell time | B |

## 8. Signals and control states (G§6.6)

- **Controller states per junction:** `fixed`, `fixed_timer_broken`, `blinker`, `dark`, `police`.
  A Markov chain per junction-day drives outages, and rain raises the outage rate. Blinker hours
  follow a schedule (e.g. 23:00–07:00).
- **Fixed** controllers have phase plans and optional countdown displays. A display can break, which
  hides the remaining red.
- **Blinker and dark** both switch the junction into the uncontrolled regime. Every approach uses gap
  acceptance against every conflicting stream, priority is ambiguous, and forced entry applies. This
  is the "Chicken" game of the guide.
- **Police** is an adaptive controller with long cycles, a bias towards the major approach, and a
  per-agent compliance probability that falls when the officer is visibly overwhelmed.
- **Each state change** writes a `signal_state` event, so phase-resolved violation statistics can be
  computed.

## 9. Splits, deadlines and missed turns (G§6.7) ⚑

**Route awareness.** Each agent draws an awareness distance from a mixture: most know well upstream,
and a "late realiser" tail is the key swept quantity. Sign visibility per split can be cut by large
vehicles in the line of sight.

**Urgency** once aware:

```
u = Δy_needed / max(x_deadline − x, ε)
```

As `u` rises:

1. `w_route(u)` grows (§3).
2. The required clearance factor `f(u, κ)` falls.
3. The `late_merge` and `forced_entry` hazards rise.

**At the deadline,** if the move is still not done:

- **stop and wait:** probability by class and persona; cars more than two-wheelers. This is what
  blocks the stream.
- **give up:** continue, then U-turn at the next median opening, or go wrong-way (§7).

## 10. Road objects (G§6)

Every object has a position, a footprint and per-class effects through the four channels. None of
them has a capacity multiplier.

| Object | Width | Speed cap | Gap supply | Incentive | Response logic |
| --- | --- | --- | --- | --- | --- |
| Pothole | footprint | yes, if hit | — | — | Detection distance falls with speed, darkness, water and occlusion. Response by class from a multinomial (swerve, slow, straddle, hit), modulated by κ. A swerve goes through lateral feasibility (§3) |
| Roughness (IRI) | — | `v0` reduction by class (cars > heavy vehicles) | — | — | stage 1 fit |
| Parked vehicle or double parking | footprint + offset | — | — | — | Static obstacle. Parking and unparking events block space for a lognormal manoeuvre time. Cruisers slow near the kerb |
| Vendor or encroachment | polygon | — | — | — | Also blocks the footpath, pushing pedestrians onto the road |
| Bus stop | dwell footprint | — | buses dwell (lognormal) at a sampled longitudinal and lateral offset | — | Passengers waiting on the carriageway take width (C); autos and e-rickshaws cluster near the stop (attractor) |
| Stop-anywhere attractor (metro exit, market) | — | — | — | raises the `stop_anywhere` hazard | Persona-driven Poisson dwell events |
| Speed hump | — | by class × geometry | — | — | A two-wheeler edge bypass is used where the hump stops short of the kerb (C) |
| Gradient | — | `v0` loss per % by class | — | — | e-rickshaw climbing speed from the power cap and loaded mass (C) |
| Waterlogging | closes part of the width | depth–speed curve (Pregnolato 2017) | — | — | Hides potholes, so they are detected later |
| Work zone | polygon | — | — | — | Lane-drop deadline (§9) plus an optional signal outage (§8) |

**Effective width** `W_eff(x) = W − Σ w_i(x) − c_shy` is an **emergent check** with its own observer.
`c_shy` comes from the clearance model, not a separate parameter. The Varanasi near-1:1 relation
between width loss and capacity loss must come out without being tuned in.

## 11. Zones and enforcement fields (G§5)

**Zone parameters** come from the register (G§5.2 and §9.5):

- detection probability sources: police posts and cameras, each with a halo;
- effective penalty;
- median-opening spacing (from geometry);
- footpath bollards;
- kerb occupancy;
- land-use intensity;
- starting violation share.

**Detection field.** It is computed on every ribbon from point sources `k` (posts, cameras), each with
detection probability `p_k` and halo length `ℓ_k`:

```
D(s) = 1 − Π_k (1 − p_k · exp(−dist_k(s) / ℓ_k))
```

Police halos are at least ~2.4 km, camera halos about 500 m, with speed effects halving every ~900 m
(Hauer 1982). One-day operations decay over time with `τ ≈ 3 days` in multi-day runs.

**Starting violation share** seeds the imitation counts at warm-up.

**Do not calibrate zones from challans** (G§5.1). Challan data may only *rank* violation types.

## 12. Demand and population (G§7)

**Entries.** Each entry has:

- a flow profile per 15 minutes and a class composition per interval;
- arrivals per class from a **shifted exponential**, `h = h_min,class + Exp(λ_class(t))`;
- an entry lateral position drawn from a class-specific empirical distribution;
- a destination drawn from turning shares, which sets the agent's mandatory lateral moves.

An entry can also be **fed by a simulated upstream signal** instead of a renewal process, which gives
platoons.

**Day-to-day demand variation** of ±10–15% is a sweep factor.

**The population spec is separate from geometry.** A scenario is the product of three parts that can
be swapped independently:

```
Scenario = Geometry(zone G) × Population(persona mix, composition from zone P) × Enforcement(zone E)
```

The population swap (G§5.3) is then just `G = Lutyens', P = Shahdara` and the reverse. The roads ×
behaviour factorial is the full 2×2 (or 2×2×2 with enforcement).

**Routes** are fixed in early phases. Route choice is a later, separate experiment (G§7.5).

## 13. Scenario bundles (G§6.10, §7.6–7.7)

A bundle is a named set of overrides on **existing** parameters. It never adds a mechanism, and it can
be combined with other bundles.

| Bundle | Moves together |
| --- | --- |
| `rain(intensity)` | desired speed ↓, visibility and detection ↓, potholes hidden, signal outage rate ↑, flooded segments switched on, mode and demand shift (two-wheelers sheltering, C) |
| `night` | persona mix shifted (more trucks), blinker schedule on, enforcement detection ↓, hazard detection distance ↓ on unlit links |
| `fog` | detection distance ↓ |
| `grap_stage3`, `grap_stage4` | composition filters by fuel and emission class; trucks stopped at the border |
| `ev_shift(share)` | two-wheeler sub-class mix moves to `e_two_wheeler` with its own kinematics |
| `erickshaw_cap(...)` | composition and stop-propensity scenario |

## 14. Online observers (what the core measures while running)

Detectors, conflicts, signal-cycle statistics, effective width, gridlock indicator and KPIs. Their
definitions and output tables are in [04-data-outputs-analytics.md](04-data-outputs-analytics.md).
