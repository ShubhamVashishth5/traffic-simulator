# Validation targets

Pattern-oriented validation targets (G§8.5, §9.8) and the "Check" rows of the guide's register.
These are **checks, not inputs** (CLAUDE.md rule 3): no model code may read them, and no parameter
may be tuned towards them directly.

Each target has a `use`:

- `fit`: the calibration stage named in `stage` fits against it;
- `check`: must emerge without fitting (e.g. the width–capacity curve);
- `ranking`: only the order matters (e.g. challan counts).

Pass/fail rules are in `tolerance`. The validation scorecard (P3, P10) reads these files.
