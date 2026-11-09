# P7 · Junctions, splits and gap acceptance (stage 4)

**Goal:** signalised and uncontrolled junctions, median openings with U-turns, and splits where
agents must reach a branch by a deadline. Agents comply in this phase; violations come in P8 on top
of these mechanisms.

- **Depends on:** P5.
- **Read first:**
  - guide §2.4, §6.6 (normal state only), §6.7;
  - [03-model-design.md](../03-model-design.md) §4, §8, §9.

## Tasks

### P7.0 Roundabouts (review gate, approved by the user)
- **Do:** decide [ADR-0009](../decisions/ADR-0009-roundabouts.md): roundabouts are in scope (the
  user's answer to open question 11); choose a ring of ribbons (proposed) or a plain junction box.
  The guide gives no roundabout numbers. Either way, task P7.8 is added after P7.7 to build them.
- **Done when:** ADR-0009 is accepted, and this card and P11.1 say how roundabouts are built.

### P7.1 Junction geometry
- **Do:**
  - Movement ribbons inside the junction box (wide, lane-free), and automatic conflict areas where
    movement ribbons overlap.
  - Routes as ribbon sequences; turning shares at entries.
  - Junction YAML format and builder.
- **Done when:** a four-arm junction builds with every movement and its conflict areas, and renders
  in the viewer.

### P7.2 Cross-ribbon interaction
- **Do:** a world-grid neighbour search for agents on different ribbons inside junctions;
  conflict-area occupancy; lateral and longitudinal logic seeing crossing agents.
- **Done when:** crossing streams interact with no guard collisions in dense tests.

### P7.3 Fixed-time signals with countdown
- **Do:**
  - The `Controller` trait and fixed-time plans: phases, amber, all-red.
  - Stop lines.
  - Countdown displays with a working or broken state.
  - The `signal_cycles` table.
- **Done when:**
  - Saturation flow is measured per approach.
  - Discharge acceleration averages land near the 0.4–0.6 m/s² check values (G§1).

### P7.4 Gap acceptance at uncontrolled junctions
- **Do:**
  - Critical gaps by entering × conflicting class (lognormal per agent), with the heavy-stream
    factor and κ scaling.
  - Multiple entries on one gap.
  - Waiting-time tracking (P8 feeds it into the forced-entry hazard).
- **Done when:** accepted and rejected gap distributions from the simulation, estimated with the
  same method as Abhigna et al., recover the input critical gaps (stage-4 synthetic recovery).

### P7.5 Median openings and U-turns
- **Do:** U-turn paths (gamma distribution on 4-lane roads, a mixture on 6-lane roads), U-turn
  critical gaps, and opposing-stream conflict.
- **Done when:** the U-turn path distribution's shape follows the road-width rule in micro-tests.

### P7.6 Splits and mandatory lateral moves
- **Do:**
  - An awareness-distance mixture (with the late-realiser tail) and sign occlusion by large vehicles.
  - Urgency `u` and its effects on lateral utility and required clearance.
  - The stop-and-wait vs give-up decision at the deadline; give-up routes via the next median
    opening.
- **Done when:**
  - Raising the late-realiser share produces more stop-and-wait events, voids downstream of them, and
    lower throughput.
  - The effect is monotone over a sweep.

### P7.7 Upstream-signal-fed entries
- **Do:** let a corridor entry be fed by a simulated upstream signal (platoons) instead of a renewal
  process (G§7.3).
- **Done when:** headways downstream show platoon structure (bimodal) instead of a shifted
  exponential.

## Milestone check
**The split experiment.** Sweep the late-realiser share × the two-wheeler share on a corridor with a
split. Report throughput, the stop-and-wait count and gridlock onset with CIs. This addresses a gap
the guide flags in the Indian literature, and it can be checked against video of a real split once
any is available.

## Out of scope
Red-light running, forced entry as a violation, blinker, dark and police states (all P8).
