# Release v0.1.0: what is left before the tag

**Status (2026-11-15): steps 1–5 done on branch `ccr-fbbd374f-94a2s3` (from `release-v0.1.0`);
step 6 (PR, CI, tag) waits for the user.** The user chose option B (fix, then Windows, then tag).
A self-contained handoff: a new session should need only this card, the files it names, and
`CLAUDE.md`. Ask the user before opening the PR and before pushing the tag.

## Done (2026-11-15)

- **Step 1.** The 21 calls use `libm` (marg-net needed `libm` added as a dependency; the card
  had said both crates had it). Guard test `crates/marg-core/tests/platform_math.rs` scans
  marg-core, marg-net, marg-params and marg-io, skipping comments, string literals and
  `#[cfg(test)]` items; run against the old tree it lists exactly the 21 calls.
- **Step 2.** All 15 scenarios, seed 1: **identical hashes on glibc, musl and MinGW under
  Wine.** The new hashes (first 16 hex digits; * = changed):

  | Scenario | Hash | Scenario | Hash |
  | --- | --- | --- | --- |
  | `barrier_queue` | `d173e588a2586a2a` | `roundabout` * | `555914d5a78e2e70` |
  | `corridor_2km` | `ef22e8c646ee28d7` | `side_friction_street` | `10a8de17d5e9f3d4` |
  | `corridor_capacity` | `0ef6d0b3ca200c10` | `signal_crossing` * | `7ec700914830e16b` |
  | `free_flow_early_morning` | `f0bc593e10c2d349` | `split_corridor` | `2c281914970b6160` |
  | `kerbside_parking` | `f1874219858b0e95` | `stage3_recovery` | `9cce45971e0b0348` |
  | `median_opening` * | `05cd79291768a835` | `stop_anywhere_metro_exit` | `31dcdcd73021347b` |
  | `moving_bottleneck_e_rickshaw` | `0c002088860be90e` | `upstream_signal` | `854ee2dfdba601b9` |
  | `priority_crossing` * | `a1ece9de8ca6fec8` | | |

  The new `signal_crossing` and `priority_crossing` hashes equal the old musl ones: the `libm`
  crate is a port of musl's maths library.
  - **Speed:** `libm::hypot` costs the same as glibc's (about 4.0 against 4.1 ns per call).
    `corridor_2km` (the same run on both): +0.5% paired median over 5 interleaved rounds.
    `signal_crossing` reads +7% with `marg bench`, but that is seed 1 only, which now locks up
    and takes twice the agent-steps. Over seeds 1–8 the new build is 2.5% faster, with
    seed-to-seed spread of about ±10%.
  - **Tests:** `cargo test` (workspace), clippy, fmt, ruff and pytest (68) pass. The guard
    property test: 0 overlaps, 93 events, all on narrowing roads (unchanged). One test failed:
    `dense_traffic_crosses_without_guard_events` (P7.2, 700 veh/h, seed 1: 474 of 679 through,
    under its 80%). Over seeds 1–40 it holds on 18 with glibc's maths and on 12 with `libm`
    (p = 0.25), so seed 1 had passed by luck. **The user chose to `#[ignore]` it** and record it
    (ADR-0016 open problem 1).
- **Step 3.** ADR-0003, ADR-0016, STATUS and CHANGELOG updated. The P8 milestone was re-run:
  its tables moved within their CIs and no finding changed, but the findings text in
  `scripts/p8_milestone.py` hard-codes about 25 numbers, so the regenerated report would have
  paired new tables with old prose. The committed P8 report was kept, and the CHANGELOG says
  the phase reports predate the change. One P8 number moved and is noted in the CHANGELOG: on
  this version 1 dark run (not 2) holds a pair in the guard for the whole run (40,318
  events), and two others hold one for a while (100 events, and 344 in a cooperative twin).
- **Steps 4–5.** `build.yml`: Windows CLI (MSVC, `.zip`) and wheel, a hash report from every CLI
  build, the `same-results` job (the release job needs it, and drops the hash files from the
  release). README and `docs/project-overview.md` describe Windows; WSL is no longer suggested.
  The optional `windows-latest` `cargo test` job in `ci.yml` was not added.
- **Not done here:** MSVC (the first run is CI on the PR), the Windows wheel (not testable
  here), and the CHANGELOG date, which should be set to the tag date if it is not 2026-11-15.
- **CI round 1 (PR #13).** `ci` green. `build`: the MSVC CLI built and **`same-results` passed**
  (glibc, musl and Windows give the same hashes). The Windows wheel's smoke test failed: both
  experiment runs were refused with "run hash mismatch". Cause: the Python package read and
  wrote text files without an encoding, so Windows used cp1252. The scenario's "§" was hashed as
  "Â§" by Python, while the file it wrote back was UTF-8 again for `marg batch`. Reproduced on
  Linux by forcing cp1252, with the same two hashes. Fixed: every text-mode `read_text`,
  `write_text` and `open` in `python/` and `scripts/` names UTF-8 (98 calls), and
  subprocess output is decoded as UTF-8. `test_encoding.py` runs an experiment with
  `-X warn_default_encoding` (it fails on the old code), and CI's pytest runs with
  `PYTHONWARNDEFAULTENCODING=1` and the warning as an error for marg's code (pyproject).
- **CI round 2.** The experiment now runs on Windows, but `results('kpis')` came back without
  its two rows. `ExperimentData.scan` joined each run's keys to its rows on the file path
  string, and Polars on Windows evidently reports a path in another form than it was given
  (on Linux it is the same string). The join is now on the run's own path
  (`scenario=…/run=…`), read from the end of whatever path Polars reports, with either
  separator (`run_of_file`, tested on Windows forms). The DuckDB view's glob uses `/`. The wheel
  smoke test prints both path forms, so the next run shows what Polars does there. Running
  Windows Python under Wine to check directly was refused by the session's safety check.

## Where things stand

- **The release prep is merged** (PR #12, merge commit `5f21395`): `CHANGELOG.md` with the v0.1.0
  section, and `scripts/release-notes.sh`, which prints that section and fails without one. The
  `build` workflow's release job publishes that section as the release body and marks 0.x tags as
  pre-releases. CI checks that the workspace version has notes. README and
  `docs/project-overview.md` describe 0.1.0.
- **No tag has been pushed.** The user asked for the tag on merge, then put it on hold when the
  problem below was found. Do not tag until this card is done **and** the user says so. The
  release job only runs on a tag, so it has never run: the first tag is its first test.
- **The user's choice is pending.** They asked for this plan for option B: fix determinism and
  add Windows *before* tagging. Option A is to tag now, Linux only, with the limitation stated in
  the notes. Confirm which before starting.

## The problem: results depend on the platform

The same scenario and seed give different output hashes on different builds, including the two
Linux builds the release ships. Measured 2026-11-15 on master's simulation code, seed 1:

| Scenario | glibc (`x86_64-unknown-linux-gnu`) | static musl (`x86_64-unknown-linux-musl`) |
| --- | --- | --- |
| `corridor_2km` | `ef22e8c646ee28d7` | `ef22e8c646ee28d7` (same) |
| `signal_crossing` | `f4b4b65334457de5` | `7ec700914830e16b` |
| `priority_crossing` | `2076c9ea1e0ce6c8` | `a1ece9de8ca6fec8` |

The Windows build (MinGW cross-build, run under Wine) differs too: `signal_crossing`, 4 threads,
`8f617e81…` against glibc's `f4b4b653…`. Near the junction's lock-up limit the runs end far apart:
879 against 1,159 vehicles out. The Windows build itself is fine: it compiles with no errors or
warnings and runs a full multi-threaded scenario.

**Cause.** Standard-library float methods (`f64::sin`, `cos`, `tan`, `hypot`, `atan2`) call the
platform's C math library (glibc, musl or the Windows CRT), and those differ in the last bit.
ADR-0003 already says transcendental functions come from the pure-Rust `libm` crate, which is
bit-identical everywhere. These 21 calls slipped past it. The determinism tests compare only
thread counts on one platform, so nothing caught it. Scenarios without junctions agree because
almost every call builds junction geometry.

**The 21 calls** (non-test code; 18 lines):

| Location | Call | When it runs |
| --- | --- | --- |
| `crates/marg-core/src/arbitration.rs:479` | `hypot` | every step (blocker check) |
| `crates/marg-core/src/junctions.rs:135` | `hypot` | every step (`chord_pose`) |
| `crates/marg-core/src/sim.rs:1837` | `hypot` | every step |
| `crates/marg-net/src/ribbon.rs:195` | `atan2` | at run time (direction) |
| `crates/marg-net/src/ribbon.rs:49`, `:80` | `hypot`, `atan2` | network build |
| `crates/marg-net/src/network.rs:54` | `hypot` | network build |
| `crates/marg-net/src/junction.rs:213` | `tan`, `sin` | network build (`arc_tangent_factor`) |
| `crates/marg-net/src/junction.rs:219`, `:221`, `:222` | `hypot`, `cos` ×2, `sin` ×2 | network build (`hermite_scaled`) |
| `crates/marg-net/src/junction.rs:246`, `:506`, `:782`, `:821` | `hypot` | network build |
| `crates/marg-net/src/junction.rs:738`, `:739` | `cos`, `sin` | network build (ring arcs) |
| `crates/marg-net/src/junction.rs:806` | `atan2` | network build |

Line numbers are as of `5f21395`; re-run the scan in step 1 to refresh them.

## Plan

One PR. About 2–3 hours elapsed, mostly builds and CI; about 45 minutes of editing. Follow
`CLAUDE.md`'s "Builds are expensive": write all of steps 1, 4 and 5 before building, then build
each target once.

### 1. Use `libm` for every transcendental call in simulation code

- Replace each call above with its `libm::` equivalent: `libm::hypot(a, b)`, `libm::atan2(y, x)`,
  `libm::sin`, `libm::cos`, `libm::tan`. `libm` is already a dependency of both crates
  (`grep -n libm crates/marg-*/Cargo.toml`). `sqrt` and `powi` can stay: `sqrt` is correctly
  rounded everywhere, and `powi` comes from Rust's own compiler builtins.
- Add a guard test, like `crates/marg-core/tests/no_magic_numbers.rs`. It scans
  `crates/{marg-core,marg-net,marg-params}/src/**/*.rs`, up to each file's `#[cfg(test)]`, outside
  comments, for `.(sin|cos|tan|hypot|atan2|atan|asin|acos|exp|exp_m1|ln|ln_1p|log10|log2|powf|cbrt|sinh|cosh|tanh|sin_cos|mul_add)(`,
  and fails naming file and line. To list the current calls:

  ```bash
  grep -rnE '\.(hypot|sin|cos|tan|atan2|exp|ln|powf)\(' crates/marg-core/src crates/marg-net/src crates/marg-params/src
  ```

  Ignore hits after `#[cfg(test)]`.

### 2. Verify locally on three builds before pushing

Set up the targets (about a minute; the agent proxy allows apt and rustup):

```bash
rustup target add x86_64-pc-windows-gnu x86_64-unknown-linux-musl
apt-get install -y gcc-mingw-w64-x86-64 musl-tools wine
```

Build all three:

```bash
cargo build --release -p marg-cli                                    # glibc
CC_x86_64_unknown_linux_musl=musl-gcc cargo build --release --target x86_64-unknown-linux-musl -p marg-cli
cargo build --release --target x86_64-pc-windows-gnu -p marg-cli     # ~3.5 min each
```

Run all 15 scenarios with seed 1 on each and require identical `output_hash` (in the run folder's
`manifest.json`, or printed by `marg run`). Windows goes through Wine:

```bash
WINEDEBUG=-all WINEPREFIX=<scratch>/wine MARG_PARAMS_DIR=params/base \
  wine target/x86_64-pc-windows-gnu/release/marg.exe run scenarios/<s>.yaml --seed 1 --out <dir>
```

For the musl binary, also set `MARG_PARAMS_DIR=params/base`. Write outputs under the scratchpad.
**Don't `rm -rf` a path built from a variable**: the safety check blocks it; use fresh folder names.

If a hash still differs, find the first step where the runs part, using trajectories
(`outputs: {trajectories: {every_s: 0.1}}`). Look there for another platform-dependent
operation.

Then:
- Benchmark against the base build, interleaved, at least 5 rounds each (`marg bench
  scenarios/{corridor_2km,signal_crossing}.yaml --repeat 1`; build the base in a separate git
  worktree with its own `CARGO_TARGET_DIR`). Budget: +5% (rule 6). Three calls per step should
  cost well under 1%.
- Run `cargo test --release -p marg-core` and the guard property test
  (`cargo test --release -p marg-core --test guard -- --ignored`; last result 93 events, all on
  narrowing roads).

### 3. Record the change

- The reference hashes change once, deliberately. Old glibc hashes, seed 1, for comparison:

  | Scenario | Hash | Scenario | Hash |
  | --- | --- | --- | --- |
  | `barrier_queue` | `d173e588a2586a2a` | `roundabout` | `06ed33d424b363c3` |
  | `corridor_2km` | `ef22e8c646ee28d7` | `side_friction_street` | `10a8de17d5e9f3d4` |
  | `corridor_capacity` | `0ef6d0b3ca200c10` | `signal_crossing` | `f4b4b65334457de5` |
  | `free_flow_early_morning` | `f0bc593e10c2d349` | `split_corridor` | `2c281914970b6160` |
  | `kerbside_parking` | `f1874219858b0e95` | `stage3_recovery` | `9cce45971e0b0348` |
  | `median_opening` | `32864bfa34a162e9` | `stop_anywhere_metro_exit` | `31dcdcd73021347b` |
  | `moving_bottleneck_e_rickshaw` | `0c002088860be90e` | `upstream_signal` | `854ee2dfdba601b9` |
  | `priority_crossing` | `2076c9ea1e0ce6c8` | | |

  Expect the junction scenarios to change and the corridors to stay the same. Report which
  changed.
- ADR-0003, Consequences: "output is bitwise-identical on every supported build (glibc, musl,
  Windows), checked in CI; every transcendental call in simulation code uses `libm` (guard test)".
- `plan/STATUS.md`: the fix, and P0.9 done.
- The phase reports were made before the change, so they reproduce statistically but not bit for
  bit; say so in `CHANGELOG.md`. Optionally re-run `scripts/p8_milestone.py` (about 6 minutes)
  so the release's newest numbers match the tagged code. If any number in `CHANGELOG.md` moves,
  update it.

### 4. Windows builds and a same-results check in CI

In `.github/workflows/build.yml`:

- **`plan` job:** add a CLI entry
  `{"name":"windows-x86_64","os":"windows-latest","target":"x86_64-pc-windows-msvc"}`, and a
  wheel entry `{"name":"windows-x86_64","os":"windows-latest","target":"x64"}`.
- **`cli` job on Windows:**
  - set `defaults: run: shell: bash` on the job: Windows runners default to PowerShell;
  - the binary is `marg.exe`;
  - skip `strip` and the musl `ldd` check for MSVC;
  - Git Bash has `tar` and `sha256sum`, but `shasum` may be missing, so use `sha256sum` there;
  - a `.zip` is friendlier on Windows than `.tar.gz` (optional).
- **Same results on every build:** after the smoke test, each `cli` job runs a few scenarios with
  seed 1: at least `signal_crossing`, `priority_crossing` and `corridor_2km`, which take seconds
  each natively. It writes `hashes-<name>.txt` (scenario and output hash) and uploads it. A new job
  `same-results` (`needs: cli`) downloads them all and fails unless every build's file is
  identical. This covers glibc, musl, Windows, and macOS when enabled.
- Optional: a `windows-latest` `cargo test` job in `ci.yml`. It is slow; the build job's run
  already exercises the binary.

MSVC can't be run here (only MinGW), so the first CI run on GitHub is the real test; allow one or
two fix rounds of 10–15 minutes each. If the MSVC hashes differ while MinGW's match, suspect the
CRT linking in some other C math call, perhaps through a dependency.

### 5. Docs

- **`CHANGELOG.md` (0.1.0 section):**
  - add the Windows CLI and wheel to the downloads table;
  - remove "There is no native Windows build yet";
  - add "results are identical on every build (checked in CI)" and the note on the changed
    reference hashes;
  - set the section's date to the tag date.
- **README:** add a Windows line to "Install a release" (unzip, set `MARG_PARAMS_DIR`, run
  `marg.exe`). Drop "use WSL" here and in `docs/project-overview.md`.

### 6. Release

1. Open the PR (ask the user first). Get CI and build green, then the user merges.
2. **With the user's go-ahead**, tag the merge commit and push the tag:

   ```bash
   git fetch origin master && git tag v0.1.0 <merge-sha> && git push origin v0.1.0
   ```

3. Watch the tag's `build` run. Its release job checks the tag matches `Cargo.toml` (0.1.0),
   builds the body with `scripts/release-notes.sh`, and publishes a pre-release with every
   artifact.
4. Download one archive and its checksum from the release and run a scenario from it.
