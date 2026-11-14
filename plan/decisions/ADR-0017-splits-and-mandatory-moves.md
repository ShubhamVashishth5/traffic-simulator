# ADR-0017: Splits and mandatory lateral moves

- **Status:** proposed. **Run for the first time on 2026-11-14 (P7.6 and the P7 milestone):
  the forms below do not produce what P7.6 asks for. Three of them need your decision**
  (options at the end). The ⚑ of plan/03-model-design.md §9 stays open until then
- **Date:** 2026-11-13
- **Phase / task:** P7.6 (also used by P7.5 U-turns and by turns at boxes)

## Context
G§6.7 gives the urgency `u = Δy / max(x_d − x, ε)` and says what rises with it, without forms or
numbers. §9 of the model design marks the section ⚑.

## Proposed (as coded)
- **Window.** A movement can have a part of the arriving ribbon's width it is made from. At a
  split it is the branch's own width (it binds: a vehicle outside it at the line takes the branch
  it is in, logged as `missed_branch`). A U-turn at a box is made from one notional lane at the
  median side (binds). A left or right turn at a box is made from one notional lane at its side
  where the approach is at least two wide (does not bind: only the pull below).
- **Awareness.** A share `split.late_realiser_share` of drivers are late realisers. Each driver
  draws the distance at which it knows its branch from `split.awareness_distance` or
  `split.late_awareness_distance`. A late realiser also learns it from the sign, within
  `sign_visibility_distance`, unless the vehicle it follows is a bus, truck or tractor.
- **Urgency.** `û = min(u / tan θ_max, 1)`, with ε the driver's standstill gap: 1 means the move
  can no longer be made even at the heading limit. No new parameter for the scale.
- **Pull.** The lateral choice adds `w_route · û` (m/s²) per metre a candidate brings the vehicle
  nearer its window (`split.route_weight`), over candidates on the way there only.
- **Squeezing.** The clearance kept to side neighbours when choosing is scaled by
  `exp(−γ_u û)` (`split.urgency_clearance_link`).
- **Deadline.** Where the vehicle could still just stop before the line and is outside a binding
  window: with probability `split.stop_and_wait_probability` it stops before the line and creeps
  over when there is room; otherwise it gives up and takes the branch it is in. One draw per
  vehicle and split, logged as `split_decision` with its features.

## Added while running it (2026-11-14) *(mine, please review)*
- **A driver that never learnt its branch takes the one it is in at its deadline**, there and
  then (`missed_branch`, aware = false). Before, it reached the line on the wrong side, the
  collision guard held it back because its route ran onto the other branch's carriageway, and
  the switch to the branch it was in (made at the line) never came: 50,959 guard events and a
  locked split in the first run. A driver that gave up switches at once too; one that decided
  and drifted out of its branch's part of the road afterwards switches as it reaches the line.
- **Give-up goes back through the next median opening.** A driver that took a branch it did
  not want, and whose next junction on that branch is a box with a U-turn from its arm, makes
  that U-turn there (`reroute` event, `reroutes` KPI). Tested on a split with an opening on one
  branch. Further on, its trip is drawn as any other's.

## What running it showed (P7 milestone, `reports/phases/P7.md`)
`scenarios/split_corridor.yaml` (10.5 m splitting into two 5.25 m branches, placeholder inputs),
five seeds per point:
- At 1,000 veh/h **with no late realisers**, about 13% of drivers reach their deadline on the
  wrong side and stop, as many give up, and the split gridlocks in most runs. In single runs, 44%
  of drivers were on the wrong side at their deadline with `w_route` = 1 and 28% with the
  register's maximum of 20.
- Raising the late-realiser share from 0 to 0.4 changes throughput, stop-and-wait and voids by
  less than the noise at every two-wheeler share; at 1,200 veh/h almost every run gridlocks.
- Three causes:
  1. **The pull comes too late.** It acts only within the lookahead (150 m), and û is near zero
     until the move is nearly impossible, so a driver who has known its branch for a kilometre
     starts moving where the road is densest.
  2. **A stop-and-wait never ends.** Two waiting drivers who need each other's side block each
     other for good: a swap. The road behind locks; the gridlocks start there.
  3. **A late realiser who has not learnt its branch by its deadline does not stop**, so more
     late realisers make more missed branches rather than more stoppages, which is not what
     G§6.7 describes ("stops and waits in place, which blocks the stream").

## Options (for your decision)
**A. When a driver who knows its branch moves over.**
- A1. As now: pull `w_route · û`, within the lookahead.
- A2. *(recommended)* A pull from the moment it knows, `w_route · max(û, û₀)`, with a new
  register entry `road.split.base_urgency` (û₀, grade C, sweep, unset): drivers who know drift
  over at leisure, and the late tail is what is late. Needs the route view beyond the lookahead
  (the split ahead, not the leaders, is what matters there).
- A3. Urgency against the distance at which it learnt its branch rather than against the
  heading limit: `û = Δy / Δy₀ × d₀ / max(d, ε)`. No new parameter, but a stronger assumption.

**B. How a stop-and-wait ends.**
- B1. As now: when there is room (can lock for good).
- B2. *(recommended)* Patience: after `road.split.wait_patience` (new, grade C, sweep, unset)
  it gives up and takes the branch it is in. Ends every swap; the waiting still blocks the stream
  for that long, which is the effect G§6.7 wants.
- B3. Of two waiting drivers that block each other, the one nearer its window goes first and
  the other gives way (no parameter; resolves swaps only).

**C. A late realiser who learns its branch at or past its deadline.**
- C1. As now: it takes the branch it is in.
- C2. *(recommended)* It is a driver on the wrong side at its deadline like any other: it stops
  to wait with the same probability if it learns its branch before the line (the sign, or its
  late awareness, inside its stopping distance), braking at up to the safe limit.

## Not built
- The probability does not depend on class or persona (the register has one entry; G§6.7 says
  cars more than two-wheelers).
- The `late_merge` and `forced_entry` hazards (P8).

## Open
All three new register entries, and the four that existed, are unset: the guide has no numbers.
A scenario with a split or a U-turn is refused without them.
