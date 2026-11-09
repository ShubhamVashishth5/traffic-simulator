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
