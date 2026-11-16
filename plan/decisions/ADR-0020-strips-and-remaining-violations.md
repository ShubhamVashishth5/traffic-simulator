# ADR-0020: Footpaths, undivided roads, wrong-way paths, and the last violation types

- **Status:** accepted (the user's decisions of 2026-11-16, listed under "Decision"). The
  parts marked *(mine)* fill details those decisions leave open, for your review
- **Date:** 2026-11-16
- **Phase / task:** P8.4 (`stopline_seepage`, `wrong_way`, `footpath`, `late_merge`,
  `opposite_encroachment`, `stop_anywhere`, per-class intercept shifts)

## Context

P8.4 asks for eleven violation types. ADR-0019 built four at signals; `forced_entry` came with
the engine (ADR-0018). Plan/03 §7 gives each of the other six one line. Three of them need road
features that the network could not describe:

- `footpath`: "queue beside a footpath with no bollards → move onto the footpath strip";
- `opposite_encroachment`: "own queue long, oncoming gap available (undivided road) → use the
  opposite side";
- `wrong_way`: "legal detour is long; a service road or opposite strip is available → route
  through the opposite-direction strip".

ADR-0002 promised "footpaths, service roads and the opposite carriageway are strips with access
flags", and P2.1 listed them, but they were never built: a ribbon has one direction and one
width profile.

The other three need the rule's details settled:

- `stopline_seepage`: where a vehicle that creeps past its line stands;
- `late_merge`: needed ADR-0017's options settled first;
- `stop_anywhere`: ADR-0014 question 3 left "P8 replaces the constant rate by the logistic
  propensity" without a form.

## Part 1. Per-agent road edges (the mechanism shared by parts 2–4)

A vehicle's usable width is normally its ribbon's width profile. A vehicle that has decided on
one of three acts has wider edges, for itself only, until the act is over:

| Act | Edge that moves | To |
| --- | --- | --- |
| footpath riding | kerb (`y_max`) | the footpath's outer edge, where the footpath has no bollards |
| opposite encroachment | median (`y_min`) | the far kerb of the opposite carriageway |
| (wrong-way paths are ribbons of their own: part 4) | | |

Everything that reads the road's edges for a vehicle reads its own: the lateral choice, the
clamp of its position each step, and the collision guard. Nothing else changes: other vehicles
keep the ribbon's edges, and no capacity or speed factor is applied (ADR-0005).

**Deadline** *(mine)*. An act ends where its strip ends: the footpath reaches bollards or its
end, or the ribbon ends (a stop line, a split, the end of the pairing). Within its stopping
distance of that point, a vehicle still outside the ribbon's own width makes for it as at a
split deadline (ADR-0017): it moves back when there is room, and stops before the end if there
is none. It never carries a wider edge onto the next ribbon. The act is over, and its episode
closes, once its body is back within the ribbon's own width and it is moving.

## Part 2. Footpaths (`network.ribbons[].footpath`) and `footpath`

**Network.** `footpath: {width, bollards: [[from_s, to_s], …], from_s, to_s}` on a ribbon: a
strip beside the kerb, `width` m wide, over `[from_s, to_s]` (default: the whole ribbon), with
bollards over the listed stretches. Footpath width and bollards are site data (G§5.2 "footpath
bollards, per link, A, fix"), so they come from the scenario, not the register.

**Who can ride it** *(mine)*. A vehicle no wider than the footpath. This gives G§4.2's
"two-wheelers, sometimes autos" from geometry, with no new parameter (a 1.5 m footpath takes a
1.4 m auto but not a 1.7 m car).

**Trigger** (continuous). Standing in a queue (held by a standing vehicle or a closed line
ahead), beside a stretch of footpath without bollards, with nothing between its body and the
kerb (its side limit on the kerb side reaches the kerb).

**Features.**

- κ(t).
- Waiting time since it stood.
- Imitation counts.
- Enforcement 0 (P9).
- Risk 0: no conflicting stream on a footpath; pedestrians walking along it are not modelled.
- Time saved *(mine)*: the standing queue it would ride past. That is the length of standing
  vehicles ahead of it on the kerb side within the lookahead, divided by its desired speed: a
  proxy in seconds that grows with the queue, as G§4.2's "queue length beside an accessible
  footpath" asks. There is no estimate of the wait to come.

**Act.** The kerb edge moves out to the footpath's outer edge for this vehicle (part 1); its
lateral choice then decides whether to use it. G§6.4's "footpath riding pushes pedestrians onto
the carriageway" waits for walking pedestrians (not modelled; noted for P9).

## Part 3. Undivided roads (`network.ribbons[].opposite`) and `opposite_encroachment`

**Network.** `opposite: [<ribbon id>, …]` (at most two) names the ribbons that carry the other
direction over the same ground. Each pairing must be mutual. A partner's polyline must be this
ribbon's reversed (checked to 1 cm), so a point `(s, y)` here is `(L − s, −y)` there: the
mapping is exact and costs nothing. An undivided road is two such ribbons on one centreline,
each with its own width profile; their median edges meet.

**Seeing each other.** Each vehicle's move phase and lateral choice also walk its partner's
vehicles near `L − s`, mapped into its own frame and moving towards it at their own speed. They
are taken as leaders (closing at the sum of the speeds), as side neighbours when level, and as
conflicts (`head_on`), by the same rules as vehicles on its own ribbon. *(Mine.)* Where the two
carriageways only meet at a line (they do not overlap), a pair is skipped unless one of the two
bodies is across that line. Two vehicles passing on their own sides of the centreline do not
brake for each other, as today. Where the partner's width overlaps this ribbon's (a wrong-way
path, part 4), every pair counts. The collision guard checks partner pairs too. The scan is
entered only on paired ribbons, and only by vehicles within reach of the line or of a partner
vehicle across it: corridors without pairings do not change.

**Trigger** (continuous). On a paired ribbon, standing in a queue, with an oncoming gap: no
oncoming vehicle beside its body in the band it would move into. Features as for `footpath`.
Time saved is the standing queue ahead of it on the median side. **Risk** is the shortfall of
the oncoming gap: `1 − gap / t_c`, clamped to [0, 1]. The gap is the time until the nearest
oncoming vehicle in that band reaches it, at their closing speed; `t_c` is its critical gap,
with the heavy-vehicle factor against a bus or truck.

**Act.** Its median edge moves to the far kerb of the opposite carriageway (part 1). Oncoming
vehicles then see it as a vehicle in their way: they brake for it and steer round it if there
is room. If there is none, both stand. That is the gridlock the guide describes at level
crossings and jams.

## Part 4. Wrong-way paths and `wrong_way`

**Network** *(mine)*. A wrong-way path is an ordinary ribbon that is the opposite (part 3) of
the carriageway it runs against. Its width profile lies over that carriageway, mirrored, so
riders and oncoming traffic share the ground and see each other. It leaves a junction by an arm
marked `wrong_way: {legal: <arm>}`. A vehicle whose drawn turn is that arm decides there,
**once**:

- **wrong way (the violation):** it keeps the arm;
- **legal:** it leaves by the `legal` arm and turns back at the first median opening downstream
  that has a U-turn from its approach (the `reroute` of ADR-0017, logged as `reroute` with cause
  `legal_detour`).

A wrong-way arm whose legal route has no such opening is refused.

**Features.**

- Time saved: the detour saved divided by its desired speed. The detour is the legal route's
  length (to the opening, the U-turn, and on along the receiving carriageway to where the
  network ends) less the wrong-way path's length to where it ends. It is computed when the
  network is built and logged with each decision.
- Risk: the shortfall of the gap to the first oncoming vehicle on the path, as in part 3.
- κ; imitation; enforcement 0.

Time saved and the persona intercepts (G§3.2: delivery riders, e-rickshaws, cyclists) are what
make long medians tempt riders (G§4.2, "detour saved versus the legal route").

## Part 5. `stopline_seepage` (the user's choice: up to the first crossing)

**Trigger** (continuous). Standing at the front of a red line (as for `red_light`), where its
movement has room past the line. The room ends at the start of the first area its movement shares
with a **crossing** movement (`network` conflict areas). The vehicle needs room for its standstill
gap and at least part of its body.

**Act.** A flag `seeps`. The line no longer holds it; it moves on and stands with its front its
standstill gap short of that area while its movement shows red. Standing there it has not
committed to the junction (no entry ticket, as a permitted movement waiting inside, ADR-0016), so
streams with green are not held for it. At green it goes on.

**Features** *(mine)*. κ, waiting time and imitation. Time saved 0: the head start at green is not
estimated. Risk 0: it stands short of every crossing stream. It competes with `red_light` and
`early_start` (the first clock to fire ends the others).

**Records.** `stop_line_crossed` carries `act: "stopline_seepage"`, and a seeper's crossing on red
is not counted among the red-light crossings by third of the red in `signal_cycles`.

## Part 6. `late_merge` (after ADR-0017 A2, B2, C2)

**Trigger** (continuous). Aware of its branch and outside the branch's part of the road, before
its deadline. The branch's stream is queued, with a standing vehicle in the branch's part of the
road ahead within the lookahead, while the side it is on moves (its own leader is not standing).

**Features.** κ; imitation; time saved = the standing queue in the branch's part ahead, divided
by its desired speed (as part 2); risk = its urgency `û` (how hard the move will be by then)
*(mine)*; enforcement 0.

**Act** *(mine)*. It drops the pull towards its branch (ADR-0017 A2) until its deadline. At the
deadline it does not draw stop-and-wait or give-up: it stops and waits to cut in, at full urgency
(clearance scaled by `exp(−γ_u)`). It gives up after `road.split.wait_patience` like any waiting
driver (B2). Waiting there, it holds up the stream it is in: G§6.7's cost.

## Part 7. `stop_anywhere` (the user's choice: thinning)

The per-class rate of ADR-0014 (`road.stop_anywhere.rate.<class>`, per km, times an attractor's
multiplier) becomes the rate of stop **opportunities** (a hail, a set-down). When
`violations.stop_anywhere.intercept` is set, the driver takes each opportunity with probability
`σ(η)`. It is a one-shot decision with its own uniform; κ, imitation, enforcement and the persona
and class intercept shifts enter it, and time saved, waiting time and risk are 0. A declined
opportunity is logged as `complied`, a taken one as `violation` and then `stop_anywhere` as
before. The next opportunity is drawn either way, so twin arms meet the same opportunities.
**With the intercept unset nothing changes**: every opportunity is taken, the output hashes stay
the same, and stop-anywhere is then not a violation decision, so the cooperative twin keeps those
stops. With the intercept set, the twin (`violations.enabled = 0`) takes none.

## Part 8. Per-class intercept shifts (ADR-0018 part 2)

`violations.<type>.intercept_shift.<class>` for every type and class (unset: 0; grade C, fit,
stage 6) is added to the intercept of every decision of that type by a vehicle of that class,
and logged in the intercept term.

## Decision

The user, 2026-11-16:

- **Footpath, opposite encroachment and wrong way: build all three now**: footpaths on ribbons,
  undivided roads as paired opposite-direction ribbons that see each other head-on, and wrong-way
  riding as a path over the opposite carriageway using the same interaction (parts 1–4).
- **Seepage: up to the first crossing** (part 5), no new parameter.
- **Splits: A2 + B2 + C2** (ADR-0017), then `late_merge` on top (part 6).
- **Stop-anywhere: thinning** (part 7).

## Parameters

New register entries:

- `violations.<type>.intercept_shift.<class>`: unset.
- `road.split.base_urgency`: unset, so A1's behaviour.
- `road.split.wait_patience` (s): unset, so B1's behaviour.

`violations.stopline_seepage`, `wrong_way`, `footpath`, `late_merge`,
`opposite_encroachment` and `stop_anywhere` intercepts existed (unset: off). Footpaths, pairings
and wrong-way arms are network data in the scenario.

## Consequences

- Scenario schema: `RibbonSpec.footpath`, `RibbonSpec.opposite`, `ArmSpec.wrong_way`. The Python
  and TS types and the data dictionary are regenerated.
- New micro-scenarios and tests for each type: it triggers only under its trigger; footpath
  riding only beside an accessible footpath, by a vehicle that fits; head-on vehicles on an
  undivided road neither brake for each other on their own sides nor overlap when one
  encroaches.
- With no pairing, no footpath, no wrong-way arm and every new intercept unset (the shipped
  register and scenarios), the corridor and box scenarios do not move. ADR-0016's left-hook rules
  and ADR-0017's options change junction and split runs, which is why those hashes change.
