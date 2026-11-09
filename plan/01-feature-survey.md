# 01 · Feature survey of existing traffic simulators

**Bottom line.** We don't need to invent features: SUMO, Vissim and Aimsun already show the full set of
*application* features researchers expect (live GUI, a runtime API, virtual detectors, trajectory and
conflict output, seeded replications, dashboards). We copy those. What none of them offers is the
research layer this project needs most:

- native lane-free 2-D behaviour;
- a parameter register with A/B/C grades and Fix/Fit/Sweep roles;
- staged calibration that freezes each stage;
- built-in sensitivity analysis;
- paired "twin" counterfactual runs.

Those five are where this simulator stands out.

Surveyed November 2026. Model behaviour follows the parameter guide, not these tools. Only their
application features are borrowed.

## Simulators looked at

| Simulator | What it is | Features worth taking | Gap for this research |
| --- | --- | --- | --- |
| [SUMO](https://sumo.dlr.de/docs/SUMO.html) (DLR, open source, C++) | Most-used open microsimulator | [Sublane model](https://sumo.dlr.de/docs/Simulation/SublaneModel.html) for mixed widths; TraCI/libsumo runtime API; SUMO-GUI with per-object [parameter dialogs](https://sumo.dlr.de/docs/SUMO-GUI.html), colour-by-attribute and breakpoints; netedit; detectors (E1/E2/E3, edgeData); [FCD trajectories](https://sumo.dlr.de/docs/Simulation/Output/FCDOutput.html); [SSM device](https://sumo.dlr.de/docs/Simulation/Output/SSM_Device.html) (TTC, DRAC, PET); [CSV/Parquet output](https://sumo.dlr.de/docs/TabularOutputs.html) since 1.24 (FCD file 74 MB vs 357 MB XML) | Sublanes are a discretisation: maintainers say [resolution adds noise and slows runs](https://www.mail-archive.com/sumo-user@eclipse.org/msg12139.html). There's no persona or violation layer and no calibration tooling |
| PTV Vissim (commercial) | Industry standard, used in Indian calibration work | [Non-lane-based lateral behaviour](https://www.aperza.com/catalog/page/894/5266/) (vehicles of different widths share a lane); data collection points, queue counters, node evaluation; vehicle record; multirun with seed increments; [SSAM conflict export](https://cgi.ptvgroup.com/download/ptv_vision/VISSIM/downloads/200/ReleaseNotes_VISSIM_2020.00-06_ENG.pdf) limited by section and time; COM API | Closed source and licensed per seat, so HPC sweeps cost money. Behaviour is a black box, which is a problem when the paper's claim is about behaviour |
| [Aimsun Next](https://docs.aimsun.com/next/latest/UsersManual/DynamicScenariosAndExperiments.html) (commercial) | Micro, meso and hybrid | A clean [scenario → experiment → replication](https://docs.aimsun.com/next/23.0.0/UsersManual/ScenariosExperimentsResultsReplications.html) hierarchy: one seed per replication, averaged results | Lane-based, closed source |
| MATSim + [SimWrapper](https://simwrapper.github.io/docs) / Via | Agent-based demand and assignment | Events-file output; SimWrapper web dashboards defined in YAML (charts, link maps, agent animation) that build automatically after a run; Via desktop animation | Queue-based links with no microscopic interaction. Wrong level for lateral behaviour |
| [CityFlow](https://arxiv.org/abs/1905.05217) | Multi-threaded C++ simulator for RL | Speed through multithreading: authors report [over 20× SUMO throughput](https://arxiv.org/abs/1905.05217); Python API; web renderer | Simplified lane-based physics |
| [A/B Street](https://a-b-street.github.io/docs/software) | Rust simulator with OSM import, native or WebAssembly | Edit roads and signals, then compare before and after; runs in the browser; OSM import | Built as a planning game. No lateral interaction and not research-grade output |
| [OpenTrafficSim](https://opentrafficsim.readthedocs.io/) (TU Delft, Java) | Multi-level research framework | Typed units; behaviour built from components (LMRS lineage); a KPI module | Lane-based; Java toolchain |
| [MovSim / traffic-simulation.de](https://github.com/movsim/traffic-simulation-de) | Treiber's browser demos (IDM, MOBIL) | Live parameter sliders that change behaviour instantly. The interaction model for our web view | Teaching tool only |
| [SiMTraM](https://sourceforge.net/projects/simtram/) (IIT Bombay) | SUMO extension for Indian mixed traffic | Strip-based lane-free movement, calibrated on Mumbai data | Stuck on SUMO 0.12/0.17 and unmaintained |

## Feature catalogue and what we do with each

"Phase" points to [05-roadmap.md](05-roadmap.md).

| # | Application feature | Seen in | Our version | Phase |
| --- | --- | --- | --- | --- |
| 1 | Seeded replications, scenario → experiment → run | Aimsun, Vissim | Experiment manager. Runs keyed by hash. Replications stop when the 95% CI is tight enough | P3 |
| 2 | Paired comparisons with common random numbers | (rare) | **Twin runs**: the same agents get the same draws in both arms. Used for population swaps, dark vs working signals, pothole repairs | P3 |
| 3 | Virtual detectors and sections | SUMO E1/E2/edgeData, Vissim | Screenlines and areas: counts, PCU, speeds, density, lateral histograms per class | P2 |
| 4 | Trajectory output | SUMO FCD, Vissim vehicle record | Parquet trajectories with configurable rate and spatial windows | P2 |
| 5 | Surrogate safety and conflicts | SUMO SSM, Vissim SSAM | Online 2-D TTC, PET and DRAC for oriented rectangles, logged as events | P5 |
| 6 | Columnar research output | SUMO 1.24 Parquet | Parquet everywhere, with versioned schemas and a generated data dictionary | P2 |
| 7 | Runtime control API | TraCI, COM, CityFlow | Python API (PyO3) plus live commands over WebSocket | P2, P4 |
| 8 | Live GUI: play, pause, step, speed, breakpoints | SUMO-GUI | Web live view; breakpoints on events (e.g. "pause at first gridlock") | P4 |
| 9 | Object inspector | SUMO-GUI parameter dialog | Agent inspector: traits, drawn parameters and **why** it decided ("logit terms for this red-light run") | P4, P8 |
| 10 | Colour by attribute | SUMO-GUI | Colour by class, persona, κ, speed, violation state or zone | P4 |
| 11 | Live parameter sliders | traffic-simulation.de | Panel generated from the parameter schema, with grade and role badges; every change is logged | P4 |
| 12 | Dashboards | SimWrapper, Aimsun | Live KPI strip in the viewer; Quarto reports after runs | P3, P4 |
| 13 | Before/after comparison | A/B Street | Side-by-side twin view: two synchronised runs, one map, one difference panel | P4 |
| 14 | Network editor | netedit, Vissim, A/B Street | YAML corridor builder first, then a web editor that places objects and draws ribbons | P2, P12 |
| 15 | OSM import | SUMO netconvert, A/B Street, MATSim | Python importer (OSMnx) → ribbon network, with manual width overrides | P11 |
| 16 | Multithreading for scale | CityFlow | Rayon two-phase update; identical results for any thread count | P2, P11 |
| 17 | Pedestrians | SUMO, Vissim Viswalk | Crossing agents with gap acceptance and rolling crossings | P6 |
| 18 | Public transport | all | Buses with stops, dwell times and stopping offsets | P6 |
| 19 | Screenshots and video export | SUMO-GUI, Via | Export replay clips to MP4 or GIF for talks | P12 |
| 20 | Calibration tooling | mostly external | Staged calibration that freezes each stage (guide §8.3), enforced in code | P3, P10 |
| 21 | Sensitivity analysis | external | Morris → Sobol → GP surrogate, built in (SALib) | P3, P10 |
| 22 | Provenance | weak everywhere | Every run stores the resolved register: value, distribution, grade, role and source | P1 |
| 23 | Meso or hybrid modelling | Aimsun | Not planned. Revisit only if city-wide context is needed | — |
| 24 | Emissions | SUMO (HBEFA, PHEM) | Not planned. Possible add-on for GRAP scenarios (open question) | — |
| 25 | RL environment (Gym) | CityFlow, SUMO-RL | Not planned. Cheap to add later on top of the Python API | — |
| 26 | 3-D view | Vissim, Aimsun | Not planned | — |

## What we deliberately do differently

- **Continuous lateral position, not strips or sublanes.** SUMO's sublanes and SiMTraM's strips
  discretise width. The guide's mechanisms are continuous in metres: clearance as a function of speed,
  effective width, pothole offsets. A strip grid would add resolution artefacts to exactly the
  quantities we validate.
- **Behaviour is open and decomposable.** Every decision can be explained term by term (feature 9).
  That matters for a thesis whose claim is "roads × behaviour".
- **Research workflow first, GUI second.** Commercial tools are GUI-first and scripted as an
  afterthought. Here the headless runner and output schema are the product, and the viewer reads the
  same data.
