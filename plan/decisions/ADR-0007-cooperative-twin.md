# ADR-0007: What the cooperative twin switches off

- **Status:** proposed: needs the user's decision (open question 1)
- **Date:** 2026-11-09
- **Phase / task:** P3.3 (read by P5 squeezing and P8 violations)

## Context
The social efficiency deficit (`sed`, G§8.4) compares an arm with its **cooperative twin**: the same
seed, demand and agents, with selfish behaviour switched off. The P3.3 card asks for
`marg.bundles.cooperative()`: "violation hazards off, κ fixed at the population lower quantile for
squeezing". Three things are not settled:

1. which quantile "the population lower quantile" means;
2. whether κ is fixed **only in the squeezing decision** or everywhere κ acts (κ also scales `T`,
   `s0`, `v0`, gaps, clearance and politeness, G§3.1);
3. the exact `sed` formula, which open question 1 asks the user to supply from the literature review.

Neither violations (P8) nor squeezing (P5) exist yet, so nothing in the P2 core reads the switches.

## Options
For the quantile:
1. **Lower quartile (q = 0.25).** A clearly cooperative population that is still realistic.
2. **Lower decile (q = 0.10).** Closer to "everyone polite"; further from observed behaviour.
3. **The population minimum, or κ = −∞ in the squeeze rule.** Squeezing off entirely. This
   confounds the cooperative effect with a different gap-acceptance regime.

For the scope of κ:
1. **Squeezing only (proposed).** Following, desired speeds and clearance keep each agent's own κ,
   so the twin differs from the observed arm only in the selfish decisions.
2. **Everywhere.** The twin is a different population, not the same population behaving
   cooperatively. Paired differences then mix a composition effect into `sed`.

## Decision (proposed, pending review)
- `marg.bundles.cooperative(kappa_quantile=0.25)` returns two `scenario`-role switches:
  - `violations.enabled = 0`: every violation hazard off;
  - `personas.cooperative.squeeze_kappa_quantile = q`: the squeezing rule uses the κ at quantile
    `q` of the population distribution instead of the agent's own.
- κ is replaced in squeezing only (scope option 1).
- `q = 0.25` is the proposed default and is a function argument, so the experiment report records
  which quantile was used. It changes only through this ADR.
- `sed` stays the default from `plan/04` §3 until open question 1 is answered: the relative excess
  of total delay in an arm over its cooperative twin.

## Consequences
- Until P5 and P8 implement the switches, the cooperative arm simulates exactly what the observed
  arm does (tested: identical KPIs under shared seeds). Its run hash still differs, because the
  resolved parameters differ.
- P5 must read `personas.cooperative.squeeze_kappa_quantile` in its squeezing rule, and P8 must read
  `violations.enabled`. Both cards should cite this ADR.
