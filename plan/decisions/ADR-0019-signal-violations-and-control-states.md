# ADR-0019: Violations at signals, and signal control states

- **Status:** proposed. Built as written, so that P8.5–P8.8 and the milestone could run; every
  part marked *(mine)* is a choice plan/03 and ADR-0018 do not settle, for your review
- **Date:** 2026-11-15
- **Phase / task:** P8.4 (`red_light`, `early_start`, `amber_go`, `box_blocking`), P8.7

## Context

ADR-0018 settled the decision rule: its timing (one-shot and hazard clock), the imitation
counts, the field of view and the κ links. P8.4 had built one type, `forced_entry`. Plan/03 §7
gives each type's trigger and act in one line, and ADR-0018 part 1 classifies them. What those
lines leave open for the four signal types:

- how a vehicle that decided to run a red crosses its line from a standstill (`gate_open`
  lets a vehicle through only while it moves: the gap of the P8.4 status line);
- where "time saved" comes from without a countdown;
- what separates early start from red-light running when both are crossing a red line;
- which vehicles take the amber decision, and what its risk is.

Plan/03 §8 and G§6.6 give the controller states (fixed, broken timer, blinker, dark, police), an
outage chain with rain as a driver, the blinker schedule, and the officer's long biased cycles.
They do not say how a dark junction decides who goes first among equals, what "bias" means in
numbers, or how compliance with an officer enters.

## Part 1. Red-light running (`red_light`, continuous)

**Situation and trigger** *(mine)*. The agent's movement shows red. The signal holds it: red,
not the keep-clear rule or a vehicle across its path. It is within its comfortable stopping
distance of where it would stand, or standing there: `gap ≤ v²/2b + 2 s0`. Inside that
situation the hazard runs only while the line is its most constraining leader (the front of
the queue: `line cap ≤` every other constraint). Behind another vehicle the episode's clock
pauses rather than ends: a two-wheeler that filters in front does not deal a new threshold.
The episode ends at green (reason `green`) or when the light goes out (`trigger_ended`).

**Act** *(mine)*. A new per-agent flag `runs_red`. It opens the agent's line whatever the light
shows and whatever its speed, until it has crossed. Inside the junction the arbitration decides
who goes first (ADR-0016 part 4), exactly as for a forced entry. That arbitration is first come,
first served by entry ticket, unless the other vehicle can no longer stop. So green cross
traffic that can still stop brakes for a runner, and a runner waits for one that cannot stop.
Plan/03's "cross with gap acceptance against cross traffic" enters through the risk term
below, not as a second gate: a decision that waited for an acceptable gap would make the
violation a gap acceptance.

**Features.**

- κ(t).
- Waiting time: since it stood.
- Imitation counts.
- Enforcement: 1 while an officer controls the junction, else 0 until P9's field *(mine)*.
- Time saved: the red still to come *as the driver can judge it* *(mine)*:
  - exact if a countdown works, or if the driver saw its movement's amber begin (it knows the
    red's length: a regular user of the junction);
  - otherwise, the mean of what is left of a red of the cycle's length given how long it has
    watched it, `(R − w)/2`. This is the posterior mean under an arrival uniform in the red.
- Risk: the shortfall of the gap it would have in the moving conflicting streams if it went
  now, `1 − gap/t_c`, clamped to [0, 1]. Here `t_c` is its critical gap, with the heavy-stream
  factor. The streams are those with green, vehicles on amber that cannot stop, and every
  vehicle already inside the junction, whatever its rank (a runner gives way to everyone).
  This is computed in the move phase, only for front vehicles at a red line, and only when a
  red-light rule runs.

**Consequences for the timing regimes (P8.5 check).** Two terms set where in the red runs
fall:

- At red onset, time saved is the whole red and the cross streams are still in their all-red.
- Near the end of red the cross streams' amber and all-red empty them (risk → 0), while
  waiting time has grown.

Nothing is fitted to the regimes.

## Part 2. Early start (`early_start`, continuous; competes with `red_light`)

**Trigger** *(mine)*: standing at the front of a red line with a **working countdown**, in the
change interval before its green: the remaining red is at most the amber and all-red of the
stage just before the one that gives it green (5 s in the shipped crossing). The window comes
from the plan, not a register entry. The countdown is what lets a driver know that window.
Without one (none, broken, police) there is no early-start episode, and the end-of-red regime
can come only from `red_light`'s risk and waiting time. Time saved is the displayed remaining
red. The act is the same as `red_light`'s. The two clocks compete; when one fires, the other
episode ends (`other_type_acted`).

Options considered for the window:

- **W1 (built):** the change interval from the plan.
- **W2:** a register window in seconds (no source).
- **W3:** the whole red, with a per-type "remaining red" term. W3 makes early start a second
  red-light clock with a different intercept, so the two cannot be told apart.

## Part 3. Amber go (`amber_go`, one-shot)

**Who decides** *(mine)*: at amber onset, each moving vehicle whose movement turns amber and
that is in the *option zone*. It can still stop at the safe braking limit (those that cannot
are let through, ADR-0016) and would reach the line within the amber at its present speed. A
vehicle farther away simply stops; if it ran the red it would be a red-light decision.

**Features.**

- Time saved: the red it would otherwise wait (exact: it knows the cycle).
- Risk: how late into the amber it would cross, `(gap/v)/amber`, in (0, 1].
- Enforcement: as for red light.

The guide's "vehicle type" enters only through κ and the persona. Per-class intercept shifts
(ADR-0018 part 2) are not built yet. The act is `gate_open`, the same pass a vehicle that
cannot stop gets. `P = σ(η)` against one draw per episode.

## Part 4. Box blocking (`box_blocking`, continuous)

**Trigger:** standing at its line on green, or at a junction without lights, held because its
way through is not clear (the arbitration's "blocked" hold, ADR-0016). Waiting time counts from
the episode's start; time saved and risk are 0 *(mine: no estimate)*. **Act:** `box_block`.
The arbitration skips the way-through check for it until it has crossed; rule 1 (never into a
body across the path) still holds, so it stands in the box behind the blocker.

## Part 5. Control states (P8.7)

A signal junction's controller has a run-time state, `fixed | blinker | dark | police`. A broken
countdown is `fixed` with `countdown: broken`. The state is set by four things:

- the scenario (`control.state`, `control.schedule: [{at_s, state}]`);
- the viewer's live toggle (`toggle_signal_state`, logged and replayed);
- the outage chain;
- the night blinker schedule.

What the junction does in each state:

| State | Lights | Who goes first |
| --- | --- | --- |
| fixed | the fixed plan | as P7 (ADR-0016) |
| police | the officer's plan | as a signal; perceived enforcement 1 |
| blinker with `blinker_major` | off | two-way stop ranks for those arms (HCM, as an uncontrolled major road) |
| blinker without, and dark | off | **every movement gives way to every movement it conflicts with** *(mine)* |

Under blinker and dark, forced entry applies.

Plan/03 §8 says "gap acceptance against every conflicting stream, priority ambiguous". Built
literally: at the line each vehicle waits for a gap in every crossing or merging stream,
whatever its rank. A standing vehicle is not "coming", so once all approaches stand, each sees
a gap, and the arbitration inside the box sorts out who goes. That is Chicken: the one that
forces in first goes.

**Outage chain** *(mine)*: per junction, working → dark at a constant hazard
`−ln(1 − p_day)/86 400 s⁻¹`, so that a junction fails on a share `p_day` of days
(`road.signal.outage_probability`). The outage lasts a draw of `road.signal.outage_duration`
(lognormal). If `road.signal.officer.arrival_delay` is set, an officer takes over after that
delay until the outage ends; then the fixed plan resumes where its own clock is. Draws come
from the `Environment` stream, entity = junction, counters `3n` (the gap before outage `n`) and
`3n + 1` (its duration). Rain coupling is P9's.

**Night blinker:** `control.night_blinker: true` and the scenario's new `start_time_h` (hour of
day at t = 0) put the junction on blinker between `road.signal.blinker_start` and `blinker_end`.

**Officer's plan** *(mine)*: the fixed plan's stages in order with their amber and all-red,
over `road.signal.officer.cycle_length`. The green is shared as in the fixed plan, then a share
`b = road.signal.officer.major_bias` of the minor stages' green moves to the stages that serve
the major arms. The major arms are `control.police_major`, or else the arms of the stage with
the longest green. So the major share is `s_fix + b(1 − s_fix)`, and `b = 1` is refused (it
would leave the minor approaches no green). The officer is not adaptive in this version.
**Compliance with the officer** is not a probability of its own. Under police the red-light
rules run with perceived enforcement 1, so `βe` sets it, and the guide's "lower when visibly
overwhelmed" has no term yet. `road.signal.officer.compliance` stays unread.

**What records the state:**

- a `control_state` event `{junction, from, to, cause}`, with cause one of `schedule`, `live`,
  `outage`, `officer`, `repaired`, `blinker_on` or `blinker_off`;
- `signal_state` and `stop_line_crossed` events carry `control`;
- `signal_cycles` gains `control`, `countdown` and the red crossings by third of the red. A
  cycle ends when the lights go out.

## Decision

Built as above: options W1, T2 and I1 (ADR-0018) throughout, the runner flag, mutual yielding
when dark. The *(mine)* parts stand until you change them.

## Parameters

No new register entries. Read for the first time: `road.signal.outage_probability`,
`outage_duration`, `blinker_start`, `blinker_end`, `officer.arrival_delay`,
`officer.cycle_length`, `officer.major_bias` (all unset except the blinker hours), and the
intercepts of `red_light`, `early_start`, `amber_go`, `box_blocking` (unset: off).

## What the measurements say (P8.5–P8.8, milestone; `reports/phases/P8.md`)

- **Part 1:** fitted to 1 in 17, red-light runs fall 36% in the first third of the red and
  40% in the last, against Mumbai's third and two-thirds.
- **Part 2:** with the early-start intercept equal to the red-light one, a working countdown
  barely moves the end of red. An intercept 3 to 6 higher produces Sharma's shift. Calibration
  decides; nothing was tuned.
- **Part 5, the review point that matters most.** Dark-as-mutual-deference plus a twin that
  only switches violations off makes violations *save* delay at a dark junction (−73% to −81%
  SED). The twin then waits for gaps that rarely come. To test the guide's negative-sum claim,
  the twin (or the dark regime) needs a coordinating convention. Options for your decision:
  - **D1:** keep mutual deference, and add a "coordinated" twin (first come, first served at
    the lines).
  - **D2:** dark junctions keep the signal's major approach as informal priority (as the
    blinker with `blinker_major`).
  - **D3:** give way to traffic approaching from the right. Rules of the Road Regulations
    1989, rule 8, for an intersection where traffic is not regulated and neither road is a
    main road. Forced entry is the violation.

## Consequences

- With these types unset and every signal `fixed` (the shipped register and scenarios), the
  reference scenarios keep their output hashes.
- Tests: `crates/marg-core/tests/signals.rs`, 11 tests, including the G§4.4 regression
  (filtering in a dense corridor is never a violation with every type on).
- Protocol 4: `Status.signals` and the viewer's state selector.
- Still to build in P8.4: `stopline_seepage`, which needs where a vehicle stands past a line
  (it would be on a movement ribbon), plus `wrong_way`, `footpath` and
  `opposite_encroachment`, which need an opposite strip and a footpath in the network.
  `late_merge` waits on ADR-0017, and stop-anywhere's terms are also still to build.

## Decision on the dark regime (the user, 2026-11-16): D1

**D1.** A dark junction stays mutual deference with forced entry (the guide's Chicken), and the
cooperative twin coordinates: **first come, first served at the stop lines**, as at an all-way
stop. New register switch `road.signal.dark_first_come` (0/1, `scenario`, default 0), set to 1
by `marg.bundles.cooperative()` with `violations.enabled = 0` (ADR-0007). Under it, at a
junction that is dark or blinking without a major arm:

- a vehicle at the front of its line is stamped with the time it got there (the first step its
  line binds and it is within its desired gap of it). It loses the stamp if it is no longer at
  the front (a two-wheeler squeezes in ahead of it) *(mine: a stamp held by a vehicle that
  something stood in front of locked the junction)*;
- it may enter when no vehicle on a conflicting movement (crossing or merging, not diverging)
  holds an earlier stamp and still waits at its own line (ties by id), and no vehicle on such a
  movement is inside the junction short of the end of the area they share, as at an all-way
  stop *(mine: built first without it, two vehicles entered one after the other and each
  stood in the other's path);*
- the keep-clear rule and the arbitration inside the box (first come, first served by entry
  ticket, ADR-0016 part 4) are as before;
- no gap is read, so forced entry has nothing to trigger on.

No ring can form: the stamps are a total order. The twin then differs from the observed arm in
two ways, no violations and an agreed order, which is what the guide's claim compares. The
milestone experiment was re-run with it (`reports/phases/P8.md`).
