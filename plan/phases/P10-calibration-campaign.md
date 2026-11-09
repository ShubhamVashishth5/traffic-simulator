# P10 · Calibration and validation campaign

**Goal:** calibrate stages 1–6 against real Indian data, in order, freezing each stage. Screen and
decompose sensitivity, and publish a pattern-oriented validation scorecard on held-out data. This
phase is **bound by data availability**: each stage can start as soon as its data exists, even before
P9 is finished.

- **Depends on:** P3 (tooling) and the mechanism phase for each stage.
- **Read first:**
  - guide §8 (all);
  - [04-data-outputs-analytics.md](../04-data-outputs-analytics.md) §4, §6.

## Tasks

### P10.0 Data inventory and split
- **Do:** list every dataset (own video, the Chennai trajectory set if obtained, counts, signal
  plans) with its site, period and what it measures. Assign each to fitting or **held-out** per stage
  *before* any fitting. Record the split in `calibration/data_split.yaml` (hashes).
- **Done when:** the user reviews and approves the split.

### P10.1 Video and trajectory ingestion (if own data)
- **Do:**
  - Importers from common trajectory formats (CSV per track) and count sheets into the observed-data
    schema used by the distance functions.
  - Coordinate registration to ribbons.
- **Done when:** an observed dataset and a simulated run can be compared with the same code path.

### P10.2 Stages 1–3 (free flow, following, lateral)
- **Do:**
  - Stage 1: early-morning speed distributions.
  - Stage 2: headways by leader–follower pair, and the speed–capacity curve.
  - Stage 3: lateral positions and partial densities.
  - Each stage: recovery test → fit → frozen file → stage report.
- **Done when:** each stage's check from G§8.3 passes on its fit data, and its held-out pattern is
  reported.

### P10.3 Stages 4–6 (gap acceptance, side friction, violations)
- **Do:** the same loop for stages 4, 5 and 6, using junction video, parking and bus-stop sections,
  and red-light and wrong-way counts.
- **Done when:**
  - Each stage passes its check.
  - Side friction reproduces the width–capacity curve *without* it being tuned in.
  - If it doesn't, the fix goes to stage 3 or 5 per the guide, never back to free flow.

### P10.4 Sensitivity analysis
- **Do:** Morris over all Fit and Sweep parameters on the headline outputs (throughput, mean and p95
  delay, sed, conflicts, gridlock share); then Sobol on the 10–20 survivors, with a GP surrogate if
  the run budget requires it.
- **Done when:** the sensitivity report shows S1 and ST with CIs. Parameters with negligible effect
  are fixed, with an ADR.

### P10.5 Held-out validation scorecard
- **Do:** run all patterns from [04 §4](../04-data-outputs-analytics.md) on held-out sites and
  periods.
- **Done when:** the scorecard is rendered and every pattern is marked pass, near or fail, with
  plots. Failures are recorded honestly, with a diagnosis.

## Milestone check
A calibration-and-validation report for the shipped defaults, suitable for publication with them:

- data, stages, recovery tests, frozen values with uncertainty;
- sensitivity indices;
- the held-out scorecard;
- the parameter appendix with grades and roles.
