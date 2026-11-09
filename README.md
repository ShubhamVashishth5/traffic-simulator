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
cargo run --release -p marg-cli -- run scenarios/toy.yaml --seed 1 --out runs/toy
```
