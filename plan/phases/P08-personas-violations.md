# P8 · Personas, violations, imitation and signal control states (stages 6–7)

**Goal:** the social layer:

- drivers as points in a continuous trait space, grouped into personas;
- one logistic decision rule for every violation;
- imitation working in both directions;
- junction control states that turn a coordination game into Chicken.

Every decision must be explainable term by term.

- **Depends on:** P6, P7.
- **Read first:**
  - guide §3, §4, §6.6;
  - [03-model-design.md](../03-model-design.md) §6–8;
  - [ADR-0007](../decisions/ADR-0007-cooperative-twin.md): every violation hazard must honour
    `violations.enabled` (the cooperative twin, P3.3).

## Tasks

### P8.1 ADR: decision rule timing
- **Do:** confirm the ⚑ one-shot vs hazard formulation (model design §7), the imitation term form,
  and the FOV geometry. Show with a test that the cumulative violation probability is invariant to
  `Δt_dec`.
- **Done when:** the ADR is approved by the user (review gate).

### P8.2 Trait space and personas
- **Do:**
  - Trait vectors drawn from a persona mixture (multivariate normal per persona).
  - The ten starter personas as register entries.
  - Trait → parameter link functions (γ's).
  - The within-trip OU process on κ, plus the jump on delay.
  - Distraction episodes.
  - Persona shares come from the population spec.
- **Done when:**
  - The realised trait moments match the inputs.
  - κ autocorrelation matches the OU parameters.
  - Personas appear in `agents.parquet`.

### P8.3 Decision engine
- **Do:**
  - A `Violation` trait with `trigger`, `features` and `act`.
  - Shared computation of the logit, with every term logged in `events.attrs`.
  - Perceived enforcement (a stub field until P9) and perceived risk.
  - Imitation counts from the FOV query.
- **Done when:** for a scripted agent, the logged terms add up to the logged probability; the viewer
  inspector shows the breakdown.

### P8.4 Violation catalogue
- **Do:** implement each type from model design §7 as a module:
  - `red_light`, `early_start`, `amber_go`, `stopline_seepage`;
  - `wrong_way`, `footpath`, `forced_entry`, `late_merge`;
  - `box_blocking`, `opposite_encroachment`, `stop_anywhere` (from P6).
  - Each one: register entries, micro-tests, and an event type.
- **Done when:** each type triggers only under its trigger condition. Baseline filtering is never
  counted as a violation (G§4.4 regression test).

### P8.5 Red-light timing regimes and countdown effect
- **Do:** scenarios with and without a working countdown timer.
- **Done when:**
  - Fitting `β0` to ~1 in 17 red-light runners produces both timing regimes (after red starts,
    before green) without fitting them.
  - The countdown shifts violations toward the end of red (Sharma et al. 2012 pattern), reported
    with CIs.

### P8.6 Imitation in both directions
- **Do:** the violators-moving term and the compliers-waiting term, plus the "any type"
  cross-norm switch.
- **Done when:** a sweep over `βs⁺` shows tipping (a sharp rise in the early-start share past a
  threshold), and `βs⁻` delays it.

### P8.7 Signal control states
- **Do:**
  - Blinker, dark and police controllers.
  - The outage Markov chain per junction-day (rain coupling hooked up in P9).
  - Blinker schedule.
  - Police: long cycles, a major-approach bias, and compliance with the officer.
  - Live toggle from the viewer.
- **Done when:** switching to dark moves the junction into the uncontrolled regime; events and
  `signal_cycles` record the state.

### P8.8 Persona-share sweeps
- **Do:** experiment templates that sweep the delivery-rider and e-rickshaw shares (G§3.2: grade C,
  report across the range).
- **Done when:** reports show KPIs as functions of both shares.

## Milestone check
**The dark-signal experiment** (G§6.6): identical demand and personas; control state working vs
dark, as twin arms; swept over the aggressive-persona share. Report throughput, delay, conflicts,
gridlock onset and the social efficiency deficit. This is a direct test of the thesis claim about a
negative-sum game played as zero-sum.

## Out of scope
Zone enforcement fields (P9); calibration on real violation counts (P10).
