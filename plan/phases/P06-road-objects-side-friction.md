# P6 · Road objects and side friction (stage 5)

**Goal:** potholes, parked vehicles, stop-anywhere events, bus stops, humps, gradients, waterlogging
and midblock pedestrians, all acting **only** through usable width, speed caps, gap supply and
incentives. Published capacity losses must *emerge* from these mechanisms; they are not inputs.

- **Depends on:** P5.
- **Read first:**
  - guide §6 (rules at the top), §6.1–6.5, §6.8–6.10;
  - [03-model-design.md](../03-model-design.md) §5, §10.

## Tasks

### P6.1 Object framework
- **Do:**
  - Objects with position `(ribbon, s, y)`, footprint, lifetime (static or event-driven) and an
    `ObjectResponse` per class.
  - Spatial indexing alongside agents, so lateral logic sees objects as static neighbours.
  - Live add and remove through the server commands stubbed in P4.
  - **No API exists for capacity or flow multipliers.**
  - Width changes in a ribbon's own profile are seen ahead the same way. *From P5.5:* the
    lateral model reads the width only where the vehicle is, so where the road narrows the
    carriageway pushes vehicles into their neighbours. In the P5.5 property test every
    collision-guard event (964,509 in 2,050 of 4,086 random bottleneck scenarios) comes from
    narrowing, and none from the 5,914 scenarios without it.
- **Done when:** an object placed in the viewer is avoided by agents, and the P5.5 bottleneck
  scenarios log no guard events.
- **Outcome (2026-11-12):** see [ADR-0012](../decisions/ADR-0012-road-objects-and-narrowing.md).
  Guard events in the bottleneck scenarios fell from 964,509 to 136, not to zero. Standing
  bodies gridlocked traffic above about 1,000 veh/h until part 2 of the ADR was decided
  (option A, every class can creep: 93 guard events, one parked car passes 1,194 of 1,200 veh/h).

### P6.2 Kerbside parking and double parking
- **Do:**
  - Parked vehicles drawn by kerb occupancy, with an offset-from-kerb distribution and a
    double-parking probability.
  - Parking and unparking manoeuvres as blocking events with a lognormal duration.
  - Cruising agents slow near the kerb.
- **Done when:** the effective-width observer reports `W_eff` consistent with the footprints plus the
  clearance-derived shy distance.
- **Outcome (2026-11-12):** done; the rules are in
  [ADR-0013](../decisions/ADR-0013-kerbside-parking.md) (proposed, please review). `W_eff`
  from the observer agrees with footprints plus the clearance model's shy distance within
  0.04 m over a 500 m stretch (three seeds). The guide has no numbers for the survey
  quantities, so the register leaves them unset and a scenario must give them.

### P6.3 Stop-anywhere and attractors
- **Do:** persona-driven Poisson hail and stop events (autos, e-rickshaws, cabs, bike taxis), with
  lognormal dwell; attractors (metro exits, markets) that raise the rate locally.
- **Done when:** events appear at the expected rates per class over seeds.
- **Outcome (2026-11-12):** done, by [ADR-0014](../decisions/ADR-0014-stop-anywhere.md) (your
  decision: pull over or not by a per-class probability; events per vehicle-kilometre).
  Events come at the asked rate within Poisson error; an attractor multiplies it locally.

### P6.4 Bus stops
- **Do:**
  - Bus dwell (lognormal), with stopping longitudinal and lateral offsets.
  - Passengers waiting on the carriageway as width-taking obstacles.
  - Autos and e-rickshaws clustering near stops (attractor).
- **Done when:** a Warangal-type four-lane case shows a capacity drop and a speed drop of a
  plausible size (the target band file).
- **Outcome (2026-11-12):** done ([ADR-0015](../decisions/ADR-0015-bus-stops-surface-pedestrians.md)).
  The drop depends on inputs the guide does not give (where the bus stands, how often it comes):
  at the kerb 1.5% of capacity and 13% of speed, out in the road beyond waiting passengers 32%
  and 53% (with swing-out; 40% and 74% before it). The published 7.9% and 22% lie between (`reports/phases/P6.md`).

### P6.5 Potholes and roughness
- **Do:**
  - A Poisson pothole field (kerb-biased), lognormal size and depth.
  - Detection distance as a function of speed, light, water and leader occlusion.
  - Multinomial response by class (swerve, slow, straddle, hit) scaled by κ; a swerve goes through
    P5 feasibility.
  - Crossing-speed caps by class; IRI → `v0` reduction by class.
- **Done when:**
  - Two-wheelers mostly swerve and heavy vehicles mostly slow.
  - A potholed-section scenario lands near the ~36% free-flow speed loss target *without* tuning
    toward it (report the gap if not).
- **Outcome (2026-11-12):** done (ADR-0015). Responses follow the probabilities given (tested one
  choice at a time). Free-flow speed loss 26% at 40 potholes/km and 51% at 120/km with
  placeholder responses; the source does not report its density, so 36% is within reach, not
  matched.

### P6.6 Humps, gradients, waterlogging
- **Do:**
  - Humps: a class × geometry speed cap, plus the two-wheeler edge bypass where the hump does not
    reach the kerb.
  - Gradients: `v0` loss per % by class; e-rickshaw climbing speed from the power cap.
  - Waterlogging: closes width, applies the depth–speed cap, hides potholes.
- **Done when:** hump crossing speeds fall within the 14–23 km/h band; a loaded e-rickshaw on a 4%
  ramp becomes a moving bottleneck.
- **Outcome (2026-11-12):** humps done: about 18 km/h on the hump for every class, and
  two-wheelers go round the end of a short one. **The ramp check does not hold as written:** 2 kW
  and 700 kg allow 26 km/h on 4%, above the e-rickshaw's 21 km/h free speed, so the power cap
  does not bind there (it does on 8%: 13 km/h). The e-rickshaw is the slowest vehicle on a 4%
  ramp only because it already is on the level.

### P6.7 Midblock pedestrians
- **Do:**
  - Pedestrian agents with Poisson crossing demand per 100 m and a crossing-point distribution.
  - Group size; an accepted-gap distribution; rolling crossings with a centreline wait.
  - Blocked footpaths push pedestrians onto the carriageway.
- **Done when:** a six-lane midblock sweep over pedestrian flow shows practically no capacity effect
  below ~200 ped/h and rising losses above it.
- **Outcome (2026-11-12):** done (ADR-0015). Capacity loss 0% at 100 and 200 crossings per hour,
  14% at 400, 32% at 800, 46% at 1,600 (placeholder gap and group size; not tuned). At the
  highest flows some vehicles drive into a pedestrian and are put back by the collision guard
  (42 events in 1,900 crossings at 1,600/h, none at 200/h or below).

### P6.8 Stage-5 experiment: width–capacity curve
- **Do:** a P3 experiment sweeping kerb occupancy (0–40% width loss) on Varanasi-like geometry, with
  replications and capacity estimated per arm.
- **Done when:** the validation scorecard includes the width-loss vs capacity-loss pattern. A
  near-1:1 relationship is the target; report the observed slope with its CI.
- **Outcome (2026-11-13):** run and reported; **the target is missed.** Slope 1.34 ± 0.14 against
  effective-width loss with swing-out (1.63 ± 0.11 on 2026-11-12, before it; target 1.0 ± 0.2). A plain narrower road gives 0.97 ± 0.02, so width
  itself behaves. The excess is made where the parked row begins, where the kerb strip merges
  into the stream. Swing-out (ADR-0012 option B) cut it; letting stopped vehicles slide sideways
  fast (a diagnostic) removes most of the rest, so what remains is how readily vehicles merge and
  are let in (stage 3, P8). Not tuned (`reports/phases/P6.md`, section 1).

## Milestone check
**Outcome (2026-11-12):** the pedestrian threshold is reproduced untuned. The bus-stop drop is
bracketed, not reproduced: it turns on inputs the guide lacks. The Varanasi curve is **not**
reproduced: capacity falls 1.3 times as fast as effective width (1.6 before swing-out). The
diagnosis follows the guide's rule (the fault is in stage 3): how readily vehicles merge at the
head of a parked row.

The Varanasi curve, the pedestrian threshold and the bus-stop capacity drop are reproduced
**without** being tuned in directly. Where a target is missed, the gap is diagnosed: per the guide,
the fault is in stage 3 or 5, never in the desired speeds.

## Out of scope
Junction objects (P7), work zones and animals (P9), rain as a bundle (P9).
