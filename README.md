# marg — lane-free mixed-traffic simulator

Research simulator for disordered (lane-free) mixed traffic on Indian urban roads, built to
implement the *Simulator Parameter Guide: Delhi Disordered Traffic*.

**New here?** Start with the [project overview](docs/project-overview.md): what the simulator is,
the research it supports, how it works and where it stands.

**Status:** a research preview, version 0.1.0. Phases P0–P8 of 13 are built: the corridor core,
lateral behaviour, road objects, junctions and signals, personas and violations. **Nothing is
calibrated against real data yet**: the values are the guide's literature values, and some
checks are missed. [`CHANGELOG.md`](CHANGELOG.md) lists what works and the known gaps,
[`plan/`](plan/README.md) has the design and roadmap, and [`plan/STATUS.md`](plan/STATUS.md)
tracks progress task by task.

## Layout

| Path | What |
| --- | --- |
| `crates/` | Rust workspace: `marg-params` (register, schema), `marg-net` (geometry), `marg-core` (simulation), `marg-io` (outputs), `marg-cli` (`marg` binary), `marg-server` (live viewer backend), `marg-py` (Python bindings) |
| `python/marg/` | Python research API |
| `web/` | Web viewer |
| `params/`, `scenarios/`, `validation/` | Parameter register, scenarios, validation targets |
| `plan/` | Design and roadmap |

## Install a release

[Releases](https://github.com/ShubhamVashishth5/traffic-simulator/releases) carry ready-built
files for x86_64 Linux and Windows, so you need no Rust toolchain. Every build gives the same
results bit for bit for the same scenario and seed.

```bash
# The command line, with the parameter register and the shipped scenarios
tar xzf marg-v0.1.0-linux-x86_64-musl.tar.gz && cd marg-v0.1.0-linux-x86_64-musl
export MARG_PARAMS_DIR=$PWD/params/base
./marg run scenarios/corridor_2km.yaml --seed 1 --out runs/reference

# The Python package (Python 3.11 or later). It does not include the register: it reads the
# one MARG_PARAMS_DIR points at (above: the CLI archive's), or the nearest params/base above
# the working directory.
pip install marg-0.1.0-*.whl
```

On Windows, in PowerShell:

```powershell
Expand-Archive marg-v0.1.0-windows-x86_64.zip -DestinationPath .
cd marg-v0.1.0-windows-x86_64
$env:MARG_PARAMS_DIR = "$PWD\params\base"
.\marg.exe run scenarios\corridor_2km.yaml --seed 1 --out runs\reference

# The Python package: the wheel whose name ends in win_amd64
pip install marg-0.1.0-cp311-abi3-win_amd64.whl
```

## Build from source

```bash
cargo test
```

```bash
uv sync && uv run maturin develop --release && uv run pytest
```

```bash
cargo run --release -p marg-cli -- run scenarios/corridor_2km.yaml --seed 1 --out runs/reference
```

## Web viewer

```bash
(cd web && npm ci && npm run build)
cargo run --release -p marg-cli -- serve --runs runs     # http://127.0.0.1:8080
```

- **Live:** start a scenario; play, step, time warp, breakpoints; click a vehicle to inspect it;
  change `live` parameters while it runs; save the session. A saved session is scenario + seed +
  command log, and `marg replay runs/live/<id>` reproduces it bitwise.
- **Replay:** open any run folder under `--runs` (or a URL, or a local folder); scrub, filter with
  SQL (DuckDB-WASM), with the same dashboards from the stored tables.
- **Compare twins:** two arms of an experiment on one clock, side by side or overlaid.

For development, `npm run dev` in `web/` proxies to a running `marg serve`. The protocol is in
[`docs/viewer-protocol.md`](docs/viewer-protocol.md).

## Builds

Every push to `master`, every pull request and every `v*` tag runs the
[`build` workflow](.github/workflows/build.yml). It produces, as workflow artifacts:

| Artifact | What |
| --- | --- |
| `cli-linux-x86_64-musl` | static `marg` binary for any x86_64 Linux (HPC clusters, `deploy/marg.def`) |
| `cli-linux-x86_64-gnu` | `marg` for glibc Linux (about 7% faster than the static build) |
| `cli-macos-arm64` | `marg` for Apple-silicon Macs (opt-in, see below) |
| `wheel-linux-x86_64`, `wheel-macos-arm64` | Python wheels (`pip install marg-*.whl`, Python ≥ 3.11; macOS opt-in) |
| `sdist`, `web` | Python source distribution; web viewer bundle |

Each CLI archive contains the binary, the parameter register (`params/`), the shipped scenarios and
a SHA-256 checksum; set `MARG_PARAMS_DIR` to its `params/base`. Binaries and wheels are
smoke-tested by running a scenario before upload. Pushing a tag `vX.Y.Z` also publishes
everything as a GitHub release (0.x versions as pre-releases).

macOS builds run only when the repository variable `BUILD_MACOS` is `true` (Settings → Secrets
and variables → Actions → Variables), because they need macOS runners, which this repository
does not get by default (GitHub cancels such jobs before they start).

To release a version:

1. Set `version` in `Cargo.toml` (`[workspace.package]`). The Python package and every crate
   take it from there; `web/package.json` is set by hand.
2. Write its section in [`CHANGELOG.md`](CHANGELOG.md). CI fails without one, and it becomes the
   body of the release (`scripts/release-notes.sh` prints it).
3. Once that change is on `master` with green checks, tag the merge commit:

```bash
git tag v0.1.0 && git push origin v0.1.0
```

## Experiments

An experiment is a design × arms × replications over a scenario, expanded into a run list with
run hashes and run locally or on SLURM. Results land in one Parquet dataset; reports render from it.

```python
import marg
from marg.design import Factor, Morris

reg = marg.Register()
exp = marg.Experiment(
    "screen",
    "scenarios/corridor_capacity.yaml",
    design=Morris([Factor.relative("interaction.following.time_gap.car", 0.25, reg)]),
    arms={"observed": {}, "cooperative": marg.bundles.cooperative()},  # twin arms, same seeds
    replications=marg.Replications(n=2),
)
exp.run(marg.backends.Local(workers=8))  # or marg.backends.Slurm(partition=..., chunk=50)
k = exp.results("kpis")  # Polars, one row per run, with scenario/point/arm/rep/factors
marg.sensitivity.morris(exp, ["throughput_veh_h"])
```

```bash
marg batch runs/screen --chunk 3 --chunk-size 50   # one SLURM array task; resumable
marg collect runs/screen                           # validate, write index.parquet
marg report sensitivity runs/screen                # HTML + PDF (Quarto, or pandoc + Chrome)
```
