# Simulator plan: lane-free Delhi mixed traffic

**This folder is the plan for a research traffic simulator that implements the
[Simulator Parameter Guide](https://claude.ai/artifact/3jViwPq85Tf1Adcr7Nad38)** (summary in
[reference/](reference/parameter-guide-summary.md)).

- **Core:** Rust, built for many fast runs on a laptop or HPC.
- **Research layer:** Python, for experiments, calibration, sensitivity and reports.
- **Viewer:** web, for live runs with parameter tweaking, replays and twin comparisons.
- **Output:** every result is Parquet, carrying the resolved parameter register (value, grade, role,
  source).
- **Build:** 13 phases, each split into tasks sized for an AI session.

Status: **draft for review** (2026-11-09). No code yet.

## How to review (about 30 minutes)

1. [05-roadmap.md](05-roadmap.md): the phase order and the milestone experiment for each phase. *Is
   the order right for your thesis?*
2. [02-architecture.md](02-architecture.md): stack, components, performance targets. *Anything you'd
   veto?*
3. [open-questions.md](open-questions.md): ten questions, each with a default. *Answer what you can.*
4. Skim [03-model-design.md](03-model-design.md): items marked ⚑ are proposed formulations where the
   guide leaves room. P5 and P8 open with review gates on the two biggest ones.
5. [04-data-outputs-analytics.md](04-data-outputs-analytics.md): output tables, metrics, the
   validation scorecard and reports. *Is anything missing for your papers?*
6. [01-feature-survey.md](01-feature-survey.md) is background. It covers what SUMO, Vissim, Aimsun,
   MATSim, CityFlow, A/B Street and others offer as applications, and which features we adopt.

Comment inline on the PR, or edit the files directly. Both work.

## Key decisions

| Decision | Status | Record |
| --- | --- | --- |
| Rust core + Python API + web viewer | accepted (your choice) | [ADR-0001](decisions/ADR-0001-rust-core-python-web.md) |
| Continuous lateral position on ribbons; no lanes or strips | proposed | [ADR-0002](decisions/ADR-0002-continuous-lateral-ribbons.md) |
| Counter-based RNG, two-phase update: deterministic runs and common random numbers for twins | proposed | [ADR-0003](decisions/ADR-0003-determinism-crn.md) |
| Parquet outputs, resolved register per run, run identity by hash | proposed | [ADR-0004](decisions/ADR-0004-parquet-outputs-run-identity.md) |
| Road effects only through mechanisms, never capacity multipliers | accepted (guide rule) | [ADR-0005](decisions/ADR-0005-mechanisms-not-multipliers.md) |
| Built for corridors first, structured for zones of 20–100 junctions | accepted (your answer) | [02 §1](02-architecture.md#1-space-ribbons-not-lanes), [P11](phases/P11-network-scale.md) |
| Design factors on distributions set the mean at fixed CV | proposed (P3.2) | [ADR-0006](decisions/ADR-0006-design-factors-on-distributions.md) |
| What the cooperative twin switches off | **needs your decision** (P3.3, open question 1) | [ADR-0007](decisions/ADR-0007-cooperative-twin.md) |
| Live sessions: commands act between steps, resample keeps each agent's quantile, run hash covers the commands | proposed (P4.1, P4.5) | [ADR-0008](decisions/ADR-0008-live-sessions.md) |

## Folder map

| Path | What it is |
| --- | --- |
| [01-feature-survey.md](01-feature-survey.md) | Existing simulators and the application features we take from them |
| [02-architecture.md](02-architecture.md) | Tech stack, architecture diagram, core design, performance targets, quality gates |
| [03-model-design.md](03-model-design.md) | Guide → equations and data structures, section by section |
| [04-data-outputs-analytics.md](04-data-outputs-analytics.md) | Run folders, Parquet tables, metrics, validation patterns, experiments, calibration, reports |
| [05-roadmap.md](05-roadmap.md) | The 13 phases, dependencies, milestone checks |
| [06-ai-workflow.md](06-ai-workflow.md) | How to iterate with AI: loop, prompt template, rules, review checklist |
| [phases/](phases/) | One task card per phase, each task with "Done when" checks |
| [decisions/](decisions/) | Architecture decision records (ADR template included) |
| [STATUS.md](STATUS.md) | Task tracker, updated by every PR |
| [open-questions.md](open-questions.md) | Questions for you, each with a default |
| [reference/](reference/) | The parameter guide summary |

## Starting the build

When the plan is approved, start with P0.1 using the prompt in
[06-ai-workflow.md](06-ai-workflow.md#session-prompt-template). P0 and P1 need no answers from the
open questions, except question 2 (a guide export) for P1.3.
