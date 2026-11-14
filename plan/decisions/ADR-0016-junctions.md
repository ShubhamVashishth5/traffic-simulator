# ADR-0016: Junctions — routes, stop lines, gaps and vehicles on other ribbons

- **Status:** proposed (rules added while building P7.1–P7.4; **please review**). Your
  decisions of 2026-11-13: **D** for vehicles on other ribbons (open problem 1, part 4) and
  **permitted turns wait for their gap inside the box** (open problem 2, part 3). The rules I
  added to make those work are marked *(mine)*
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
  moving and cannot stop before the line **at the safe braking limit** (`interaction.lateral.
  safe_decel`, 4 m/s²) is let through; it then reaches the line within v / 2b ≤ 2.5 s, inside
  the amber. *(Mine; first built with the driver's comfortable deceleration, which let a car
  at 20 m/s through from 200 m out, 7 s into red, and into the cross street's green.)*
- **keep clear**: a vehicle of a *crossing* movement that the signal has stopped is still in
  the area the two movements share (counted from its own stop line on, so its last vehicles are
  let out). Under a signal, movements that have green together are not kept clear of each other:
  who goes first between them is decided inside the junction (part 4). A vehicle that can no
  longer stop before the line at the safe braking limit is not held. *(Mine.)*
- **its way through is not clear** *(mine)*: a vehicle stands in the junction, or within its own
  length and standstill gap beyond it on the way out, with its body across this vehicle's path
  (or, for a permitted turn, across its path as far as where it will wait). Entering anyway is
  P8's `box_blocking`; the compliant driver of P7 waits at the line.
- **under no signal, it must give way and has no gap** (part 3).

Vehicles may still stop inside a junction when something stops in front of them after they
entered.

### 3. Gap acceptance (G§2.4)
- Who gives way: at an uncontrolled box with `major` arms, the ranks of two-way stop control
  (HCM 2010, ch. 19) mirrored for left-hand traffic. With no major arm nobody waits for a gap.
  Under a signal, a right or U-turn gives way to movements that have green with it. At a
  roundabout, joining connectors give way to the ring.
- **Where it waits.** Uncontrolled and at a roundabout: at the stop line. **Under a signal (your
  decision): inside the box**, at a hold point just short of the priority stream's carriageway:
  where its body, along its path, first reaches the width of a movement that has green and
  priority over it. On green it enters; vehicles behind it pass it at the line. While it waits it
  holds its line (no lateral move). When the opposing stream is stopped by the signal there is
  nothing left to wait for and it clears ("sneakers").
- The gap: time from the vehicle's own arrival at its waiting place (stop line or hold point) to
  the arrival, at the shared area, of the conflicting vehicle that leaves least to spare. It is
  taken if it is at least the vehicle's critical gap (per agent, lognormal by class; × its
  heavy-stream factor against a bus, truck or tractor; × `exp(−γ_g κ)`, γ_g = 0 until P8). It
  decides once it is within its desired gap of that place. A conflicting vehicle that stands
  still and is not accelerating is not coming. A gap taken for a wait inside the box holds until
  the vehicle leaves the junction.
- Each acceptance writes a `gap_accepted` event with the gap, the critical gap, the largest gap
  turned down, how many were turned down and the wait. Several vehicles can take one gap.

### 4. Vehicles on other ribbons (P7.2) — decision D
Vehicles near a junction are listed each step, each with its body as other ribbons see it: the
**chord** of its path from half a length behind its centre to half a length ahead, lying along
that chord and as wide as its turned body is across its path *(mine)*. On a straight ribbon that
is its footprint on its own ribbon. On a curve a long vehicle lies inside the arc (its rear
follows inside its front) instead of its ends swinging out; and a body that creeps sideways is
widened, not rotated, as on its own ribbon (rotating a 10 m bus that creeps at 0.2 rad swings its
ends by a metre). The guard between ribbons uses the same bodies, and leaves to the guard of a
route the pairs where one is ahead of the other on its own route *(mine)*.

**Rule 1 (never drive into a body lying across the path).** In the move phase a vehicle places
each one on another ribbon on its own path. A body turned across the path beyond the heading
limit is measured by its outline: the nearest part of it inside the vehicle's lane band ahead of
its front is followed as a leader, whoever goes first. The band includes the pair's minimum
clearance, except to a vehicle that gives way to this one (then only its body must be missed, or
a turner waiting politely at the edge of a stream stops the stream it waits for) *(mine)*.

**Rule 2 (decided early, kept) and rule 3 (the one behind waits at the line).** A sequential pass
over each junction before the move phase (`arbitration.rs`, deterministic) compares every pair
whose movement ribbons overlap along the paths they will drive: each centre at its lateral
offset, sampled every 0.5 m. The *contact* is where the two come within their half-widths and
the pair's minimum clearance (half-widths widened by L²/8ρ for the chord on a curve). Two from
one arm that start apart meet only if their paths come closer than they start. Who goes first:
- **Inside the junction, first come, first served** *(mine)*. A vehicle gets a ticket when it
  commits: when it crosses its stop line, or, waiting inside for a gap, when it takes the gap
  or has nothing left to give way to. Between two with tickets the earlier goes; a vehicle with a
  ticket goes before one without. A ticket is no earlier than that of a vehicle ahead of it on
  its own movement that is in its way. This order is fixed when the later one could still stop
  at its line, never changes, and cannot run in a circle: the version that decided by estimated
  arrival times changed its mind as speeds changed and formed rings of vehicles waiting for one
  another.
- **Before the line** (neither has a ticket): a decision taken stands until one is through; a
  permitted movement under a signal gives way to a movement with priority over it; of two from
  one arm, the one behind at the stop line waits for the other; otherwise the one that would
  reach the contact first goes. A decision is taken once either is within its stopping distance
  and desired gap of the contact.
- **Physics before order:** the one that is to wait can no longer stop short at the safe braking
  limit and the other still can: the other waits.
- **A body in the way turns it round** *(mine)*: if the one that is to go is blocked (within the
  gap it keeps to a leader) by the body of the one that is to wait, and not the other way round,
  the other goes. Arbitration and rule 1 then never disagree about who can move.
- **Rings** *(mine)*: if the decisions at a junction still form a cycle of vehicles waiting for
  one another, one decision is turned round: the yielder that is not blocked, of the movement
  with priority, with the most room to its contact, then the lower id.

The one that gives way stops short of the contact (paths that cross; one from the same arm that
has not crossed its line stops at the line) or, where the paths merge, follows the other as if it
were already ahead on the shared path. While it gives way inside the junction it holds its line,
and standing inside the junction no vehicle moves sideways *(mine)*: each movement there is its
own ribbon, and the turning room that keeps a creeping body clear of neighbours on one ribbon
(ADR-0012) is fed by vehicles on other ribbons only for bodies just ahead or behind.

Outside junctions (two ribbons that share a `next`) the earlier rules stay: the one ahead goes,
the vehicle arriving later at a crossing point follows.

## Open problems
1. ~~Vehicles lock inside the junction in dense traffic.~~ Decision D, built as above. At the
   signalised crossing (`scenarios/signal_crossing.yaml`, every turn, 30 min): 300 veh/h per arm
   flows with **0 guard events on 4 of 4 seeds**; 700 veh/h per arm over 900 s with 0 guard events
   on seeds 1 and 2 (the checked-in test); through and left turns only at 700 veh/h, 0 guard
   events on 4 of 5 seeds. **Not yet robust at 500–700 veh/h with right turns**: one seed in four
   stalls part way (a chain of vehicles waiting on a slow one, cause not yet found) and some seeds
   keep a pair held by the guard for minutes (below).
2. ~~A right-turner waiting for a gap stands at the stop line.~~ Your decision: inside the box.
3. **A bus turning towards a vehicle beside it** (new): as a long vehicle's front enters its turn
   its body sweeps up to L²/8ρ (about 1.3 m for a 10 m bus on a 10 m radius) towards the inside of
   the turn, into a vehicle that drew level with it there on the approach. This is the real
   "left hook". The model has no rule for it: a two-wheeler does not know the bus will turn, and
   the bus does not check its inside before it turns. It is the cause of the remaining guard
   events (up to 0.5 m of overlap, held for up to minutes). Options: (a) a vehicle beside a long
   vehicle that will turn its way keeps out of its sweep; (b) a long vehicle does not move on
   while something is level with it within its sweep; (c) both. **Your decision.**
4. Saturation: with every turn at 700 veh/h per arm the crossing saturates (about 1,050 of 1,330
   vehicles through in 30 min). The compliant right turn is the bottleneck: the register's
   critical gaps (car median 8.15 s, from uncontrolled junctions in Warangal) are rarely offered
   by a 560 veh/h opposing stream, so right turns clear mostly at the end of green. Not tuned:
   critical gaps under a signal are a stage-4 question, forced entry is P8.
5. Performance: the signal scenario runs at about 2,700 ns per agent-step (about 1,050 on the
   corridor). The arbitration pass is quadratic in the vehicles at a junction and samples paths
   every 0.5 m. Corridor scenarios are unaffected (the junction code is not entered).

### 5. Added while running P7.4–P7.8 (2026-11-14) *(mine, please review)*
- **U-turn geometry.** A movement ribbon is a cubic Hermite curve whose end tangents were as
  long as the chord. For a half turn that makes a hairpin of about a sixth of the chord at both
  ends (1.3 m for carriageways 8 m apart), and a vehicle on the inside of the turn is then beyond
  the centre of curvature: the ribbon folds over and its world position jumps (10,414 guard
  events at the median opening). A U-turn's tangents are now `4 r tan(θ/4)` over the chord
  (twice it for a half turn), which follows a circular arc; other movements keep chord-long
  tangents, so box scenarios are unchanged.
- **Paths that share a ribbon** (`arbitration.rs`, contact). A pair was never arbitrated if
  their paths had any ribbon in common ("one follows the other"). That holds when one is already
  on a ribbon the other will drive; two that come together onto it from different ribbons (a
  merge onto a ring) meet before it, and are now compared up to it. Box paths never share a
  ribbon beyond their first, so their decisions are unchanged.
- **The rejected gaps are logged** (`gap_rejected`, when a gap is first offered), with the gap,
  the critical gap it was compared with and the class it was against (rule 9): a simulated run
  is then a gap-acceptance survey (P7.4, `marg.gaps`).
- **Entries fed through a signal (P7.7).** `upstream_signal` on an entry. With `stop_line_s`
  the signal is simulated: a barrier at that station that a fixed-time plan closes (green,
  amber, red). At amber it is closed only to a vehicle that can still stop before it at the
  safe braking limit, decided step by step (the junction's own stop line decides once, at the
  start of amber). Without `stop_line_s`, arrivals wait in the entry's queue unless the signal
  shows green (the form the schema already described; the arrivals behind are still a renewal
  process). The first lets the model's own queue discharge make the platoons; the second
  releases them at the insertion rate.
- **Roundabouts only** (ADR-0009 addendum): tickets ordered along shared arcs, no ticket order
  far from the contact, the yielder rule of part 4 applied to bodies ahead at a shallow angle,
  and "blocked" read from the move phase too. Boxes keep their behaviour (same output hashes).

## Consequences
- Scenario schema, Python and TypeScript types and the data dictionary changed
  (`signal_cycles.parquet`, six new event kinds, four new KPIs). Decision D adds no output
  column; the agent store gains `line_held` and `box_ticket` (run-time state only).
- Per-agent draws gained four blocks (critical gap, heavy factor, late realiser, awareness
  distance). Existing draws keep their blocks: reference scenarios keep their output hashes.
