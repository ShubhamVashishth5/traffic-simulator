# P3 · Experiments, analytics and reports v1

**Goal:** turn single runs into research. Experiments with designs, replications and twin arms run
locally or on SLURM, results land in one queryable dataset, and reports render automatically. This is
the backbone the user rated "of utmost importance", so it comes before the behaviour work.

- **Depends on:** P2.
- **Read first:**
  - [04-data-outputs-analytics.md](../04-data-outputs-analytics.md) §1, §5–7;
  - guide §8.

## Tasks

### P3.1 Experiment model
- **Do:**
  - Python `marg.Experiment(name, base, design, arms, replications, outputs)`.
  - Expand it into a JSONL run list with run hashes.
  - Store `experiment.yaml` and the run list under `runs/<experiment_id>/`.
- **Done when:** expansion is deterministic, and identical experiments give identical run lists.

### P3.2 Designs
- **Do:** implement `Grid`, `FullFactorial`, `FractionalFactorial`, `LatinHypercube`, `Morris` and
  `Saltelli`. Use SALib samplers where possible. Parameters are addressed by register path, and the
  bounds come from the register `range` unless overridden.
- **Done when:**
  - Each design's sample count and bounds are tested.
  - Sampling a `Fix` parameter is refused.

### P3.3 Twin arms and common random numbers
- **Do:**
  - Arms are named override sets applied to the same seeds.
  - Ship `marg.bundles.cooperative()`: violation hazards off, κ fixed at the population lower
    quantile for squeezing.
  - Tests that the same agent ID has identical draws in both arms.
- **Done when:** the paired-difference variance is below the independent-difference variance on a
  test experiment (check the variance-reduction ratio).

### P3.4 Replication control
- **Do:** fixed `n`, or sequential: add replications until the CI half-width target on a chosen KPI
  is met (or `max` is reached). Record why it stopped.
- **Done when:** a test shows the sequential rule stops within the expected range on a synthetic
  metric.

### P3.5 Backends
- **Do:**
  - `Local(workers)` process pool calling the CLI.
  - `Slurm(partition, time, cpus, chunk)` writing `sbatch` array scripts, plus `marg collect` to
    validate and index the outputs; the runs are resumable (skip by hash).
  - An Apptainer definition file and a static musl build of `marg`.
- **Done when:** a 200-run experiment completes locally; on SLURM, it survives a cancelled job and
  resubmission with no duplicates. (If no cluster is reachable from CI, use a SLURM docker test
  harness or a dry-run mode.)

### P3.6 Results dataset
- **Do:**
  - Hive-partitioned Parquet with an `index.parquet` of manifests.
  - Python `exp.results(table, filters)` over DuckDB and Polars.
  - Helpers for paired differences, CIs and effect plots.
- **Done when:** results for 10,000 runs load and aggregate in seconds on a laptop (benchmark
  recorded).

### P3.7 Sensitivity analysis
- **Do:**
  - `marg.sensitivity.morris(exp, outputs)` → `μ*`, `σ` with plots.
  - `marg.sensitivity.sobol(exp, outputs)` → S1 and ST with bootstrap CIs.
  - `marg.sensitivity.surrogate(exp, model="gp"|"rf")` → indices on the surrogate, with validation
    R².
- **Done when:** the analytic Ishigami test function reproduces the known indices through the full
  pipeline (a fake simulator backend).

### P3.8 Staged calibration API (freeze rule)
- **Do:**
  - `marg.calibrate.stage(k, ...)` may write only `role=Fit, stage=k` entries.
  - Write `calibration/stage_k.yaml` with values, uncertainty, data hashes and diagnostics.
  - Refuse stage `k+1` unless stage `k` is frozen.
  - Synthetic recovery test utility.
  - Methods: least squares and CMA-ES first; ABC-SMC (pyabc) next.
- **Done when:** a synthetic stage-1 recovery (fit `v0` distributions from simulated detector data)
  recovers the true values within their CIs, and an attempt to write a stage-1 parameter during
  stage 2 raises an error.

### P3.9 Reports v1
- **Do:**
  - Quarto templates: `run`, `experiment`, `sensitivity`, `calibration-stage`, and the parameter
    appendix.
  - `marg report <type> <path>` to HTML and PDF.
  - matplotlib house style; tables export to LaTeX and CSV.
- **Done when:** the reports render for the P2 reference run and for a Morris experiment over the P2
  Fit parameters.

## Milestone check
Morris screening over all P2 `Fit` parameters (desired speeds, T, s0, a, b, pair multipliers), run as
a SLURM array or a large local pool. The sensitivity report renders and shows which following
parameters matter for corridor capacity. That screen decides which parameters stage 2 actually fits.

## Out of scope
New behaviour.
