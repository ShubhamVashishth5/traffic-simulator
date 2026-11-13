# ADR-0017: Splits and mandatory lateral moves

- **Status:** proposed. **The code exists and is untested**: no micro-scenario, no sweep. The
  forms below confirm the ⚑ of plan/03-model-design.md §9 only once you agree to them
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

## Not built
- Give-up does not reroute through the next median opening: the trip ends on the wrong branch.
- The probability does not depend on class or persona (the register has one entry; G§6.7 says
  cars more than two-wheelers).
- The `late_merge` and `forced_entry` hazards (P8).

## Open
All three new register entries, and the four that existed, are unset: the guide has no numbers.
A scenario with a split or a U-turn is refused without them.
