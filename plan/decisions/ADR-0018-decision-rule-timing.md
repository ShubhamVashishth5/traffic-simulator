# ADR-0018: The violation decision rule — timing, imitation, field of view, trait links

- **Status:** accepted (the user, 2026-11-14, at the P8.1 review gate: **T2, I1, F1, K2**). The
  parts marked *(mine)* are for your review
- **Date:** 2026-11-14
- **Phase / task:** P8.1 (governs P8.2–P8.8; read by P9 for enforcement and the starting
  violation share)

## Context

G§4.1 gives one logistic rule for every violation, `P(violate) = σ(β0 + βκ·κ + βt·time saved +
βw·waiting time + βs·share of nearby violators − βe·perceived enforcement − βr·perceived risk)`,
and G§4.3 asks for imitation in both directions as two counts in the field of view. Plan/03 §7
marks the formulation ⚑: it proposes one-shot triggers and hazards, `log(1 + N)` counts and a
field-of-view cone, without settling how a hazard is realised, which other traits enter the rule,
or what the cone looks at. The register adds one more question for this ADR
(`personas.traits.aggressiveness.sd`): how κ's spread combines with the per-class parameter
spreads the register already has (G§3.1 lets κ scale `T`, `s0`, `v0`, gaps, clearance and
politeness).

What the core does today, which the choices below should fit:

- Lateral and strategic decisions run every `core.decision_interval` (`Δt_dec`, 0.5 s, `fix`),
  staggered by agent id, and at once when something changes (ADR-0010 §3).
- Stop-anywhere draws an exponential *distance* to the next stop once per event and lets it
  count for more inside an attractor (ADR-0014): an integrated-hazard clock in space.
- Gap acceptance re-reads the offered gap as a driver approaches (P7.4 found this biases the
  logged gaps low); forced entry was left for P8.
- `violations.enabled` (ADR-0007) must switch every violation hazard off in the cooperative twin.

### The problem in numbers

Agents wait 60 s at red; the hazard rises with waiting time and falls with the red still to come,
`η(t) = −9 + 0.05·t + 0.02·(60 − t)`, `λ = 1 s⁻¹ · σ(η)` (about 1 in 15 run it, close to the
Mumbai 1 in 17). 200,000 agents, one uniform each (`scripts/p8_decision_timing.py`; the
checked-in test `model::decision::tests::cumulative_probability_is_invariant_to_the_decision_interval`
runs the same case with 20,000 agents from the core's own random streams):

| `Δt_dec` (s) | σ(η) as a probability every tick | Hazard, one draw per tick (T1) | Hazard clock, one draw per episode (T2) | Same agents run as at 0.1 s: T1 | T2 |
| --- | --- | --- | --- | --- | --- |
| 0.1 | 49.9% | 6.64% | 6.55% | — | — |
| 0.25 | 24.2% | 6.63% | 6.55% | 3.7% | 100% |
| 0.5 | 13.0% | 6.70% | 6.55% | 3.5% | 100% |
| 1.0 | 6.8% | 6.67% | 6.55% | 3.4% | 100% |
| 2.0 | 3.5% | 6.85% | 6.55% | 3.8% | 99.96% |

Exact share: 6.65% (`1 − exp(−∫λ dt)`; Monte-Carlo SE 0.06%, and T2 uses the same thresholds at
every interval, so its rows are identical). "Same agents" is the share of agents who run at either
interval that run at both. The naive rule moves 14-fold with the interval; both hazard forms are
right on average; only the clock keeps *who* runs (and when: within one `Δt_dec`) when the
interval changes, so twin arms and live parameter changes compare like with like.

## Part 1. Trigger kinds and how a hazard is realised

Every type is one of two kinds (plan/03 §7, unchanged):

- **One-shot** — the decision belongs to an instant (arriving at amber, approaching a split):
  `P = σ(η)`, decided once per episode against one uniform.
- **Continuous** — the decision can be taken at any moment while a condition holds (waiting at
  red, queued beside a footpath, waiting for a gap): a hazard `λ = λ_max · σ(η)` per second.

How a continuous trigger is realised:

1. **T1: a Bernoulli draw each tick** with `p = 1 − exp(−λ·Δt_dec)` (plan/03 as written).
   Invariant on average. A fresh uniform every tick; under common random numbers a change of
   interval, of a coefficient or of a neighbour's behaviour re-deals every decision.
2. **T2: an integrated-hazard clock (proposed).** When the episode starts, draw `E = −ln(1 − u)`
   once. At each evaluation add the trapezoid `½(λ_prev + λ)·Δt` to `Λ`; act at the first
   evaluation with `Λ ≥ E`. `P(acted by t) = 1 − exp(−Λ(t))` exactly for a hazard linear in time,
   and within 0.1% here at 2 s (tested). One draw per episode. Raising any term that raises `λ`
   can only make an agent act sooner (a monotone coupling), so paired differences between twin
   arms carry little noise, and a live `set_param` moves violations smoothly. It is the same idea
   as stop-anywhere's exponential distance. The log gives `Λ`, `E` and `1 − exp(−Λ)` at the act.
3. **T3: fixed opportunities.** `σ(η)` is the probability per opportunity of a set length
   `τ_opp` (a register entry, not `Δt_dec`), i.e. `λ = ln(1 + e^η)/τ_opp`. Keeps the guide's
   formula literally, but the per-episode probability depends on an arbitrary `τ_opp`, and it
   still needs T1 or T2 underneath.

**The hazard's link (with T1 or T2):** `λ = λ_max · σ(η)`, plan/03's form, bounded so a large
time-saving term cannot make violations instantaneous. At low rates `λ ≈ λ_max·e^{β0 + …}`, so
`λ_max` and `β0` cannot both be fitted: `λ_max` is one shared `fix` entry,
`violations.hazard.max_rate` = 1 s⁻¹ (grade C: a driver acts on a decision at most about once per
perception–reaction time, Wong & Goh's mean 1.02 s, G§2.3), and only the intercepts are fitted
(stage 6). *(Mine.)*

**When features are read.** At the episode's start (the step the condition first holds, not the
next tick), then at the agent's staggered decision ticks, and at once when a count in view
changes by a violator starting to move. The episode ends when the condition ends; the clock is
then dropped. Competing types for one agent (red light and early start at the same line) each
run their own clock; the first to fire decides the act. *(Mine.)*

**Explaining it (rule 9)** *(mine)*: every episode writes one event when it ends — `violation`
(acted) or `complied` (condition ended first) — with the type, the features and each term at the
last evaluation, `λ`, `Λ`, `E` and `1 − exp(−Λ)`. A run is then a survey of violation
opportunities, as `gap_rejected` made it one of gaps (P7.4), and stage 6 can fit by likelihood.

**Random numbers.** One uniform per (agent, type, episode) from the agent's `Decisions` stream,
at a counter of its own (`VIOLATION_BASE + type·2⁴⁰ + episode`), so violations never shift other
draws, and `violations.enabled = 0` simply draws nothing.

**Classification** (refined per module in P8.4):

| Type | Kind | Episode |
| --- | --- | --- |
| `red_light` | continuous | its most constraining leader is a red stop line and it is within its comfortable stopping distance of it, or standing at it; ends at green or when it acts. Moving arrivals are in it for a few seconds, the front of the queue for the rest of red |
| `early_start` | continuous | at the front with a countdown visible; competes with `red_light` |
| `amber_go` | one-shot | amber onset, for a vehicle that can still stop at the safe braking limit (those that cannot are let through, ADR-0016) |
| `stopline_seepage` | continuous | queued at red with a lateral gap towards the line |
| `wrong_way` | one-shot | at the point where the legal route and the opposite strip part |
| `footpath` | continuous | queued beside a footpath without bollards |
| `forced_entry` | continuous | waiting at the line or hold point with no acceptable gap |
| `late_merge` | continuous | aware, before its deadline, with its branch's stream queued (pending ADR-0017) |
| `box_blocking` | continuous | at a green line held by "way through not clear" (ADR-0016) |
| `opposite_encroachment` | continuous | queued on an undivided road with an oncoming gap |
| `stop_anywhere` | as built (ADR-0014) | its exponential distance is already a clock; its probability terms move into this rule in P8.4 |

## Part 2. The other traits in the rule *(mine, please review)*

G§3.1 says what time pressure, risk tolerance, conformity and norm sensitivity act on; the
guide's formula has only κ. Proposed: they scale the coefficients of the agent's own rule (a
random-coefficient logit), so a trait matters only where its feature is present:

```
βt,i = βt·exp(γ_π π_i)      βw,i = βw·exp(γ_π π_i)         time pressure
βr,i = βr·exp(−γ_ρ ρ_i)                                     risk tolerance
βs⁺,i = βs⁺·exp(γ_c c_i)                                    conformity
βe,i = βe·exp(γ_η η_i)      βs⁻,i = βs⁻·exp(γ_η η_i)       norm sensitivity
```

The alternative, adding `β_π·π + β_ρ·ρ + …` to `η`, would make a conformist violate more with
nobody in view. `γ = 0.5` each (grade C, assumed: one SD of a trait scales a coefficient by
1.65), `sweep`. Per-type terms the general rule lacks (amber: required deceleration; class:
"two-wheelers most") are allowed as extra named terms with their own register coefficients,
logged the same way; per-class intercept shifts are `violations.<type>.intercept_shift.<class>`.

## Part 3. The imitation term

1. **I1: two log counts (proposed, plan/03):** `+βs⁺·ln(1 + N_violating) − βs⁻·ln(1 + N_waiting)`.
   Both directions of G§4.3, each with diminishing returns: the first violator matters most
   (Pelé et al.: the probability grows with the number crossing), and a waiting crowd deters
   (Fraboni et al.). Zero in view adds nothing. Tipping comes from the feedback: one going raises
   everyone's hazard, so more go.
2. **I2: the share of violators,** `βs·N_v / (N_v + N_c)` (G§4.1's words). One coefficient, but
   it cannot tell one rider alone from a crowd of fifty, and it says nothing when nobody is in
   view; G§4.3 asks for two counts.
3. **I3: linear counts with a cap,** `βs⁺·min(N_v, N_max) − βs⁻·min(N_c, N_max)`. A sharper
   threshold, one more parameter (`N_max`) with no source.

**Who counts** *(mine)*: a *violator* is an agent in view whose act of the same type is under way
(from the act until its episode's end, e.g. from crossing a red line until it leaves the
junction) and that is moving (above `observers.stop.speed_below`). A *complier waiting* is an
agent in view in an episode of the same type that has not acted and is standing. Counts are taken
at each evaluation, with no memory. `violations.imitation.any_type = 1` (a `scenario` switch,
default 0) counts violators of any type (Keizer et al., a G§4.3 sensitivity case); compliers stay
same-type.

## Part 4. The field of view

1. **F1: a sector in world coordinates (proposed).** From the agent's front centre, radius `R`
   (`violations.imitation.fov_radius`), half-angle `φ` (`…fov_half_angle`) about its heading; an
   agent counts if its centre is inside. Candidates: agents on the agent's own ribbon within `R`
   (from the ribbon order), plus, within `R` of a junction, the agents that junction already lists
   each step (ADR-0016 part 4), so a rider at the line sees those crossing into the box. No
   occlusion. Run only for agents in an episode, at their evaluations.
2. **F2: along its own path only:** agents on its own ribbon and route within `R`, no angle.
   Cheaper; misses violators on other approaches and what is beside it.
3. **F3: F1 with occlusion:** a bus or truck between the agent and the other hides it (the split
   sign rule of ADR-0017 does this for one vehicle ahead). More realistic for a two-wheeler
   behind a bus; a ray test per pair.

Values *(mine, grade C, sweep; the register has none)*: `R` = 30 m (about five rows of a queue),
`φ` = π/2 (everything ahead of the driver's shoulders; mirrors not modelled).

## Part 5. How κ combines with the per-class spreads

The register's per-class distributions (`v0`, `T`, `s0`, `a`, `b`, critical gaps) are measured or
fitted spreads across drivers. A link from κ to them can work three ways:

1. **K1: on top of the draws (plan/03 as written):** `v0_i = v0_draw·(1 + γ_v κ)`,
   `T_i = T_draw·exp(−γ_T κ)`, … The realised spread is wider than the register's
   (`Var ln T = σ_T² + γ_T² σ_κ²`), so fitting a link at stage 6 changes the free-speed and
   following spreads frozen at stages 1–2 (CLAUDE.md rule 4).
2. **K2: κ decides where in the distribution an agent draws (proposed; a Gaussian copula).**
   `X_i = F_X⁻¹(Φ(r_X·z_i + √(1 − r_X²)·ε_i))`, with `z_i = (κ̄_i − μ_κ)/σ_κ` and `ε_i` the
   agent's own normal score from its existing draw. When κ has the register's population
   distribution, every `X` keeps its register marginal exactly; `r_X` (a correlation in
   `[−1, 1]`, `personas.links.<parameter>`) only decides who gets the fast speeds and short gaps.
   A persona with a higher mean κ then draws higher up the same distribution. `r_X = 0` keeps
   today's sampler, so outputs do not move until a link is set. Point-valued scales (clearance,
   politeness) keep the multiplicative form already built (`exp(−γ_c κ)`, ADR-0010); the existing
   `personas.links.critical_gap` (0) becomes a correlation of this kind.
3. **K3: κ only where there is no measured spread** (clearance, politeness, critical gap,
   violations). `v0`, `T`, `s0`, `a`, `b` stay independent of κ. Simplest, no double counting,
   but a "rushed driver" persona cannot be faster or follow closer.

**Within-trip κ** *(mine)*: the OU process and the jump on delay move κ(t) in the decision rule
and the clearance scale (and politeness, once it has a per-agent link); the copula-linked draws
(following parameters and the critical gap) keep the values drawn at entry (re-mapping them every
tick would cost about 10% of a step, over the 5% budget). As built (P8.2):

- κ(t) takes one **exact** OU transition at each of the agent's decision ticks,
  `κ' = κ̄ + (κ − κ̄)e^{−θΔ} + s·√(1 − e^{−2θΔ})·N(0, 1)`, so its law does not depend on `Δt_dec`.
  The register gives the reversion rate `θ` and the **stationary SD** `s`
  (`personas.traits.within_trip.sd`, replacing the unset `…volatility` "per √hour", which had no
  SI unit); the noise is `s·√(2θ)`.
- The jump `J` comes **once per trip**, when the accumulated delay (time so far minus free-flow
  time so far) first passes the threshold, and then reverts like any other deviation.
- Both off unless set (they are unset `fit` entries, stage 6).

## Part 6. For P8.2 *(mine, please review)*

- A persona is a mean shift of the trait vector in units of each trait's register SD
  (`personas.<persona>.shift.<trait>`, grade C, sweep), translating G§3.2's words: at mean 0,
  low −1, moderate +0.5, high +1, very high +1.5. Within a persona the traits are independent
  normals with the register's SDs. The population's realised moments are reported per run.
- Personas are tied to vehicles by G§3.2's "typical vehicle" column: an agent's persona is drawn
  from those eligible for its vehicle type, with the register's shares renormalised among them
  (every e-rickshaw is an e-rickshaw driver; a motorcycle is an average commuter, rushed driver,
  delivery rider or bike-taxi rider). The e-rickshaw share is then a composition share and the
  delivery-rider share a share within two-wheelers (P8.8).
- With no persona shares set, every agent has the register's trait distributions and no persona
  label: today's behaviour and output hashes (10 reference scenarios compared, all identical).
- **A population is all ten shares or none.** Setting some and not others is refused, naming the
  missing ones (a share of 0 is allowed): otherwise setting only `delivery_rider` would make every
  two-wheeler a delivery rider. Within a class whose eligible shares are all 0, agents take the
  class's first eligible persona (the e-rickshaw driver for an e-rickshaw). Tractors, which G§3.2
  does not list, take the goods-vehicle driver.
- Word map as applied: "moderate" and an unqualified mention ("schedule pressure") are +0.5. The
  ten personas' shifts are 70 register entries (`personas.shift.<persona>.<trait>`); personas
  whose G§3.2 description names no trait (average commuter, cyclist) have all zeros. Signatures
  that are not traits (low speed, stop propensity, wrong-way propensity) come from the vehicle
  class or, in P8.4, from per-persona intercept shifts.
- **Distraction episodes (the user's decision, 2026-11-14: "stop re-deciding").** A phone user is
  an agent whose distraction trait is in the top `personas.traits.phone_use_share` of the
  population, so a persona with a higher distraction mean has more phone users. A phone user
  alternates between attentive periods (exponential, `personas.distraction.episode_rate` per
  hour) and episodes (`personas.distraction.episode_duration`); every agent starts attentive. While
  distracted it keeps its lateral target (no scheduled or leader-change decision) and, from P8.3,
  does not re-read violation triggers (their clocks pause). Repulsion from neighbours and the
  collision guard still act. No reaction delay is added (G§2.3). Rate and duration have no guide
  numbers: unset, and a run with phone users but without them is refused.

## Part 7. Built in P8.3 *(mine, please review)*

- **Engine** (`crates/marg-core/src/violations.rs`). An episode per (agent, type) while the
  trigger holds; the uniform comes from the agent's `Decisions` stream at
  `8·2⁴⁰ + type·2³⁶ + episode`. The rule is read at the step the trigger first holds, at the
  agent's staggered decision ticks, and at the step after anybody acts (the counts in view
  changed). An episode whose agent no longer sees its trigger ends as `complied` (reason
  `gap_accepted`, `trigger_ended` or `left`); an act stays open, counted as a violator in view,
  until the agent has left the junction.
- **Logged per decision**: the features, the agent's own coefficients (traits applied), every
  term, `η`, and `λ`, `Λ`, `E`, `1 − exp(−Λ)` (continuous) or `σ(η)` and the draw (one-shot). The
  coefficients are logged because traits scale them (part 2): without them a term could not be
  checked against its feature. The delivery rider's intercept shift (stage 6) is added to the
  intercept term.
- **Refusals**: a type whose intercept is set needs every shared coefficient (the guide has no
  numbers; 0 is allowed); a type not built yet is refused rather than silently ignored.
- **The first type: `forced_entry`** (G§2.4, §4.2). Trigger: standing at the line of an
  uncontrolled junction (not a roundabout: the ring is not robust yet, ADR-0009) with a gap
  offered and turned down. Features: κ(t); waiting time since it stood; **risk = the gap's
  shortfall, `1 − gap / critical gap`** (the critical gap includes the heavy-stream factor, so
  the size of the conflicting vehicle is in it); time saved 0 (no estimate of the wait to come
  yet); enforcement 0 (P9); imitation counts from the field of view. Act: it opens its own line,
  as a gap taken does; inside, the junction's arbitration decides who goes first (ADR-0016 part
  4), so the major-road vehicle brakes for it. Distracted, it does not re-read (its clock
  pauses). A driver still held standing at the same line when no gap is read that step (something
  in the junction holds it first) keeps its episode, its clock paused: ending and restarting the
  episode each time would draw a new threshold and under-count violations (first built that way:
  about six times as many episodes, a fifth as many acts per episode).
- **Viewer**: the inspector shows the persona, the traits and each open episode's coefficients,
  features, terms, `η`, `λ`, `Λ`, `E` and probability (protocol 3).

## Decision

**T2 + I1 + F1 + K2** (the user, at the gate): continuous triggers run an integrated-hazard
clock with `λ = λ_max·σ(η)`; imitation is two log counts; the field of view is a sector in world
coordinates without occlusion; κ links to parameters with measured per-class spreads through a
Gaussian copula. The parts marked *(mine)* are built as written unless you change them.

## Parameters (new unless noted)

| Symbol | Register path | Value | Grade, role | Source |
| --- | --- | --- | --- | --- |
| `λ_max` | `violations.hazard.max_rate` | 1 s⁻¹ | C, fix | about once per perception–reaction time (Wong & Goh, G§2.3) |
| `γ_π, γ_ρ, γ_c, γ_η` | `personas.links.time_pressure`, `.risk_tolerance`, `.conformity`, `.norm_sensitivity` | 0.5 | C, sweep | assumed (part 2) |
| `R`, `φ` | `violations.imitation.fov_radius`, `.fov_half_angle` (exist, unset) | 30 m, π/2 | C, sweep | assumed (part 4) |
| any-type switch | `violations.imitation.any_type` | 0 | C, scenario | Keizer et al. 2008 (G§4.3) |
| `r_X` | `personas.links.free_flow_speed`, `.time_gap`, `.standstill_gap`, `.max_accel`, `.comfortable_decel`; `.critical_gap` (exists, meaning changes) | 0 | C, fit 6 | G§3.1 names the links, not their size |
| OU, jump | `personas.traits.within_trip.reversion_rate`, `.sd` (renamed from `.volatility`), `.delay_jump`, `.delay_threshold` | unset | B, fit 6 | Kesting & Treiber 2008; RSS 2022 |
| distraction | `personas.traits.phone_use_share` (exists); `personas.distraction.episode_rate`, `.episode_duration` | unset | C, sweep | G§3.1 (no numbers) |
| intercept shifts | `violations.<type>.intercept_shift.<class>` | unset | C, fit 6 | G§4.2 "who does it most" |
| persona shifts | `personas.<persona>.shift.<trait>` | per part 6 | C, sweep | G§3.2 words |

The intercepts and the shared coefficients already in `params/base/violations.yaml` stay as they
are (unset; a run that enables a type without them is refused, as for P6's unset inputs).

## Consequences

- `crates/marg-core/src/model/decision.rs` holds the rule (terms, σ, the hazard and one-shot
  decisions, the clock) with five tests, including the invariance check above;
  `violations.rs` runs it (P8.3). With no persona share, no link, no within-trip process and no
  intercept set — the shipped register — every output hash is unchanged (10 reference scenarios
  compared before and after P8.2–P8.3).
- Cost: only agents in an episode evaluate the rule and query the field of view, at their
  decision ticks; one draw per episode. The corridor (no triggers) should not move.
- Outputs: `violation` and `complied` events with their terms; violation counts by type in
  `agents.parquet` and the KPIs; violations by phase bin in `signal_cycles` (plan/04).
- Calibration: stage 6 fits intercepts (and `r_X` if data allow) by matching shares or by the
  likelihood of the logged episodes; `λ_max` never moves.
- With K2, a per-class distribution without a closed-form quantile (gamma, mixture) cannot carry
  a link; the loader refuses a non-zero `r_X` on one. Truncated normals gain a quantile.
