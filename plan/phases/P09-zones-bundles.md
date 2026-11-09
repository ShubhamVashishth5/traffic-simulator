# P9 · Zones, enforcement and scenario bundles

**Goal:** build area-level compliance from its causes (enforcement, design, land use, population),
and make the population-swap experiment a one-liner. Add the compound scenario bundles (rain, night,
fog, GRAP, EV shift) and the remaining local situations (animals, work zones).

- **Depends on:** P8.
- **Read first:**
  - guide §5, §6.5, §6.10–6.11, §7.4–7.7;
  - [03-model-design.md](../03-model-design.md) §11–13.

## Tasks

### P9.1 Zone model
- **Do:** zones as ribbon sets carrying the G§5.2 parameters, and zone profiles
  (`lutyens_type`, `shahdara_type`) as register presets marked as hypotheses (G§5.3).
- **Done when:** `marg params explain` shows the zone provenance for an agent's location.

### P9.2 Enforcement fields
- **Do:**
  - Police posts and cameras as point sources with distance halos.
  - Temporal decay for operations.
  - The field precomputed onto ribbons.
  - Perceived enforcement wired into the P8 decision engine; the effective penalty.
- **Done when:** a test where violation rates drop near a post, with distance decay that matches the
  halo parameters.

### P9.3 Geometry × population × enforcement composition
- **Do:**
  - Scenario composition from three independently chosen parts (model design §12).
  - `marg.design.PopulationSwap(G, P, E)` builds the full factorial as twin-seeded arms.
- **Done when:** a 2×2×2 experiment builds from two zone profiles in a few lines of Python.

### P9.4 Scenario bundles
- **Do:**
  - `rain(intensity)`, `night`, `fog`, `grap_stage3`, `grap_stage4`, `ev_shift(share)`,
    `erickshaw_cap(...)`, as named override sets of existing parameters only.
  - They can be stacked.
  - A test fails if a bundle introduces a parameter outside the register.
- **Done when:** the rain bundle at three intensities gives capacity and speed losses that can be
  compared with the G§6.10 ranges (a validation check, not a fit).

### P9.5 Animals and work zones
- **Do:** cattle as dwelling obstacles on medians and verges; dog crossing events (higher rate at
  night); work zones as obstacle + lane-drop deadline + optional signal outage, all reusing existing
  objects.
- **Done when:** no new behaviour code is needed beyond the object definitions (checked in review).

### P9.6 Time-of-day runs
- **Do:** day-long runs with 15-minute demand and composition profiles and a heavy-vehicle entry
  window; the day-to-day variation sweep.
- **Done when:** a 12-hour corridor speed profile can be compared with the CSE pattern (validation
  check).

### P9.7 Decomposition report
- **Do:** a report template for factorial decomposition: main effects and interactions of geometry,
  population and enforcement on each KPI, with CIs from replications.
- **Done when:** it renders for the milestone experiment.

## Milestone check
**The population swap** (G§5.3): Shahdara-type agents in a Lutyens-type zone and the reverse, plus
both enforcement levels. The decomposition report states how much of each KPI difference is
geometry, population, enforcement and their interactions. This is the evidence-based answer to
"culture or conditions?".

## Out of scope
Real-data calibration (P10); large networks (P11).
