# marg — lane-free mixed-traffic simulator

Research simulator for disordered (lane-free) mixed traffic on Indian urban roads, built to
implement the *Simulator Parameter Guide: Delhi Disordered Traffic*.

**Status:** early development. See [`plan/`](plan/README.md) for the design and the phased
roadmap, and [`plan/STATUS.md`](plan/STATUS.md) for progress.

## Layout

| Path | What |
| --- | --- |
| `crates/` | Rust workspace: `marg-params` (register, schema), `marg-net` (geometry), `marg-core` (simulation), `marg-io` (outputs), `marg-cli` (`marg` binary), `marg-server` (live viewer backend), `marg-py` (Python bindings) |
| `python/marg/` | Python research API |
| `web/` | Web viewer |
| `params/`, `scenarios/`, `validation/` | Parameter register, scenarios, validation targets |
| `plan/` | Design and roadmap |

## Quick start

```bash
cargo test
```

```bash
uv sync && uv run maturin develop --release && uv run pytest
```

```bash
cargo run --release -p marg-cli -- run scenarios/corridor_2km.yaml --seed 1 --out runs/reference
```

## Builds

Every push to `master`, every pull request and every `v*` tag runs the
[`build` workflow](.github/workflows/build.yml). It produces, as workflow artifacts:

| Artifact | What |
| --- | --- |
| `cli-linux-x86_64-musl` | static `marg` binary for any x86_64 Linux (HPC clusters, `deploy/marg.def`) |
| `cli-linux-x86_64-gnu` | `marg` for glibc Linux (about 7% faster than the static build) |
| `cli-macos-arm64` | `marg` for Apple-silicon Macs |
| `wheel-linux-x86_64`, `wheel-macos-arm64` | Python wheels (`pip install marg-*.whl`, Python ≥ 3.11) |
| `sdist`, `web` | Python source distribution; web viewer bundle |

Each CLI archive contains the binary, the parameter register (`params/`), the shipped scenarios and
a SHA-256 checksum; set `MARG_PARAMS_DIR` to its `params/base`. Binaries and wheels are
smoke-tested by running a scenario before upload. Pushing a tag `vX.Y.Z` that matches the
workspace version in `Cargo.toml` also publishes everything as a GitHub release:

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
