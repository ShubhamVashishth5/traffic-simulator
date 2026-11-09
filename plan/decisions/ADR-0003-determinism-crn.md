# ADR-0003: Counter-based RNG streams and a two-phase update

- **Status:** proposed
- **Date:** 2026-11-09
- **Phase / task:** plan (implemented in P0.5, P0.6, P2.10)

## Context
The key experiments are paired comparisons: the population swap, dark vs working signal, the
cooperative twin used for the social efficiency deficit, and pothole repair. Paired designs need
**common random numbers**: the same agent gets the same draws in both arms. Results must also be
reproducible across thread counts and machines (laptop vs HPC), and live sessions must be
replayable.

## Options
1. One global RNG, consumed in update order. Order-dependent, which breaks twins and threading.
2. Per-thread RNGs. Results depend on thread count.
3. **Counter-based streams** keyed by `(master_seed, stream kind, entity id)`, plus a two-phase
   update: read the snapshot, compute in parallel, then apply.

## Decision
Option 3:

- ChaCha8 streams keyed as above;
- decisions use per-agent decision streams;
- environment and objects have their own streams;
- the perceive, decide and move phases only read the previous state;
- `libm` for transcendental functions in decision code.

## Consequences
- Output is bitwise-identical for any thread count, tested in CI.
- Twins and the population swap get variance reduction for free.
- Spawn order must not affect draws, so agent IDs come from the demand stream.
