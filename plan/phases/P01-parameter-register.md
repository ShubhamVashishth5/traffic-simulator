# P1 · Parameter register and scenario schema

**Goal:** every number the model will ever use lives in a typed, versioned register, with a
distribution, unit, A/B/C grade, Fix/Fit/Sweep/Check role, calibration stage and source. Every run
records exactly which value each one took.

- **Depends on:** P0.
- **Read first:**
  - guide §9 (master register) and §8.1 (the three-way split);
  - [03-model-design.md](../03-model-design.md) (which parameters exist);
  - [02-architecture.md](../02-architecture.md) §6.

## Tasks

### P1.1 Parameter entry type
- **Do:** in `marg-params`, define `Param<T>`:
  - `value`: scalar or `Dist`;
  - `unit` (SI conversion table);
  - `grade` (A, B, C, A/B, B/C);
  - `role` (Fix, Fit{stage}, Sweep, Check, Scenario);
  - `source` (citation + guide §);
  - `range` (hard bounds), `live` (bool), `notes`.
- **Done when:** serde round-trip tests pass, and unit conversion is tested (km/h → m/s, cm → m).

### P1.2 Distributions
- **Do:** implement `Dist`:
  - Normal, TruncNormal, LogNormal (by mean and SD *or* by median and percentiles);
  - Gamma, ShiftedExponential, Uniform, Empirical (quantile table), Categorical/Multinomial;
  - Mixture, MultivariateNormal (for traits), Constant.
  - Each has `sample(rng)`, `mean()` and `describe()`.
- **Done when:**
  - Sampling moments match the analytic values within tolerance (property tests).
  - The lognormal fitted from Wong & Goh (mean 1.02 s, p15 0.68, p85 1.28) reproduces those
    numbers ±0.02 s.

### P1.3 Transcribe the register
- **Do:**
  - Turn guide §9.1–9.7 into `params/base/{vehicles,interaction,personas,violations,zones,road,demand}.yaml`,
    one entry per row, with paths like `interaction.following.time_gap.two_wheeler`.
  - Where the guide gives a range (e.g. T 0.3–0.49 s), encode a distribution and note the choice in
    `notes`.
  - Put the validation targets from §9.8 in `validation/targets/*.yaml`, not the register.
  - The full guide is a Claude Doc (link in
    [reference/parameter-guide-summary.md](../reference/parameter-guide-summary.md)). Read it with
    the docs tools, or use an exported copy if the user has added one.
- **Done when:**
  - A test counts the entries per group against the §9 table row counts.
  - Every entry has a grade, role and source.

### P1.4 Scenario schema and resolver
- **Do:**
  - Define `Scenario`: geometry ref, objects, demand, zones, bundles, outputs, duration, warm-up,
    and `overrides: {path: value|Dist}`.
  - Write a resolver: base register ← scenario overrides ← experiment values ← live commands, with
    provenance per entry.
  - Reject overrides outside `range`.
  - Reject overrides of `Fix` entries unless `--allow-fix-override` is passed (and record it).
- **Done when:** golden tests over three example scenarios produce the expected resolved spec and
  provenance.

### P1.5 Run identity and `params.parquet`
- **Do:** hash the resolved spec canonically (sorted keys, normalised floats), and write
  `spec.resolved.yaml` and `params.parquet` (schema in
  [04-data-outputs-analytics.md](../04-data-outputs-analytics.md) §2).
- **Done when:** the same inputs give the same hash across runs, machines and key order.

### P1.6 Typed handles and the "no magic numbers" check
- **Do:**
  - Generate a Rust module of typed parameter handles from the register (build script), so model
    code calls `p.interaction.following.time_gap(class)`.
  - Add a CI test that scans `marg-core` model modules for float literals outside an allowlist
    (0, 1, 0.5, 2 and math constants).
  - Add a test that lists register entries never read by any module (warning until P9, error after).
- **Done when:** adding a literal `1.5` to a model function makes CI fail.

### P1.7 CLI: `marg params`
- **Do:**
  - `marg params list [--grade C] [--role Sweep] [--stage 3] [--group personas]` prints a table.
  - `marg params explain <path>` shows the value, distribution, source, grade, role and every
    override layer.
- **Done when:** output matches guide §9 for three spot-checked rows.

## Out of scope
Using any parameter in behaviour (P2 onwards).

## Notes
Python and TS types for the register come from the P0.4 pipeline. The web parameter panel (P4) is
generated from this schema, so the `live`, `range` and `unit` fields matter.
