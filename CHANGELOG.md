# Changelog

All notable changes to marg. Versions follow [semantic versioning](https://semver.org).

**0.x releases are research previews.** They are not calibrated against real data. Outputs,
output schemas and default parameters can change between minor versions, so a later version may
give different results for the same scenario and seed. Each run folder records the version that
wrote it.

The release workflow publishes a version's section below as the body of its GitHub release.
Tagging fails if the section is missing.

## [Unreleased]

Phase P8 completed: the rest of the violation catalogue, the decisions taken on 2026-11-16, and
the persona-share experiment template.

### Added

- **The last six violation types** (ADR-0020), each with micro-scenario tests:
  - `stopline_seepage`: at the front of a red line, creep past it and stand short of the first
    crossing stream;
  - `late_merge`: before a split, stay on the moving side and wait at the deadline to cut in;
  - `footpath`: queued beside a footpath without bollards, ride it, for vehicles that fit on it;
  - `opposite_encroachment`: queued on an undivided road, move onto the opposite carriageway
    when there is an oncoming gap;
  - `wrong_way`: decided once before a junction, ride a wrong-way path instead of the legal
    route and its U-turn;
  - `stop_anywhere` through the rule, by thinning. The class rate becomes a rate of
    opportunities, each taken with probability σ(η). With its intercept unset nothing changes.
- **New road features in the scenario format:**
  - footpaths on ribbons (`footpath: {width, bollards, from_s, to_s}`);
  - undivided roads as paired opposite ribbons (`opposite: [...]`), whose vehicles see each
    other head on when one of them is across the centreline;
  - wrong-way paths (`wrong_way: {legal: <arm>}` on a junction arm).
- Per-class intercept shifts for every type (`violations.<type>.intercept_shift.<class>`, 110
  register entries, unset).
- Six KPIs: `stopline_seepages`, `wrong_way_rides`, `footpath_rides`, `late_merges`,
  `opposite_encroachments`, `stop_anywhere_taken`.
- `marg.templates.persona_shares` and `share_table`: the delivery-rider × e-rickshaw share
  sweep as a reusable experiment, with KPIs as functions of both shares (P8.8).
- Scenarios `undivided_level_crossing` and `wrong_way`.

### Changed

- **Dark junctions in the cooperative twin** (ADR-0019 D1). `marg.bundles.cooperative()` also
  sets `road.signal.dark_first_come = 1`, so the twin takes turns at the lines, first come
  first served, with the junction clear of conflicting traffic.
- **Splits** (ADR-0017 A2, B2, C2):
  - drivers who know their branch can drift over from the moment they know
    (`road.split.base_urgency`, unset);
  - a stop-and-wait ends after `road.split.wait_patience` (unset);
  - a late realiser who learns its branch at the line while it can still stop decides again.
    This last one needs no parameter, so it changes split and median-opening runs.
- **The left hook** (ADR-0016 open problem 3, both rules):
  - a vehicle on the inside of a turning one keeps its sweep `L²/8ρ` clear;
  - the turning one holds at its line while something is level inside its sweep.
- The field of view for imitation is measured from mid-length, so vehicles abreast count
  (ADR-0018).
- `signal_cycles` measures saturation flow over the queued vehicles that did not cross on red.
  A red-light runner or a seeper belongs to no discharge. Without violations nothing changes.

### Results (`reports/phases/P8.md`)

- With the coordinated twin, violations at a dark junction no longer clearly cut delay: the
  SED CIs span zero (it was −73% to −81%). No negative-sum game appears there.
- Opposite-carriageway encroachment at a level crossing is a negative-sum game: throughput
  falls from 86% to 20% and both streams gridlock.
- Seepage raises saturation flow as measured at the line only inside its CI, and imitation
  still does not tip: both are checks, not met.
- At amber onset, a vehicle that can still stop loses any gate it had, so a gap taken far
  upstream for a wait inside the box no longer lets it cross on red (ADR-0016).

Output hashes: the nine scenarios without junctions are unchanged. `median_opening`,
`priority_crossing`, `roundabout`, `signal_crossing` and `split_corridor` change (left-hook
rules and C2). The P8 phase report was regenerated.

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
  - Deterministic: the same output bit for bit at any thread count and on every build (Linux
    glibc and musl, Windows), checked in CI.
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
| `marg-v0.1.0-windows-x86_64.zip` | `marg.exe` for 64-bit Windows, with the register and the scenarios |
| `marg-0.1.0-*.whl` | the Python package for x86_64 Linux (`manylinux`) and 64-bit Windows (`win_amd64`), Python 3.11 or later (`pip install`). It reads the register from `MARG_PARAMS_DIR`: point it at a CLI archive's `params/base` |
| `marg-0.1.0.tar.gz` | the Python source distribution |
| `marg-web-v0.1.0.tar.gz` | the web viewer bundle, for `marg serve` |

Each archive has a SHA-256 checksum next to it. The Python package is not on PyPI.

### Same results on every build

- The same scenario and seed give the same output bit for bit on Linux (glibc and musl) and
  Windows. Before this release, 21 calls in the junction geometry took `sin`, `cos`, `tan`,
  `hypot` and `atan2` from the platform's C maths library, whose results differ in the last bit
  between platforms. They now use the `libm` crate, as ADR-0003 requires, and a test keeps it
  that way. CI runs three scenarios on every build and fails if their output hashes differ.
- That changed the output hashes of four shipped scenarios: `median_opening`,
  `priority_crossing`, `roundabout` and `signal_crossing`. The other 11 are unchanged.
- The phase reports (P2–P8) were made before this change. Runs without a junction reproduce bit
  for bit; runs through a junction reproduce statistically, not bit for bit. A re-run of the P8
  milestone on this version moved its numbers within their confidence intervals and changed
  none of its findings.

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
    more often with personas on. At 700 veh/h with every turn, 12 of 40 seeds stall part way,
    so P7.2's dense-traffic test is no longer run by default.
  - Some dark-junction runs keep a pair of vehicles in the collision guard step after step
    (2 of the 15 observed dark runs in the P8 report; 1 when it is re-run on this version).
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
