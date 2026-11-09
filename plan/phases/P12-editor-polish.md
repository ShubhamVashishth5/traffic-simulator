# P12 · Scenario editor and polish

**Goal:** build and change scenarios in the browser without touching YAML; export clips for talks;
publish documentation; make the tool installable by other researchers.

- **Depends on:** P4, P11.

## Tasks

### P12.1 Web scenario editor
- **Do:**
  - Draw or edit ribbons and width profiles.
  - Place objects: potholes, parked cars, bus stops, humps, vendors, work zones.
  - Edit junction movements and signal plans; set zones and police posts.
  - Validate edits against the schema; save as a scenario YAML with provenance (`edited_in_ui`).
- **Done when:** a new corridor with a bus stop and a split is built and run with no YAML edits.

### P12.2 Clip export
- **Do:** render a replay time window to MP4 or GIF (a headless browser or a server-side renderer),
  with an overlay of time and KPIs.
- **Done when:** a 30-second clip exports from a replay.

### P12.3 Documentation site
- **Do:** user guide, model reference (generated from the ADRs and model design), parameter register
  browser (generated), data dictionary, and examples. Publish with mkdocs-material or Quarto
  website.
- **Done when:** the docs build in CI.

### P12.4 Packaging and release
- **Do:** wheels on PyPI (or a GitHub release), static binaries, an Apptainer image, `CITATION.cff`,
  a Zenodo DOI for versions used in papers.
- **Done when:** a fresh machine can `pip install` and run the examples.

## Milestone check
A colleague with no prior setup installs the tool, opens the editor, builds a scenario, runs a small
experiment and renders a report.
