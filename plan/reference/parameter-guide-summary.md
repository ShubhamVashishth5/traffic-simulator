# Simulator Parameter Guide: summary

Full guide: [parameter-guide.md](parameter-guide.md) (exported from the Claude Doc,
https://claude.ai/artifact/3jViwPq85Tf1Adcr7Nad38).
Companion to `claude/literature-review-summary.md`. Written November 2026.

## Design rules
- Draw every parameter from a distribution, not a constant.
- Give every parameter a trust grade: **A** (measured in Indian mixed traffic), **B** (measured elsewhere or partly in India), **C** (unmeasured; sweep it).
- Give every parameter a role: **Fix** (measured), **Fit** (calibrated in stages) or **Sweep** (experimental factor).
- Road defects act through the agents' own mechanisms: usable width, local speed caps, gap supply and violation incentives. Never use global capacity multipliers. Published capacity losses are validation targets, not inputs.

## Sections
1. Vehicles: 12+ classes with dimensions, discharge acceleration, dynamic PCU (including e-rickshaw 0.71–0.82), and a free-flow speed chart by sub-class.
2. Interaction: time and standstill gaps (Agrawal 2026; Kanagaraj & Treiber 2018), lateral clearance C = 0.615v + 120.9 cm, reaction time, critical gaps.
3. Personas: a continuous trait space (κ etc.) plus 10 starter personas; persona shares are grade C.
4. Violations: one logistic decision rule, a catalogue of violations, and imitation in both directions.
5. Zones: compliance built from causes; do not calibrate from challans; the population-swap experiment.
6. Road situations:
   - potholes (2W swerve, heavy vehicles slow);
   - kerbside parking (capacity loss tracks width loss about 1:1, Varanasi);
   - bus stops; pedestrians (no effect below about 200 ped/h); animals;
   - signal states (dark signal = Chicken);
   - splits and mandatory lane changes with a deadline (no Indian study found; a gap to fill with video);
   - median openings, humps, gradients, rain bundle, work zones.
7. Demand:
   - registrations (67.65% 2W) are not the stream (21–33% 2W on Delhi arterials);
   - densities and the capacity–speed curve as checks;
   - shifted-exponential arrivals;
   - CSE corridor speeds (Lutyens' 44/52 vs arterials 26/27 km/h, confounded by road width);
   - night and GRAP scenario bundles.
8. Calibration:
   - Punzo et al. (2015): 3 of 6 IDM parameters suffice;
   - Carrella (2021): about half of ABMs have an unidentifiable parameter;
   - Platt (2020): Bayesian methods do best;
   - staged tuning order 0–7; Morris then Sobol; pattern-oriented validation against 7 patterns.
9. Master parameter register: grouped tables with value, distribution, source, grade and role, plus validation targets.
10. Sources: 131 linked sources grouped by section.
