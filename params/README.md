# Parameter register

Every number the model uses lives here (plan P1, CLAUDE.md rule 1). One YAML file per group; an
entry's full path is `<group>.<key>`, e.g. `interaction.following.time_gap.car`.

Each entry has:

| Field | Meaning |
| --- | --- |
| `value` | number or distribution (`dist: normal`, `log_normal_median`, …) in `unit`; `null` = not yet known (must be measured, or set by a scenario or sweep before any module uses it) |
| `unit` | unit of `value` and `range`; the resolver converts to SI |
| `grade` | A measured in Indian mixed traffic · B measured elsewhere or partly in India · C never measured |
| `role` | `fix` measured, never calibrated · `fit` calibrated in `stage` (G§8.3) · `sweep` experimental factor · `scenario` set per scenario |
| `source`, `guide`, `register_row` | citation, guide section, and the row of the guide's master register (§9) |
| `range` | hard bounds for overrides and sweeps |
| `live` | whether the web viewer may change it during a run |
| `notes` | assumptions, e.g. how a reported range became a distribution |

Validation targets (G§9.8 and the "Check" rows of §9) are **not** inputs and live in
`validation/targets/`. `guide_rows.yaml` lists every row of guide §9 and where it is implemented;
a test checks the coverage.

Lookups by vehicle fall back from type to class: `vehicles.free_flow_speed.motorcycle` is per type,
`interaction.following.time_gap.two_wheeler` is per class.
