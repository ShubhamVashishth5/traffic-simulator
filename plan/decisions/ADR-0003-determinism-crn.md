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

- Philox4x32-10 streams keyed as above. (Drafted as ChaCha8; changed in P0.5 because Philox is
  designed for counter-based use: any `(entity, counter)` block is computed directly, with integer
  arithmetic only, and it is verified against the Random123 known-answer vectors. It is also what
  JAX and cuRAND use.)
- decisions use per-agent decision streams;
- environment and objects have their own streams;
- the perceive, decide and move phases only read the previous state;
- `libm` for transcendental functions in decision code.

## Consequences
- Output is bitwise-identical for any thread count, tested in CI.
- Twins and the population swap get variance reduction for free.
- Spawn order must not affect draws, so agent IDs come from the demand stream.
- Each of an agent's parameter draws reads its own block of the agent's stream (block `k · 2³²` for
  draw `k`, P3.3). A sampler that consumes a variable number of values (rejection for truncated
  normals) therefore cannot shift the other draws: an arm that changes one distribution changes only
  that parameter. Found by the P3.3 twin test, where a desired-speed arm changed every car's time
  gap. The fix changed every agent's draws for a given seed, so the P2 milestone was re-run.
