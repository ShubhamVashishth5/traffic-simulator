# Changelog

All notable changes to marg. Versions follow [semantic versioning](https://semver.org).

**0.x releases are research previews.** They are not calibrated against real data. Outputs,
output schemas and default parameters can change between minor versions, so a later version may
give different results for the same scenario and seed. Each run folder records the version that
wrote it.

The release workflow publishes a version's section below as the body of its GitHub release.
Tagging fails if the section is missing.

## [0.1.0] — 2026-11-15

The first tagged version: phases P0–P8 of the 13-phase roadmap ([`plan/05-roadmap.md`](plan/05-roadmap.md)).
It is a snapshot for trying the simulator and citing the P2–P8 phase reports. It is not a tool
for decisions about real roads.

### What it does

- **Parameter register.**
  - 407 entries transcribed from the *Simulator Parameter Guide: Delhi Disordered Traffic*.
    Every row of its §9 is traced, and each entry carries a grade, role, calibration stage and
    source.
  - 112 entries are unset because the guide gives no number. A mechanism that needs one refuses
    to run until you supply it.
  - Scenarios are resolved in layers (register, then scenario, then experiment), with
    provenance and a run hash.
- **Lane-free core.**
  - Ribbons with width profiles, 15 vehicle types, car following against a lane-free set of
    leaders, and lateral behaviour: clearances, target choice, filtering and seepage.
  - A collision guard, and 2-D time-to-contact conflicts.
  - Deterministic: the same output bit for bit at any thread count.
- **Road situations:**
  - road objects and kerbside parking;
  - stop-anywhere and attractors, bus stops;
  - potholes, humps, gradients and waterlogging;
  - pedestrians crossing midblock.
- **Junctions:**
  - box junctions with priority ranks;
  - fixed-time signals with amber, all-red and countdown displays;
  - gap acceptance, logged as a survey of the gaps accepted and turned down;
  - median openings and U-turns, and entries fed through an upstream signal;
  - splits and roundabouts, experimental (see below).
- **Drivers and violations.**
  - Seven traits per driver, ten personas, and within-trip aggressiveness.
  - One decision rule for every violation, with every term logged.
  - Built so far: forced entry, red-light running, early start, amber go and box blocking.
  - Signal control states: fixed, blinker, dark and police, with an outage process and a night
    blinker schedule.
- **Tools.**
  - The `marg` command line: `run`, `params`, `batch`, `collect`, `report`, `bench`, `serve`
    and `replay`.
  - The Python research API: experiments, designs, twin arms on common random numbers, local
    and SLURM backends, sensitivity analysis, staged calibration and reports.
  - The web viewer: live sessions, replay and twin comparison.
  - Parquet outputs, described in [`docs/data-dictionary.md`](docs/data-dictionary.md) (output
    schema 4, viewer protocol 4).
- **Phase reports** for P2–P8 in [`reports/phases/`](reports/phases), each with its milestone
  check.

### Downloads

| File | What |
| --- | --- |
| `marg-v0.1.0-linux-x86_64-musl.tar.gz` | `marg` for any x86_64 Linux (static), with the parameter register and the shipped scenarios |
| `marg-v0.1.0-linux-x86_64-gnu.tar.gz` | the same for glibc Linux, about 7% faster |
| `marg-0.1.0-*.whl` | the Python package for x86_64 Linux, Python 3.11 or later (`pip install`). It reads the register from `MARG_PARAMS_DIR`: point it at a CLI archive's `params/base` |
| `marg-0.1.0.tar.gz` | the Python source distribution |
| `marg-web-v0.1.0.tar.gz` | the web viewer bundle, for `marg serve` |

Each archive has a SHA-256 checksum next to it. There is no native Windows build yet: on Windows,
use WSL. The Python package is not on PyPI.

### Known gaps

Each of these is recorded in [`plan/STATUS.md`](plan/STATUS.md), with the decision it waits on.

- **Not calibrated.** The values are the guide's literature values. Several checks are missed
  and reported, never tuned:
  - capacity loss against effective-width loss is 1.34, against the target of 1.0 (P6);
  - in the last free-flow check, 2 of 10 sub-classes were within ±2 km/h (P2), and it has not
    been re-run since lateral movement arrived;
  - the countdown and imitation patterns of the violation literature do not appear (P8).
- **Junctions are fragile at higher flows.**
  - A signalised crossing can lock up at 400–700 veh/h per arm when right turns are present,
    more often with personas on.
  - Some dark-junction runs keep a pair of vehicles in the collision guard step after step
    (2 of 15 runs in the P8 experiment).
  - Splits and roundabouts wait on modelling decisions: roundabouts lock above light flows, and
    the split experiment does not show the late-realiser effect.
- **Violations are off by default.** Their coefficients are unset in the register: set them to
  use the rules.
  - Not built yet: stop-line seepage, wrong-way riding, footpath riding, opposite-side
    encroachment, late merging, and stop-anywhere's terms.
  - The dark-junction regime and its cooperative twin are under review (ADR-0019). The
    dark-signal experiment does not show the guide's "negative-sum game".
- **Speed.** The reference corridor runs at about 900 ns per agent-step on the shared
  development VM, over the plan's budget of 555 (P5.9). It has not been measured on a desktop.
- **Not in this version:**
  - zones and enforcement fields, and the rain, night, fog and policy bundles (P9);
  - calibration on real data (P10);
  - OpenStreetMap import and city-scale networks (P11);
  - the scenario editor and a documentation site (P12).
- **Viewer frame rate.** The viewer's 60 fps rendering check has not been run (it needs a GPU).

[0.1.0]: https://github.com/ShubhamVashishth5/traffic-simulator/releases/tag/v0.1.0
