# marg: project overview

*A starting point for anyone new to the project: what it is, the research it was built to
support, how it works, where it stands, and where to look next. Written 2026-11-09; the live
progress tracker is [`plan/STATUS.md`](../plan/STATUS.md).*

## In one paragraph

`marg` is an open-source (MIT) microscopic traffic simulator for **lane-free, mixed traffic**: the
traffic of most Indian cities, where two-wheelers, auto-rickshaws, e-rickshaws, cars, buses and
cycles share the road without keeping to lanes. Each vehicle has a continuous lateral position and
its own drawn parameters. Every number the model uses sits in a **parameter register** that records
its source and how far it can be trusted. Runs are bitwise reproducible, and every result is a
Parquet table carrying the parameters that produced it. The goal is a simulator that is realistic,
highly tunable and fast enough to run thousands of experiments on a desktop, **for anyone to use**.
Its first calibrated setting is Delhi.

## Why another traffic simulator

Established simulators (SUMO, PTV Vissim, Aimsun and others, surveyed in
[`plan/01-feature-survey.md`](../plan/01-feature-survey.md)) already offer the application
features researchers expect: a live GUI, a runtime API, virtual detectors, trajectory output and
seeded replications. `marg` copies those. What none of them offers together is:

1. **Native lane-free 2-D behaviour.** Vehicles of different widths filter, stagger and pass
   anywhere across the road width. They are not snapped to lanes or sub-lanes.
2. **A graded parameter register.** Every parameter has a trust grade (A: measured in Indian mixed
   traffic; B: measured elsewhere; C: never measured) and a role (*fix*: measured, never tuned;
   *fit*: calibrated in a given stage; *sweep*: an experimental factor).
3. **Staged calibration that freezes each stage,** so later stages cannot quietly re-tune earlier
   ones.
4. **Built-in sensitivity analysis** (Morris screening, Sobol indices, Gaussian-process surrogates).
5. **Paired "twin" runs.** Two scenario variants run on the same random numbers, so the same
   vehicle has the same draws in both, and differences between arms are not noise.

## The research behind it

The project grew out of independent research on driver behaviour in Delhi. That research is
summarised in the [**Simulator Parameter Guide: Delhi Disordered Traffic**](../plan/reference/parameter-guide.md),
a companion to a literature review titled *Selfish Roads*. The review itself is not in this
repository. The guide lists every parameter a realistic Delhi simulator needs, with its source and
trust grade. It is the source of the model's numbers, cited throughout the code and plan as
"G§n".

The simulator is meant as a general tool, but it was designed so that the following questions can
be answered with it. Each one is a planned experiment in the roadmap.

### Are traffic violations a negative-sum game played as if it were zero-sum?

The guide models red-light running, wrong-way riding, forced entry and similar acts as
**decisions**, not fixed rates. One logistic rule weighs time saved, waiting time, nearby
violators (imitation), perceived enforcement and risk, scaled by each driver's aggressiveness κ
(G§4). The question is whether individually "rational" violations cost everyone more than they
save. Two instruments answer it:

- **The social efficiency deficit:** extra total delay in a run compared with its *cooperative
  twin*, which has the same vehicles and demand but with selfish behaviour switched off.
- **The dark-signal experiment** (planned, P8): a working signal is a coordination game with a
  referee; a dark signal turns the junction into repeated Chicken between approaches (G§6.6).
  Both run as twins, swept across the share of aggressive drivers.

### How much of a place's traffic culture comes from the place itself?

Compliance differs between areas: in Delhi, think of Lutyens' Delhi against Shahdara. The guide
argues against a single "discipline" dial. A zone's compliance should be built from causes that
can be observed: enforcement (with halos around police posts and cameras), road design, footpaths,
kerbside activity, vehicle mix and starting norms (G§5). It also warns against calibrating zones
from fine counts, which measure where police look rather than how often drivers break rules.

- **The population-swap experiment** (planned, P9): put the *same* driver population into a
  Lutyens'-type and a Shahdara-type zone, in a 2×2×2 design of geometry × population ×
  enforcement, and decompose how much of the difference each factor causes.

### How do road conditions and behaviour interact?

"Roads × behaviour" is literally an interaction term (G§8.4). Potholes, parking, bus stops,
vendors, rain and work zones act **only** through what drivers experience: usable width, local
speed limits, the gaps on offer and the incentive to break rules. The model never applies a
"−30% capacity" shortcut (decision record ADR-0005). Published capacity losses are kept as
*checks* the model must reproduce, never as inputs. Examples:

- **The width–capacity check** (planned, P6): kerbside parking in Varanasi costs capacity roughly
  one-to-one with the width it takes. The simulator should reproduce that untuned.
- **The split experiment** (planned, P7): drivers who notice their turn late cause stoppages and
  voids downstream. The guide found no Indian study of this, so it is a gap the simulator can
  explore.

### What to measure

The outputs chosen to carry these arguments (G§8.4):

- throughput at the bottleneck;
- mean and 95th-percentile delay;
- the social efficiency deficit;
- conflicts (time-to-collision below 1.5 s);
- the share of time in gridlock, and **gridlock onset**, the result the guide singles out.

## Design principles

These are enforced in code and review (full list: [`CLAUDE.md`](../CLAUDE.md)).

- **Distributions, not constants.** Every vehicle draws its own desired speed, gaps,
  accelerations and size. Indian data show large spread within each class: hatchback free-flow
  speeds have an SD of about 15 km/h around 61 km/h.
- **No magic numbers.** Every behavioural constant comes from the register, with grade, role,
  stage and source. A test scans the model code for stray literals.
- **Mechanisms, not multipliers** (ADR-0005), as above.
- **Baseline is not violation.** Filtering, staggered following and moving between notional lanes
  are the normal physics of Indian traffic, never counted as violations. Otherwise every scenario
  would show "defection" (G§4.4).
- **Determinism.** All randomness comes from counter-based random streams (Philox) keyed by seed,
  purpose and agent. Results are bitwise identical for any thread count and machine, and a vehicle
  keeps its draws across scenario variants (ADR-0003).
- **Explain decisions.** Every stochastic decision is meant to log its inputs and the terms of its
  rule, so any single event can be traced (in place once decisions arrive in P5 and P8).
- **Calibrate in stages, from the physical to the social,** freezing each stage (G§8.3):
  1. free flow;
  2. following;
  3. lateral behaviour;
  4. gap acceptance;
  5. side friction;
  6. violations;
  7. social factors, which are swept rather than fitted.
- **Validate on patterns, not one number** (G§8.5): speed distributions by class, lateral
  positions, the speed–flow–capacity curve, the width–capacity loss and violation rates by signal
  phase, all at once.
- **Performance is a feature.** Core benchmarks may not regress more than 5% without a recorded
  decision.

## How it works

```text
 scenario YAML ─┐                         ┌─► Parquet tables + manifest (run hash, resolved register)
 register ──────┼─► resolver ─► Rust core ┤
 overrides ─────┘   (layers)   (step loop)└─► live sessions ─► web viewer (live / replay / compare)
                                   ▲
           Python: experiments, designs, SLURM/local backends, sensitivity, calibration, reports
```

- **Space: ribbons, not lanes.** A road is a *ribbon*: a reference line with a width profile.
  Each vehicle has a station `s` along it and a continuous lateral position `y` across it. Junction
  movements will also be ribbons, overlapping in conflict areas (P7).
- **Behaviour.**
  - **Following:** the Intelligent Driver Model, with a lane-free leader set. Every vehicle ahead
    counts, weighted by how much it overlaps laterally, with a speed-dependent lateral clearance
    from Indian measurements.
  - **Lateral movement, filtering and conflicts:** P5.
  - **Personas and violations:** P8.
- **Parameters.** 252 register entries in [`params/base/`](../params/base). By grade: 72 A, 52 B,
  124 C, plus 4 mixed. By role: 87 *fix*, 115 *fit*, 47 *sweep*, 3 *scenario*. A run resolves
  these layers in order: register, frozen calibration stages, scenario overrides, experiment values
  and live changes. The resolved set is saved with the run, and its hash is the run's identity.
- **Outputs.** Per run:
  - `agents` (every trip and every draw), `detectors` (screenlines and Edie areas), `events`,
    `kpis`;
  - optional `trajectories`;
  - `params` (the resolved register) and `manifest.json`.

  Columns and units are in [`docs/data-dictionary.md`](data-dictionary.md).
- **Research layer (Python).**
  - Experiments: designs × arms × replications, with common random numbers.
  - Designs: factorial, Latin hypercube, Morris, Saltelli.
  - Execution: a local pool or SLURM array jobs.
  - Results: one Parquet dataset queried with Polars or DuckDB.
  - Analysis: sensitivity analysis and staged calibration.
  - Reports: rendered with Quarto.
- **Web viewer.**
  - *Live:* watch a run, change parameters while it runs, inspect any vehicle and the forces on it.
    A live session saves as scenario + seed + command log and replays headless bit for bit.
  - *Replay:* open any saved run in the browser through DuckDB-WASM, with a SQL filter.
  - *Compare:* two twin runs side by side.

  Protocol: [`docs/viewer-protocol.md`](viewer-protocol.md).

### Key numbers

| | |
| --- | --- |
| Core speed | About 90 ns per vehicle per time step on one core. The 2 km reference corridor (about 400 vehicles at once) simulates 1 hour in 1.2 s, about 2,900× real time |
| Time step | 0.1 s |
| Vehicle model | 15 types in 10 classes |
| Viewer streaming | 20,000 vehicles at 30 frames/s, 20.7 MB/s on localhost |
| Replay | A 1 h run with 1 Hz trajectories (41.6 MB) opens in about 4–5 s in the browser |

## Status (2026-11-09)

Five of thirteen phases are built. Phase-by-phase evidence is in
[`reports/phases/`](../reports/phases).

| Phase | What | Status |
| --- | --- | --- |
| P0 | Foundations: workspace, CI, schema pipeline, random streams, determinism harness | done |
| P1 | Parameter register (every row of guide §9), scenario schema, resolver, run identity | done |
| P2 | Corridor core: ribbons, 15 vehicle types, lane-free following, demand, detectors, Parquet | done; milestone partial (below) |
| P3 | Experiments, twins, SLURM backend, sensitivity, staged calibration, reports | done; milestone partial (below) |
| P4 | Web viewer: live sessions, replay, twin compare, schema-driven parameter panel | done, except a rendering check that needs a GPU |
| P5 | Lateral behaviour, filtering, seepage, 2-D conflicts | next |
| P6 | Road objects and side friction (parking, bus stops, potholes, humps, rain, pedestrians) | planned |
| P7 | Junctions, signals, gap acceptance, median openings, splits, roundabouts | planned |
| P8 | Personas, violations, imitation, signal control states | planned |
| P9 | Zones, enforcement fields, scenario bundles (rain, night, fog, GRAP, EVs) | planned |
| P10 | Calibration toolkit and validation of the shipped defaults | planned (needs data) |
| P11 | OpenStreetMap import, zones of 20–100 junctions, parallel scaling | planned |
| P12 | Scenario editor, packaging, documentation site | planned |

**What the model cannot do yet.**

- Vehicles keep the lateral position they entered at. Overtaking and filtering arrive in P5.
  Partly for that reason, only 2 of 10 vehicle sub-classes match their measured free-flow speeds
  within ±2 km/h without tuning. Slower vehicles hold up faster ones, which biases measured
  speeds. This is re-checked after P5.
- There are no junctions, road objects, personas or violations yet, so the research experiments
  above cannot run yet.
- Nothing is calibrated against real data: the values are the guide's literature values.
- A first sensitivity screen (Morris, 58 parameters, 1,180 runs) found seven parameters that
  clearly drive corridor capacity or delay, led by the standstill-gap multiplier and car
  acceleration. Many others are within noise at two replications.

## Getting started

Requirements:
- Rust (the toolchain is pinned in `rust-toolchain.toml`);
- for the Python layer, Python ≥ 3.11 with [uv](https://docs.astral.sh/uv/);
- for the viewer, Node 22.

On Windows, use WSL for now; native Windows builds are planned (STATUS P0.9).

```bash
# Run the reference corridor and write a run folder
cargo run --release -p marg-cli -- run scenarios/corridor_2km.yaml --seed 1 --out runs/reference

# Inspect parameters
cargo run -q -p marg-cli -- params list --grade C --role sweep
cargo run -q -p marg-cli -- params explain vehicles.free_flow_speed.hatchback

# Python research layer
uv sync && uv run maturin develop --release && uv run pytest

# Web viewer: http://127.0.0.1:8080
(cd web && npm ci && npm run build)
cargo run --release -p marg-cli -- serve --runs runs
```

A minimal experiment in Python (more in the [README](../README.md#experiments)):

```python
import marg

exp = marg.Experiment(
    "twins",
    "scenarios/corridor_2km.yaml",
    arms={"observed": {}, "cooperative": marg.bundles.cooperative()},
    replications=marg.Replications(n=5),
)
exp.run(marg.backends.Local(workers=6))
kpis = exp.results("kpis")  # one row per run, with arm and replication
```

## Decisions and questions still open

Decisions are recorded as decision records (ADRs) in [`plan/decisions/`](../plan/decisions).

| Item | What is undecided |
| --- | --- |
| ADR-0007 | What the cooperative twin switches off: the κ percentile used, and whether κ is replaced only when squeezing into gaps or everywhere |
| ADR-0008 | How live-viewer changes behave (proposed, implemented; awaiting approval) |
| ADR-0009 | How roundabouts are modelled: a ring of road segments or a junction box (they are in scope; decided when P7 starts) |
| [Open questions](../plan/open-questions.md) | 1: SED and the cooperative twin as configurable metrics. 3: public datasets for checking the defaults. 4: the name. 7: example scenarios. 9: an RL interface. 12: support for other cities through region profiles |

The most useful contribution from an outside reader is **data**: Indian mixed-traffic trajectories,
junction video, or classified counts. Public Indian trajectory data are scarce (G§8.2), and every
calibration stage needs some.

## Where to find things

| Path | What |
| --- | --- |
| [`plan/README.md`](../plan/README.md) | The plan: architecture, model design, data and analytics, roadmap, AI workflow |
| [`plan/reference/parameter-guide.md`](../plan/reference/parameter-guide.md) | The full parameter guide, the source of every number |
| [`plan/03-model-design.md`](../plan/03-model-design.md) | How each section of the guide becomes equations and data structures |
| [`plan/phases/`](../plan/phases) | One task card per phase, each task with "Done when" checks |
| [`plan/STATUS.md`](../plan/STATUS.md) | Task-by-task progress |
| [`reports/phases/`](../reports/phases) | Milestone results per phase (P2 free-flow speeds, P3 sensitivity screen, P4 viewer) |
| [`params/base/`](../params/base) | The parameter register (YAML) |
| [`validation/targets/`](../validation/targets) | Patterns the model must reproduce. These are checks, never inputs |
| [`scenarios/`](../scenarios) | Example scenarios |
| `crates/` | Rust: `marg-params` (register), `marg-net` (geometry), `marg-core` (simulation), `marg-io` (outputs, batch, live sessions), `marg-server` (viewer backend), `marg-cli` (the `marg` command), `marg-py` (Python bindings) |
| `python/marg/` | The Python research API |
| `web/` | The web viewer |

## How the project is built

Development is AI-assisted and task-based. Each phase is split into tasks sized for one working
session. Each task has explicit acceptance checks, and every change runs the same gates:
formatting, lints, tests, the determinism test and the benchmark comparison. The rules sessions
must follow, and the review checklist, are in [`plan/06-ai-workflow.md`](../plan/06-ai-workflow.md)
and [`CLAUDE.md`](../CLAUDE.md). Modelling choices the guide leaves open are not made silently:
they are written up as decision records with options, and decided by the maintainer.

## Licence and sources

The code is MIT-licensed. The model's numbers come from published studies, each cited at its
register entry and in the guide's source list (G§10). Cite the original studies when using a
parameter. Code from GPL-licensed projects is not copied.
