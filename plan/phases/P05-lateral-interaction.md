# P5 · Lateral behaviour, 2-D interaction and conflicts (stage 3)

**Goal:** lane-free lateral movement — filtering, seepage, overtaking on both sides, squeezing — built
from the clearance model, plus online 2-D conflict detection. This is the core of Indian mixed
traffic, so it gets the most careful testing in the plan.

- **Depends on:** P2, P3; P4 is strongly recommended for debugging.
- **Read first:**
  - guide §2.2, §2.1 (partial following), §4.4;
  - [03-model-design.md](../03-model-design.md) §2–3;
  - Kanagaraj & Treiber (2018) and Budhkar & Maurya (2017) — the actual papers, not summaries.

## Tasks

### P5.1 ADR: the exact lateral formulation
- **Do:**
  - Read Kanagaraj & Treiber (2018). Compare it with the ⚑ proposal in model design §3.
  - Settle whether Budhkar & Maurya's `C` is centre-to-centre or edge-to-edge.
  - Write `decisions/ADR-00xx-lateral-model.md` with the final equations, every parameter mapped to
    a register entry, and where the formulation departs from the paper and why.
- **Done when:** the user approves the ADR (this is a review gate).

### P5.2 Clearance model
- **Do:** pair clearance `c_ij(v̄)` with the pair-type slope and intercept modifiers and the κ
  scaling; register entries for every modifier.
- **Done when:** unit tests reproduce 1.46 m at 40 km/h and 1.33 m at 20 km/h for the baseline pair.
  Pair ordering: 2W–2W < same-type < mixed.

### P5.3 Lateral target choice
- **Do:**
  - Candidate positions and the utility (own advantage, politeness, route, band, edge, move cost).
  - The feasibility check through side neighbours.
  - Multi-rate evaluation (`Δt_dec`, staggered by agent ID) with event triggers.
- **Done when** these micro-scenarios pass:
  1. A slow leader centred in a wide road → the follower passes on the side with more room.
  2. Gaps on both sides → the choice depends on politeness and route side.
  3. A gap narrower than the clearance → the agent does not enter.

### P5.4 Lateral motion and steering
- **Do:** relaxation towards the target, steering repulsion, the heading limit, and creep speed for
  two-wheelers in queues.
- **Done when:**
  - |heading| ≤ θ_max always (property test).
  - Two-wheelers reach the front of a stopped queue through the gaps; cars do not.

### P5.5 Collision guard
- **Do:** an OBB overlap check after integration, step cut-back, and a `collision_guard` event.
- **Done when:** a property test with 10⁴ random dense scenarios logs zero overlaps after the guard,
  and the guard-event rate is reported. Calibrated scenarios must show 0.

### P5.6 Moving bottleneck and seepage scenarios
- **Do:** reference scenarios: an e-rickshaw at 21 km/h in a stream of cars and two-wheelers;
  a queue at a closed barrier (signals arrive in P7).
- **Done when:**
  - The e-rickshaw is passed on both sides.
  - Lateral position histograms by class differ (two-wheelers spread wider).
  - The results are rendered in a phase report.

### P5.7 2-D conflicts
- **Do:**
  - TTC between oriented rectangles under constant velocity over a short horizon, using cached
    neighbours.
  - Conflict episodes, and a `conflicts` table with min TTC, DRAC and type.
  - The threshold comes from the register.
- **Done when:**
  - Analytic test cases (head-on, rear-end, lateral sideswipe) match hand-computed TTC.
  - The overhead is under 15% of step time.

### P5.8 Stage-3 tooling
- **Do:**
  - Distance functions between simulated and observed lateral position distributions (by class) and
    partial densities.
  - A stage-3 calibration run on synthetic data (recovery test).
  - Plug in real trajectories when available (Chennai set, Kanagaraj et al.; see open questions).
- **Done when:** the synthetic recovery of clearance coefficients and lateral scales succeeds, or the
  parameters that cannot be recovered are reported and demoted via an ADR.

### P5.9 Performance re-check
- **Do:** re-run the corridor benchmark with lateral behaviour on.
- **Done when:** the corridor target is still met; if not, record a profile and optimisation tasks.

## Milestone check
On a medium-density mixed corridor:

- partial densities by class are in the range of Kanagaraj & Treiber;
- speed–flow points sit plausibly against the capacity–speed curve;
- zero guard collisions;
- the viewer shows two-wheelers filtering and seeping.

The phase report includes lateral histograms by class.

## Out of scope
Obstacles (P6), junctions (P7), personas beyond the κ placeholder (P8).
