# ADR-0016: Junctions — routes, stop lines, gaps and vehicles on other ribbons

- **Status:** proposed (rules I added while building P7.1–P7.4; **please review**). The
  cross-ribbon rules in part 4 **do not work yet in dense traffic** (open problem 1)
- **Date:** 2026-11-13
- **Phase / task:** P7.1–P7.4, P7.7, read by P7.5, P7.6, P7.8, P8, P11

## Context
plan/03-model-design.md §4 and §8 say what a junction must do (signals, gap acceptance, forced
entry later) but not how ribbons meet, how a vehicle knows where it is going, or how vehicles on
different ribbons see each other. The network could only hold chains of ribbons.

## What was built

### 1. Geometry and routes
- `network.junctions` lists arms (a ribbon in, a ribbon out), turning shares per arriving arm and
  the control. Three kinds: `box` (one wide movement ribbon per arriving–leaving pair, a cubic
  curve between the two ribbon ends, width interpolated), `split` (direct handover to ribbons
  that share the arriving ribbon's frame, each with part of its width) and `roundabout`
  (ADR-0009).
- Where two ribbons inside a junction overlap is found by sampling (every 0.5 m, both edges and
  the middle) and stored as the stretch of each inside the other, typed crossing, merge or
  diverge.
- A vehicle's turn at each junction is a pure function of (seed, agent id, junction, arriving
  arm), from the new `Routes` random stream: the same agent turns the same way in every arm of
  a twin experiment. Every walk along a vehicle's path (leaders, the collision guard, the
  carriageway bounds) follows its own route. Road objects, the effective-width observer and
  parking still stop at a junction: **an object beyond a junction is not seen until the vehicle
  is past it.**
- Two ribbons may now share a `next` (a plain merge). Looking back, a vehicle follows the ribbon
  it came by.

### 2. The stop line
The end of an arriving ribbon is its stop line. It is closed to a vehicle, as a standing
virtual vehicle across the road (the mechanism of barriers), when any of these holds:
- **the signal** shows its movement red or amber. At the start of amber a vehicle that is
  moving and cannot stop before the line at its own comfortable deceleration is let through;
- **keep clear**: a vehicle of a *crossing* movement is in the area the two movements share. A
  movement the signal has just stopped counts from its own stop line on, so its last vehicles are
  let out. *This rule is mine; the card asks for "conflict-area occupancy" without saying what it
  does;*
- **it must give way and has no gap** (part 3).

Vehicles may stop inside a junction when their way out is blocked. Choosing to enter anyway
(`box_blocking`) is P8.

### 3. Gap acceptance (G§2.4)
- Who gives way: at an uncontrolled box with `major` arms, the ranks of two-way stop control
  (HCM 2010, ch. 19) mirrored for left-hand traffic. With no major arm nobody waits for a gap.
  Under a signal, a right or U-turn gives way to movements that have green with it. At a
  roundabout, joining connectors give way to the ring.
- The gap: time from the vehicle's own arrival at the line to the arrival, at the shared area,
  of the conflicting vehicle that leaves least to spare. It is taken if it is at least the
  vehicle's critical gap (per agent, lognormal by class; × its heavy-stream factor against a bus,
  truck or tractor; × `exp(−γ_g κ)`, γ_g = 0 until P8). A vehicle decides only once it is within
  its desired gap of the line. A conflicting vehicle that stands still and is not accelerating
  is not coming.
- Each acceptance writes a `gap_accepted` event with the gap, the critical gap, the largest gap
  turned down, how many were turned down and the wait: the data a critical-gap estimate needs.
  Several vehicles can take one gap; nothing prevents it.

### 4. Vehicles on other ribbons (P7.2) — not working in dense traffic
Vehicles near a junction are listed in world coordinates each step. A vehicle places each one
that is on a ribbon off its own path onto that path (nearest point, its body's extent along and
across the path, its speed resolved along and across) and then:
- **ahead and in the way:** follows it (IDM). If each is ahead of the other, the lower id goes;
- **ahead and coming across:** both estimate when each reaches the crossing point at its own
  maximum acceleration; the later one follows, the earlier one carries on;
- **level:** no steering towards it. If their paths close, the one on the straighter ribbon goes
  first, else the one ahead, else the lower id; the other brakes at its comfortable rate.

The collision guard between ribbons works in world coordinates and puts back whichever vehicle
drove into the other this step.

## Open problems
1. **Vehicles lock inside the junction in dense traffic.** `scenarios/signal_crossing.yaml`
   (700 veh/h per arm): from about 73 s two vehicles that left the west arm side by side, one
   going straight on and one turning left from its right, are held by the guard every step
   (17,000 guard events each over the run) and the junction jams behind them. The rules above
   decide who goes first but give the one that waits no way to drop back, and the lateral choice
   does not see vehicles on other ribbons at all. The test
   `dense_traffic_crosses_without_guard_events` is checked in, ignored. Options:
   - **A.** One frame per arriving arm inside the junction: vehicles from one arm stay
     neighbours in the ordinary sense until their ribbons have separated, so the existing
     lateral model and guard apply. Needs a ribbon per arm fan, not per movement.
   - **B.** Give the lateral choice the same view of other ribbons as the following model has.
   - **C.** Make turns hard at boxes too: a turn can only be made from its own side of the road,
     so vehicles from one arm never cross inside the junction. Simple; least like Indian traffic.
   I lean to A. **Your decision.**
2. A right-turner waiting for a gap stands at the stop line and blocks its side of the approach.
   Real drivers wait inside the junction and clear at the end of the stage.
3. Light traffic works (test `light_traffic_obeys_the_signal_and_is_deterministic`). Nothing
   else in P7.3–P7.8 has been validated: no saturation flow, no critical-gap recovery, no
   discharge acceleration.
4. Performance: the pair search near a junction is quadratic in the vehicles there, and the
   signal scenario ran at about 2,800 ns per agent-step against about 1,100 on the corridor.
   Corridor scenarios are unaffected (same output hash; the junction code is not entered).

## Consequences
- Scenario schema, Python and TypeScript types and the data dictionary changed
  (`signal_cycles.parquet`, six new event kinds, four new KPIs).
- Per-agent draws gained four blocks (critical gap, heavy factor, late realiser, awareness
  distance). Existing draws keep their blocks: reference scenarios keep their output hashes.
