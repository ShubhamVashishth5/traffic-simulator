# Open questions

**Project goal (the user, 2026-11-09):** a hyper-realistic, highly tunable, CPU- and
memory-efficient lane-free traffic simulator that **anyone can use**. It is not tied to one research
project. Delhi, through the parameter guide, is its first calibrated setting, not its only use.
Questions are framed around that goal.

## Open

Each has a default, so work can go on without an answer. "Needed by" is the first phase that
depends on it.

| # | Question | Why it matters | Default if unanswered | Needed by |
| --- | --- | --- | --- | --- |
| 1 | **Social efficiency deficit and the cooperative twin.** Ship them as configurable metrics, with the default formula (relative excess of total delay over the cooperative twin) and ADR-0007's defaults (κ at the population's 25th percentile, in squeezing only) as settings users can change? | One of several outputs users can switch on, not a headline result. P5 (squeezing) and P8 (violations) must read the twin switches | Yes: configurable, with those defaults (ADR-0007 to be accepted on that basis) | P5 |
| 3 | **Validation data for the shipped defaults.** Which public datasets should the shipped Delhi values be checked against (e.g. the Chennai trajectories of Kanagaraj et al., if openly available)? | Users calibrate to their own data with the P3 toolkit. The shipped defaults still need checks against real data, or they rest on the literature values and synthetic recovery tests alone | Literature values plus synthetic recovery tests; real-data checks wherever a public dataset is found | P5 / P10 |
| 4 | **Name.** Keep `marg`? | Used by every crate, the Python package and the CLI; cheaper to change now than later | `marg` | — |
| 7 | **Example scenarios.** Which places should ship as worked examples (corridors, a Shahdara-type and a Lutyens'-type area)? | Users start from them; P6 road objects and the P11 importer are tested on them | Synthetic Varanasi-like and Chennai-like corridors, then one OSM patch of each zone type | P6 / P11 |
| 9 | **An RL / Gym interface** (e.g. signal control or enforcement placement)? | Cheap over the Python API, and the P4 live-session code already steps runs one at a time | Not planned | — |
| 12 | **Other cities.** Should the register become a region-neutral core plus region profiles (Delhi first), so users elsewhere supply their own values as one layer? | "Anyone can use" includes places the guide does not cover. Today the base register *is* Delhi; other values come in as scenario or calibration overrides | Keep the Delhi register as the base; other regions override it until a profile layer is designed (ADR when needed) | P11 |

## Answered

| # | Question | Answer (2026-11-09) | What it changes |
| --- | --- | --- | --- |
| 2 | Export the full guide into the repo? Is the repo public? | Yes; the repo is public and publishing the guide is fine | Full copy in [reference/parameter-guide.md](reference/parameter-guide.md); sessions read it there |
| 5 | HPC details | Clusters are planned for later. For now everything runs on one desktop: Ryzen 7600X (6 cores, 12 threads) and an RTX 4070 | Keep the SLURM and Apptainer backend (P3.5) for later. Size local defaults (workers, threads) for 6 cores. The RTX 4070 can run the P4.3 rendering check |
| 6 | Laptop OS | Windows | Add native Windows builds and CI (`x86_64-pc-windows-msvc` binary and wheel); WSL works until then |
| 8 | Emissions | Out of scope | — |
| 10 | Who else uses the viewer? | Local only | No static hosting of replays |
| 11 | Roundabouts | In scope: the simulator should handle most real situations. *How* to model them is decided later | ADR-0009 keeps options 1 (ring of ribbons) and 2 (junction box); option 3 (out of scope) is dropped. Decided at the P7.0 gate |
