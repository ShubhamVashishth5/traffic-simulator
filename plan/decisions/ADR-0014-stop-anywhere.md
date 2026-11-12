# ADR-0014: Stop-anywhere events: how a vehicle stops and where the events come from

- **Status:** accepted and implemented (P6.3). **The user decided on 2026-11-12:** question 1,
  pull over (A) with a configurable share of stops that do not (B), by class; question 2,
  a rate per vehicle-kilometre (A). See "Decision" at the end.
- **Date:** 2026-11-12
- **Phase / task:** P6.3 (governs P6.4)

## Context

P6.3 asks for "persona-driven Poisson hail and stop events (autos, e-rickshaws, cabs, bike
taxis), with lognormal dwell; attractors (metro exits, markets) that raise the rate locally".
Plan/03 gives it two table rows: the violation `stop_anywhere` ("stop at the kerb or in the
stream for a dwell time") and the attractor ("raises the `stop_anywhere` hazard").

Three things are not settled there, and each changes results:

1. **How the vehicle stops.** This is a new behaviour: so far no agent chooses to stand still.
2. **Where events come from.** Personas arrive in P8; the guide grades the rates C and gives
   no number. The register has one related entry, `violations.stop_anywhere.intercept`
   (unset, stage 6), which belongs to P8's logistic rule.
3. **What P6.3 owns and what P8 owns.**

Since ADR-0012 part 2 (option A) a standing vehicle is passed by every class, so a stopped
auto no longer gridlocks the road behind it.

## Question 1. How does a vehicle stop?

**A. Pull over, and stop wherever it has got to (recommended).** At an event the vehicle is
given a stop point ahead, at its comfortable braking distance from its current speed. Its
lateral target becomes the kerb, through the normal lateral choice and feasibility (ADR-0010),
and it brakes for the stop point as for a standing obstacle. It dwells, then drives on. If
traffic kept it from reaching the kerb, it stands where it is, in the stream.
- For: "at the kerb or in the stream" comes out of one rule, by how busy the road is. No new
  parameter for the manoeuvre. The vehicle stays an agent (its trip, delay and stops are
  recorded as usual).
- Against: a new term in the lateral choice (a pull to the kerb while stopping) and a new
  agent state; both need micro-scenarios.

**B. Stop in place.** The vehicle brakes at its comfortable rate where it is and dwells.
- For: smallest change.
- Against: every stop is in the stream. Autos mostly pull left; this would overstate the
  blocking, and stage 5 would then be fitted to compensate.

**C. Turn the vehicle into a road object for the dwell.** Remove the agent, put an object at
the kerb, re-insert the agent afterwards.
- For: reuses P6.2.
- Against: the trip is broken in two, the vehicle jumps to the kerb, and re-insertion needs
  the gap rules of an entry.

## Question 2. Where do the events come from, before personas exist?

**A. A rate per vehicle-kilometre by class, multiplied inside attractors (recommended).** Each
eligible vehicle draws the distance to its next event from an exponential (its own random
stream, ADR-0003). An attractor is a stretch of a ribbon with a multiplier on the rate
(`network.attractors`). This is the guide's "Poisson by persona".
- For: simple; rates per class are what a video count gives (stops per vehicle per km);
  twin arms see the same events.
- Against: events scale with the flow of eligible vehicles, whether or not anyone is hailing.

**B. A rate per vehicle-second.** As A, in time.
- Against: a queue then produces stops by standing still, which is not what is observed.

**C. Roadside hails.** Passengers appear on the kerb as a Poisson process in space and time
(per 100 m per hour, higher at attractors); each is taken by the next eligible vehicle that
passes and is free.
- For: closest to what happens at a metro exit: the number of stops is set by the passengers,
  not by how many autos pass.
- Against: needs an "occupied" state per vehicle and a rule for which vehicle takes a hail;
  drop-offs (the other half of the stops) still need a per-vehicle rate, so it is A plus more.

## Question 3. P6.3 and P8

Proposed, with either answer above: P6.3 builds the mechanism (event, stop, dwell, attractor,
outputs) with a constant rate per class. P8 replaces the constant by the logistic propensity
with persona and context terms, and nothing else changes. Stop-anywhere events are logged
with their rate and its terms from the start (rule 9).

## Register entries either way (all unset: the guide has no numbers)

| Entry | Unit | Note |
| --- | --- | --- |
| `road.stop_anywhere.rate.<class>` | 1/km (A) or 1/h (B) | auto-rickshaw, e-rickshaw, car (cabs), two-wheeler (bike taxis); grade C, `fit`, stage 5 |
| `road.stop_anywhere.dwell` | s | lognormal; grade C, `fit`, stage 5 |

As with parking (ADR-0013), a scenario that switches the mechanism on must supply them, and a
run without them is refused.

## Decision (2026-11-12)

**Question 1: A and B together, by a knob.** In the user's words: pull over, "but with some
distribution of absurdity like how e-rickshaws stop, not necessarily pulling over", kept as a
configurable knob. So at each event the vehicle makes for the kerb with probability
`road.stop_anywhere.pull_over_probability.<class>`, and otherwise stops where it is, holding
its line. **Question 2: A.** Question 3 as proposed.

### As implemented

1. Every vehicle of a class with a rate draws the travel to its next event from an
   exponential (its own decision stream, one block per event). Travel inside an attractor
   counts `stop_rate_multiplier` times (`network.attractors`).
2. At the event it draws pull-over or not, and a dwell. Its stop point is ahead by the gap
   its following model keeps to something standing in its way,
   `s0 + v·T + v²/(2·√(a·b))`, so it brakes comfortably. No new parameter.
3. It brakes for the stop point as for a standing obstacle (the same term as a closed
   barrier). Pulling over, its lateral target is the kerb (the left edge, `y_max`); it
   tracks it within the side neighbours' limits (ADR-0010 §4), without the lateral choice.
   Not pulling over, it keeps the target it had.
4. When it comes to a stand it stays where it is for the dwell (it does not creep on to the
   kerb), then drives on and draws its next event.
5. It is an agent throughout. Others pass it as any standing vehicle (ADR-0012 part 2). Its
   dwell counts in its delay and its stops.

**Rule added (please review):** item 4, and in item 3 that a pulling-over vehicle skips the
lateral choice (so the MOBIL safety criterion is not applied to it; the per-step limits that
stop it cutting in on a vehicle that could not avoid it still are).

### Register (all unset; the guide has no numbers)

`road.stop_anywhere.rate.<class>` (1/km) and
`road.stop_anywhere.pull_over_probability.<class>` for auto-rickshaw, e-rickshaw, car and
two-wheeler; `road.stop_anywhere.dwell` (s). Grade C, `fit`, stage 5. A class with a rate but
no pull-over probability or dwell is refused, and so is an attractor when no class has a rate.

### Outputs

Events `stop_anywhere` (rate, multiplier, pull_over, its probability, dwell, distance to the
stop point: the features of the decision, rule 9) and `stop_anywhere_standing` (how far from
the kerb it stood).

### Results

`scenarios/stop_anywhere_metro_exit.yaml` (placeholder inputs; 2,200 veh/h, 12% autos, 5%
e-rickshaws, rate × 6 along 200 m), seeds 1–3:

| | Seed 1 | Seed 2 | Seed 3 |
| --- | --- | --- | --- |
| Events (of them along the 200 m) | 288 (124) | 257 (105) | 309 (147) |
| Autos that pulled over: median distance from the kerb (m) | 0.0 | 0.0 | 0.0 |
| Autos that did not | 5.9 | 6.3 | 6.6 |
| E-rickshaws that pulled over | 1.6 | 1.1 | 2.2 |
| Speed before / at the exit (km/h) | 38.8 / 29.7 | 38.8 / 32.0 | 38.0 / 27.4 |
| Guard events | 0 | 0 | 0 |

E-rickshaws that pull over often stand a metre or two out: at 21 km/h their stop point is
close, and they stand where they have got to.

Tests (`tests/stopping.rs`): events per vehicle-kilometre within Poisson error (about 900
events), the pull-over share and the mean dwell as asked, the attractor's ratio within 20%,
pulled-over vehicles at the kerb on a quiet road (median under 0.3 m) and the others on their
line (moved under 0.3 m sideways), traffic passing stopped vehicles at 2,400 veh/h, refusals,
determinism across threads. Reference corridor: same output hash; benchmark unchanged within
noise.

## Recommendation (before the decision)

1A and 2A, with P8 taking over the rate. This is a recommendation only.
