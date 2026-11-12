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

### P6.3 Stop-anywhere and attractors
- **Do:** persona-driven Poisson hail and stop events (autos, e-rickshaws, cabs, bike taxis), with
  lognormal dwell; attractors (metro exits, markets) that raise the rate locally.
- **Done when:** events appear at the expected rates per class over seeds.

### P6.4 Bus stops
- **Do:**
  - Bus dwell (lognormal), with stopping longitudinal and lateral offsets.
  - Passengers waiting on the carriageway as width-taking obstacles.
  - Autos and e-rickshaws clustering near stops (attractor).
- **Done when:** a Warangal-type four-lane case shows a capacity drop and a speed drop of a
  plausible size (the target band file).

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

### P6.6 Humps, gradients, waterlogging
- **Do:**
  - Humps: a class × geometry speed cap, plus the two-wheeler edge bypass where the hump does not
    reach the kerb.
  - Gradients: `v0` loss per % by class; e-rickshaw climbing speed from the power cap.
  - Waterlogging: closes width, applies the depth–speed cap, hides potholes.
- **Done when:** hump crossing speeds fall within the 14–23 km/h band; a loaded e-rickshaw on a 4%
  ramp becomes a moving bottleneck.

### P6.7 Midblock pedestrians
- **Do:**
  - Pedestrian agents with Poisson crossing demand per 100 m and a crossing-point distribution.
  - Group size; an accepted-gap distribution; rolling crossings with a centreline wait.
  - Blocked footpaths push pedestrians onto the carriageway.
- **Done when:** a six-lane midblock sweep over pedestrian flow shows practically no capacity effect
  below ~200 ped/h and rising losses above it.

### P6.8 Stage-5 experiment: width–capacity curve
- **Do:** a P3 experiment sweeping kerb occupancy (0–40% width loss) on Varanasi-like geometry, with
  replications and capacity estimated per arm.
- **Done when:** the validation scorecard includes the width-loss vs capacity-loss pattern. A
  near-1:1 relationship is the target; report the observed slope with its CI.

## Milestone check
The Varanasi curve, the pedestrian threshold and the bus-stop capacity drop are reproduced
**without** being tuned in directly. Where a target is missed, the gap is diagnosed: per the guide,
the fault is in stage 3 or 5, never in the desired speeds.

## Out of scope
Junction objects (P7), work zones and animals (P9), rain as a bundle (P9).
