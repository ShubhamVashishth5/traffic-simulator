# ADR-0006: Design factors on distribution-valued parameters

- **Status:** proposed
- **Date:** 2026-11-09
- **Phase / task:** P3.2 (used by P3.7, P3.8 and the P3 milestone)

## Context
Most `fit` parameters in the register are distributions, not numbers: desired speeds are truncated
normals per sub-class (G§1, §7.3), time gaps and standstill gaps are lognormals per class (G§2.1),
accelerations are truncated normals. A design (P3.2) and a calibration (P3.8) need one number per
factor, and the card says the bounds come from the register `range`. The resolver already checks
a distribution's `range` against its **mean** (`ParamEntry::check_value`).

So a factor needs a rule for turning a number `x` into a distribution. The rule decides what a
sensitivity index or a fitted value means: "the population's mean desired speed", or something else.

## Options
1. **Rescale to the mean (scale family).** Multiply every location and scale field by
   `x / mean(base)` (`Dist::scaled`). The mean becomes exactly `x`; the shape and the coefficient
   of variation are kept (truncation bounds scale too; a lognormal keeps its σ). It works for every
   distribution form without per-form code, and the register `range` bounds it directly.
   *Con:* the SD moves with the mean, so "faster drivers" are also "more spread out".
2. **Shift the location.** Add `x − mean` to the location (and the truncation bounds), keeping the
   SD. Natural for normals; undefined for lognormals and empirical forms without per-form rules.
3. **Address a field explicitly** (`path@mean`, `path@sd`, `path@sigma`). Gives full control and
   allows spreads to be factors in their own right. Needs per-form field validation and its own
   bounds (the register `range` does not cover an SD).
4. **Factor = multiplier.** Sample a dimensionless factor around 1. Equivalent to option 1 with
   bounds `[lo/mean, hi/mean]`; reads less directly in reports.

## Decision
Option 1 by default (`BoundFactor.transform == "mean"`). Scalars take the number itself. The design
matrix and every report record the factor as the target mean, in the register unit.
`Factor.relative(path, frac)` gives bounds at `(1 ± frac)` × the base mean, clipped to the range.

Option 3 stays open. If stage-1 calibration (P10.2) needs the SDs of the free-flow speeds fitted
separately from the means, add `path@field` factors then, in a follow-up to this ADR.

## Consequences
- The Morris and Sobol indices of a distribution-valued parameter are sensitivities to its
  **mean**, at a fixed coefficient of variation.
- `fix` parameters are refused as factors, whatever their form.
- An override written by a design is a full distribution in canonical form (e.g. a
  `log_normal_median` becomes `log_normal`), so `params.parquet` shows the exact value used.
