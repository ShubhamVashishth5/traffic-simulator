# Open questions

**These need your input.** Each one has a default, so work can start without an answer. "Needed by"
is the first phase that depends on it.

| # | Question | Why it matters | Default if unanswered | Needed by |
| --- | --- | --- | --- | --- |
| 1 | What is the exact definition of the **social efficiency deficit** in your literature review? `claude/literature-review-summary.md` is mentioned in the guide summary but is not in this repo. Can you add it? | It's a headline KPI, and the twin-arm design depends on what "cooperative" means | Relative excess total delay vs a cooperative twin arm (see 04 §3) | P3 |
| 2 | Can the **full parameter guide** be exported into `plan/reference/` as Markdown? Is the repo public? | Build sessions need every number from §9. Reading the Claude Doc works but is slower, and a copy in the repo makes P1.3 reproducible | Sessions read the Claude Doc via the docs tools | P1 |
| 3 | **Data access.** Do you have, or can you request, the Chennai trajectory set (Kanagaraj et al.)? Do you plan your own video, and at which sites? | Stages 2–3 need trajectories; stages 1, 4, 5 and 6 need site video | Synthetic recovery tests only until data arrives | P5 / P10 |
| 4 | **Name.** Keep the working name `marg`, or pick another? | Crate, package and CLI names. Cheap to change before P2 | `marg` | P2 |
| 5 | **HPC details.** SLURM? Is Apptainer/Singularity allowed? Typical cores per node and wall-time limits? | Shapes the backend and chunk size | SLURM + Apptainer, 1 run per core, chunks of ~1 h wall time | P3 |
| 6 | **Laptop OS** (Windows, macOS, Linux)? | Which wheels and binaries CI must build | Linux + macOS arm64; Windows via WSL | P0 |
| 7 | **First study sites.** Which corridors, and which Shahdara- and Lutyens'-type patches? | Reference scenarios and the OSM import targets | Synthetic Varanasi-like and Chennai-like corridors first | P6 / P11 |
| 8 | **Emissions** for GRAP or air-quality questions? | Would add an emissions observer (out of scope now) | Not planned | — |
| 9 | **An RL / Gym interface** (e.g. signal control or enforcement placement)? | Cheap add-on over the Python API | Not planned | — |
| 10 | **Who else will use the viewer?** (supervisors, a public demo) | Decides whether replay should be hostable as a static site with sample runs | Local only | P4 |
