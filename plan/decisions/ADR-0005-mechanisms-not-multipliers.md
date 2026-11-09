# ADR-0005: Road effects only through mechanisms, never capacity multipliers

- **Status:** accepted (design rule from the parameter guide)
- **Date:** 2026-11-09
- **Phase / task:** plan (enforced from P6)

## Context
The guide (§6) requires road defects to act through the agents' own mechanisms: usable width, local
speed caps, gap supply and violation incentives. Hard-coded capacity losses would make the
roads × behaviour decomposition circular.

## Decision
- The step loop exposes **no** hook for capacity or flow multipliers.
- Published losses (Varanasi width–capacity, the ~36% pothole speed loss, the bus-stop capacity
  drop, rain capacity curves) live in `validation/targets/` as checks.
- Scenario bundles (rain, night, GRAP…) may only override existing register parameters. A test
  enforces this.

## Consequences
- Some targets may be missed at first. The gap is reported and diagnosed: per the guide, a
  side-friction miss is fixed in stage 3 or 5, never by re-tuning free flow.
- Results can be defended as emergent rather than imposed.
