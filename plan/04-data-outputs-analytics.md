# 04 · Research data, analytics and reporting

**Every run writes a self-describing folder of Parquet tables.** The folder holds the resolved
parameter register, so a run can always answer "which value did each parameter take, and how much do
we trust it?". Experiments are Hive-partitioned datasets of such runs that Polars or DuckDB query
directly. Reports are generated from those datasets, never assembled by hand.

## 1. Run folder

```
runs/<experiment_id>/scenario=<scenario_id>/arm=<arm>/rep=<k>/<run_hash>/
  manifest.json          run identity and provenance
  spec.resolved.yaml     full resolved run spec (register + scenario + overrides)
  params.parquet         resolved register, one row per parameter
  agents.parquet         one row per agent (static draws + trip summary)
  kpis.parquet           one row per run (headline metrics)
  detectors.parquet      one row per detector × interval × class
  signal_cycles.parquet  one row per junction × cycle × approach
  events.parquet         one row per discrete event
  conflicts.parquet      one row per conflict
  trajectories.parquet   optional; rate and spatial windows set in the scenario
  commands.parquet       live sessions only: timed command log
```

**`manifest.json`** records:

- run hash, experiment, scenario, arm and replication;
- master seed and RNG scheme version;
- `marg` version, git SHA and dirty flag; build profile;
- host, thread count, wall time and peak memory;
- output schema versions and status (`ok`, `failed`, `timeout`).

**Identity.** The run hash is `hash(spec.resolved + seed + marg version)`. Batch runners skip any run
whose hash already has an `ok` manifest. Re-submitting a half-finished sweep is therefore safe and
cheap.

## 2. Tables

Schemas are versioned in `marg-io`. A generated **data dictionary** (`docs/data-dictionary.md`) lists
every column with its type, unit and meaning.

| Table | Key columns | Main content |
| --- | --- | --- |
| `params` | `run_hash`, `path` | `value` (scalar or JSON distribution), `unit`, `grade`, `role`, `stage`, `source`, `overridden_by` (base, scenario, experiment, live) |
| `agents` | `run_hash`, `agent_id` | kind, class, sub-class, persona, traits (κ₀, π, ρ, c, η, skill, δ), drawn parameters (`v0, T, s0, a, b`, reaction time, critical gaps), origin, destination, entry and exit time, distance, travel time, **delay vs free-flow**, stops, violations by type, conflicts involved, `gave_up_split`, `stop_and_wait` |
| `kpis` | `run_hash` | throughput at named screenlines, mean and p95 delay, conflicts per veh-km, gridlock share, violations per 1,000 veh by type, warm-up and measured windows; `sed` when a twin cooperative arm exists |
| `detectors` | `run_hash`, `detector_id`, `t_start`, `class` | count, PCU-equivalent, time- and space-mean speed, density, occupancy, lateral position mean and SD, a lateral histogram (fixed bins), speed histogram |
| `signal_cycles` | `run_hash`, `junction_id`, `cycle`, `approach` | control state, timer state, arrivals, discharges, saturation flow, max queue, violations by phase-time bin (first third of red, middle, last third) |
| `events` | `run_hash`, `t`, `event_id` | `type` (violation start or end with type, signal_state, object_response, forced_entry, stop_and_wait, gave_up_split, parking_manoeuvre, ped_crossing, collision_guard, breakpoint), `agent_id`, ribbon, s, y, a typed `attrs` struct per type, and for decisions the **logit term breakdown** |
| `conflicts` | `run_hash`, `conflict_id` | agents A and B with classes, type (rear-end, lateral, crossing), min TTC, PET, max DRAC, location, time of min TTC |
| `trajectories` | `run_hash`, `t`, `agent_id` | ribbon, s, y, world x and y, heading, v, a, lateral speed, state flags (violating, waiting, distracted…) |

**Decision explanations.** Every violation decision event stores its feature values and each term of
the logit. "Why did agent 4,812 run the red?" is then a query rather than a guess. The viewer's
inspector shows the same breakdown.

**Size control.** A 2 km corridor hour at 1 Hz trajectories with ~3,000 agents at once is about 10⁷
rows — roughly 100–200 MB as zstd Parquet. Sweeps run with trajectories off. Every KPI and every
validation pattern comes from online observers, so nothing is lost.

## 3. Metric definitions

These are implemented once, in Rust observers. Python recomputes them from trajectories in tests, to
cross-check the two implementations.

- **Throughput:** vehicles and PCU per hour across a named screenline (the bottleneck), over the
  measured window only (after warm-up).
- **Delay:** travel time minus free-flow travel time, where free-flow time uses the agent's own `v0`
  and local caps. Reported as mean, p95 and by class or persona.
- **Conflicts:**
  - 2-D TTC uses constant velocity vectors between oriented rectangles. It is computed with the
    separating-axis test over a short horizon, using the cached neighbour sets.
  - A conflict is any episode with TTC below the register threshold (default 1.5 s, G§8.4).
  - PET is measured in junction conflict areas, and DRAC is also recorded.
- **Gridlock share:** the share of measured time in which any monitored area has space-mean speed
  below `v_grid` (register, e.g. 5 km/h) for at least `τ_grid` with spillback into an upstream
  junction box. The first-onset time is also recorded; it is the key output for the thesis (G§8.4).
- **Social efficiency deficit (`sed`)** ⚑: the relative excess of total delay in an arm over its
  *cooperative twin*. The twin has the same seed, demand and agents, with violation hazards and
  κ-driven squeezing switched to the cooperative setting. The exact definition must match the
  literature review — see [open questions](open-questions.md).
- **Effective width:** a time-averaged free lateral span at sampled `s`, the observer for the
  width–capacity check (G§6.2).
- **Dynamic PCU:** estimated from detector speeds and areas with the Chandra method. Checked against
  G§1 ranges, never set.

## 4. Pattern-oriented validation (G§8.5, §9.8)

Targets live in `validation/targets/*.yaml`, each with a tolerance band and a source.
`marg validate <scenario-set>` runs the reference scenarios and writes a scorecard.

| Pattern | Observer → statistic | Target (source) |
| --- | --- | --- |
| Free-flow speed by sub-class | `agents` + detectors at low density → mean and SD per sub-class | e.g. motorbike 49.2 ± 12.4, scooter 43.6 ± 10.4, moped 36.0 ± 8.0, hatchback 60.6 ± 15.5 km/h (Balakrishnan & Sivanandan 2015) |
| Lateral position by class | detector lateral histograms | class-specific distributions from video (A, own data) |
| Speed–flow–capacity | detectors across demand levels → capacity vs operating speed | 1,482–2,105 PCU/h/lane on a quadratic in speed (Dhamaniya & Chandra 2017) |
| Width loss vs capacity loss | parking scenarios at several kerb occupancies | ≈ 1:1 (7% → 6.4%, 36% → 35.1%; Varanasi) |
| Violations by signal phase, with and without timer | `signal_cycles` phase bins | end-of-red violation cycles 12% → 75% with timer; start-of-red 59% → 31% (Sharma 2012) |
| Pedestrian threshold | midblock capacity vs pedestrian flow | no effect below about 200 ped/h (Dhamaniya & Chandra 2014) |
| Corridor speed over the day | 15-min space-mean speed | arterials 26–28 km/h most of the day; Lutyens' 44 / 52 (CSE) |
| Partial densities (sanity) | detectors | 2W 170/450, car 55/225, bus 10/35, auto 15/80 veh/km (Kanagaraj & Treiber 2018) |
| Overall fit | macroscopic characteristics | within ~20% (Kanagaraj & Treiber 2018) |

The scorecard marks each pattern **pass**, **near** or **fail**. It also records which patterns were
used for fitting at which stage, so held-out patterns are visibly held out.

## 5. Experiments

```python
import marg
base = marg.Scenario.load("scenarios/corridor_parking.yaml")

exp = marg.Experiment(
    name="parking-width-sweep",
    base=base,
    design=marg.design.Grid({"objects.kerb_occupancy": [0, .1, .2, .3, .4]}),
    replications=marg.Replications(min=5, max=30, ci_halfwidth=0.03, metric="throughput"),
    arms={"observed": {}, "cooperative": marg.bundles.cooperative()},   # twin arms, same seeds
    outputs=marg.Outputs(trajectories=None),
)
exp.run(backend=marg.backends.Local(workers=8))         # or marg.backends.Slurm(...)
df = exp.results("kpis")                                  # Polars over the Parquet dataset
```

- **Designs:** grid and full factorial, fractional factorial, Latin hypercube, Morris, Saltelli
  (Sobol), and the population-swap template (`marg.design.PopulationSwap(G=[...], P=[...], E=[...])`).
- **Replications:** fixed or sequential. Sequential runs add replications until the 95% CI half-width
  of the chosen metric falls below the target (Aimsun and Vissim practice, automated).
- **Common random numbers:** arms share seeds and RNG streams, so the same agent gets the same draws
  in every arm. Paired differences then have much lower variance than independent runs.
- **Backends:**
  - local process pool;
  - SLURM: a generated `sbatch` array, each task running a chunk of the JSONL run list through the
    static CLI, with a resumable `marg collect` at the end;
  - any other machine that can run the binary.

## 6. Calibration and sensitivity

- **Staged calibration (G§8.3).** `marg.calibrate.stage(k, data, targets, method)` may write only
  parameters with `role=Fit, stage=k`. Each stage writes `calibration/stage_k.yaml` with the frozen
  values, posterior or uncertainty, data hashes, fit diagnostics and a stage report. Starting stage
  `k+1` requires a frozen stage `k`.
- **Methods.**
  - Trajectory stages 1–3: least squares or CMA-ES on trajectory and distribution distances, and ABC
    on the spreads.
  - Aggregate stages 4–6: ABC-SMC (pyabc), or a likelihood-based Bayesian approach (Platt 2020) with
    kernel density likelihoods.
  - Optional: neural posterior estimation (`sbi`).
  - Stage 7 is swept, not fitted.
- **Identifiability check.** Before fitting real data, each stage runs a **synthetic recovery test**:
  simulate with known parameters, fit, and check recovery (Carrella 2021 warns that about half of
  agent-based models have an unidentifiable parameter). Anything that cannot be recovered gets
  demoted to Sweep or fixed, with an ADR.
- **Sensitivity (G§8.4).**
  - Morris screening over all Fit and Sweep parameters → `μ*` and `σ`.
  - Sobol (Saltelli) on the survivors → S1 and ST with bootstrap CIs.
  - GP or random-forest surrogate when runs are expensive (Ciuffo et al. 2013).
  - Outputs: throughput, mean and p95 delay, sed, conflicts and gridlock share. The key quantity is
    the gap between ST and S1, because it measures interaction.

## 7. Reports

Quarto templates in `reports/` are rendered by `marg report <type> <experiment>` to HTML and PDF.
Figures use one house style; tables export as LaTeX and CSV for papers.

| Report | Contents |
| --- | --- |
| Run | Scenario summary, map, KPIs, detector time series, violation and conflict breakdown, an excerpt of the parameter register |
| Experiment | Design, arms, replications and CI convergence; KPI tables with 95% CIs; paired differences (twin arms); factor main effects and interactions plots |
| Sensitivity | Morris `μ*`–`σ` plot; Sobol S1 and ST bars with CIs; surrogate fit quality |
| Validation scorecard | The patterns in §4 with plots against target bands; fit-versus-held-out marking |
| Calibration stage | Data used, recovery test, fit diagnostics, posterior plots, frozen values |
| **Parameter appendix** | The full register for the experiment: value, distribution, source, grade, role and the stage that fixed it. This is the answer to "how do you know these numbers are right?" (G§8.1) |

## 8. Live analytics (web viewer)

The viewer reads the same observers as the headless runs:

- a KPI strip at 1 Hz;
- a time–space diagram for the selected ribbon;
- lateral-position and speed histograms per class;
- a fundamental-diagram scatter from detectors;
- violation counts by type, and a conflict heatmap.

Replays compute the same views from Parquet in the browser with DuckDB-WASM. A chart seen live can
always be regenerated from the saved run.
