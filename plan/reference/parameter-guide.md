<!--
Full copy of the Simulator Parameter Guide, exported as Markdown from the Claude Doc
(https://claude.ai/artifact/3jViwPq85Tf1Adcr7Nad38, revision 13) on 2026-11-09, with the
author's permission (open question 2). The register in params/base/ cites it as "G§n".
One embedded chart was replaced by its caption in the export. When the guide changes,
re-export it here in the same commit as any register change it causes.
-->

# Simulator Parameter Guide: Delhi Disordered Traffic

Nov 9, 2026 · @Shubham

## How to use this guide

This guide lists every parameter a realistic Delhi traffic simulator needs: what value or distribution to start from, which study it comes from, and how hard to trust it. It is a companion to the literature review *Selfish Roads*, and it covers vehicles, drivers, violations, area-level compliance and road conditions.

Three rules run through it:

- **Draw every parameter from a distribution, never a constant.** Indian studies find large spread *within* each vehicle class: hatchback free-flow speeds have a standard deviation of about 15 km/h around a 61 km/h mean. A simulator with one speed per class cannot reproduce seepage, overtaking or platoon break-up.
- **Give every parameter a trust grade.**
  - **A:** measured in Indian mixed traffic. Calibrate it.
  - **B:** measured elsewhere, or only partly in India. Give it a wide prior and test sensitivity.
  - **C:** never measured. Treat it as an experimental factor you sweep across a range. Most behaviour and violation parameters are C, and saying so openly is what makes the results defensible.
- **Tie every parameter to a mechanism.** If changing a parameter changes no emergent pattern you can check (Section 8), it is cheaper to drop it than to calibrate it.

Units are SI unless a header says otherwise. Speeds are given in km/h where the source reports them that way. Indian sub-class names (scooty, auto, e-rickshaw, Gramin Sewa) are used as Delhi residents use them.

## 1. Vehicle classes

Use at least 12 vehicle classes, and split two-wheelers and cars into sub-classes. Indian data show sub-classes behave differently: mopeds run about 13 km/h slower than motorcycles in free flow ([Balakrishnan & Sivanandan 2015](https://publications.iitm.ac.in/publication/influence-of-lane-and-vehicle-subclass-on-free-flow-speeds)). SUVs take up about 1.5 times a hatchback's road share at similar speeds ([Dhamaniya & Chandra 2017](https://link.springer.com/content/pdf/10.1007/s40999-017-0206-7.pdf)).

| Class (sub-class) | Length × width (m) | Mean accel / decel at signals (m/s²) | Urban dynamic PCU | Notes |
| --- | --- | --- | --- | --- |
| Bicycle | ≈1.9 × 0.5 (typical; grade B) | not measured | 0.31–0.37 | Speed in traffic ≈15 km/h (Sonipat model) |
| Cycle-rickshaw | not measured recently (C) | not measured | static IRC value only | Old-city Delhi; slow, wide, often wrong-side |
| Moped / scooty | 1.87 × 0.64 (all 2W) | 0.60 / 0.55 (all 2W) | 0.21–0.24 | Lower free-flow speed than motorcycles |
| Motorcycle (incl. bike taxi) | 1.87 × 0.64 | 0.60 / 0.55 | 0.21–0.24 | Bike taxi = same vehicle, different persona (Section 3) |
| E-rickshaw | legal envelope ≤ 2.6 × 1.0 | not measured (C) | 0.71–0.82 | Legal cap 25 km/h, ≤ 2,000 W motor, 4 passengers + driver |
| Auto-rickshaw (CNG) | 3.20 × 1.40 (Delhi); median 2.92 × 1.35 (Chennai) | 0.43 / 0.59 | 0.97–1.04 | Peak-hour operation mostly 6–28 km/h |
| Hatchback | 3.72 × 1.44 | 0.49 / 0.62 (all cars) | 1.00 (reference) | Delhi's "standard car" |
| Sedan | all cars: 4.46 ± 0.42 × 1.85 ± 0.12 | 0.49 / 0.62 | 1.0–1.6 | Use a length distribution, not one size |
| SUV / big car | 4.58 × 1.77 | 0.49 / 0.62 | 1.53–1.66 |  |
| LCV (goods, e.g. small pickups) | not separated | 0.46 / 0.78 | 2.4–2.8 | Frequent kerbside loading |
| City bus | 10.10 × 2.43 | not measured | 5.7–6.3 | Dwell at stops; see Section 6 |
| Truck / heavy vehicle | median 11.97 × 2.92 (heavy commercial) | not measured | 3.7–5.2 | Delhi entry timings restrict daytime trucks |

&#91;embedded content: Balakrishnan & Sivanandan (2015): about 17,800 vehicles on 24 sections of four- and six-lane divided roads in Chennai · e-rickshaw cap from CMVR\]

The spread inside the car classes is larger than the whole gap between mopeds and motorbikes, so each agent needs its own desired speed drawn from its sub-class distribution. Every motorised class except the e-rickshaw sits well above the 25 km/h legal cap, which is why e-rickshaws act as moving bottlenecks on mixed roads.

Sources: dimensions from Dhamaniya & Chandra (2017, Delhi and other cities) and [Agrawal et al. 2026](https://arxiv.org/abs/2608.00602) (Chennai drone data). Acceleration and deceleration from [Ramireddy et al. 2021](https://pp.bme.hu/tr/article/download/16796/9272) (24 signal approaches in Hyderabad and Warangal). PCU from Dhamaniya & Chandra (Delhi) and [Khan & Dass 2025](https://komunikacie.uniza.sk/pdfs/csl/2025/02/10.pdf) (Sonipat, the first published e-rickshaw PCU). E-rickshaw legal limits from [CMVR Rule 2(cb)](https://advocategandhi.com/?p=28460) and the [2014 notification](https://www.deccanherald.com/archives/e-rickshaws-likely-used-goods-2208844). Auto-rickshaw operating range from a [CRRI study in Guwahati](https://2021.ursi.org/proceedings/procAP19/papers2019/URSISummaryPaperArtiChoudhary.pdf).

Three cautions for using these numbers:

- **Signal acceleration rates are averages over a whole manoeuvre, not vehicle capability.** The 0.4–0.6 m/s² values describe how traffic actually discharges. For a car-following model's maximum acceleration, start from the calibrated values in Section 2 (1–2 m/s²), and check that simulated discharge reproduces these averages.
- **PCU is an output to check, not an input to set.** PCU depends on vehicle mix and speed, so let it emerge from vehicle size, speed and gap behaviour. Then compare it with these ranges. The older static IRC:106 values undervalue buses and trucks and overvalue two-wheelers relative to current measurements (Khan & Dass 2025).
- **E-rickshaws, cycle-rickshaws and bicycles are the least measured classes**, yet they set the speed of mixed old-city streets. Their e-rickshaw speed cap is a legal limit, not observed behaviour. One Sonipat site modelled e-rickshaws at about 21 km/h against 33 km/h for cars.

## 2. Interaction parameters

How vehicles keep distance from each other matters more to emergent traffic than any vehicle attribute. Two-wheelers following at half a second and half a metre are the main reason Indian roads carry more vehicles per metre of width than lane-based traffic.

### 2.1 Longitudinal following

| Class | Desired time gap T (s) | Standstill gap s₀ (m) | Max accel a / comfortable decel b (m/s²) | Desired speed v₀ (m/s) |
| --- | --- | --- | --- | --- |
| Motorcycle / scooty | median 0.49 (drone data); 0.3 (calibrated) | median 0.55; 0.5 | 2.0 / 2.0 | 18–25 |
| Auto-rickshaw | median 0.81; 1.0 | median 0.83; 2.0 | 1.0 / 1.0 | 5–6 (congested calibration) |
| Car | median 1.21; 0.8 | median 1.32; 2.0 | 1.0 / 1.0 | 12–18 |
| Bus | 1.0 (calibrated) | 2.0 | 1.0 / 1.0 | 10–14 |

First value in each cell: steady-state medians from [Agrawal et al. 2026](https://arxiv.org/abs/2608.00602) (Chennai, about 5,900 trajectories). Second: Intelligent Driver Model values calibrated by [Kanagaraj & Treiber 2018](https://arxiv.org/pdf/1805.05076), with emergency braking capped at 9 m/s². Treat these as priors with spread. The two sources disagree by up to a factor of 1.6 on time gaps, which is normal between studies and sites.

- **Following is partial.** In Chennai trajectories, a substantial share of drivers were not strictly following one leader ([Kanagaraj et al. 2015](https://idr.nitk.ac.in/handle/123456789/13557)). Drivers follow staggered leaders, two leaders at once, or squeeze between them ([Asaithambi et al. 2016](https://link.springer.com/content/pdf/10.1007/s40890-016-0025-6.pdf)). Choose the *effective leader* by lateral overlap, not by lane.
- **Time gap varies by leader–follower pair**, not just by follower. Indian car-following studies model leader type explicitly ([Munigety 2018](https://jtte.chd.edu.cn/article/doi/10.1016/j.jtte.2017.12.002)). Start with a multiplier matrix (follower × leader) set to 1, then fit it.

### 2.2 Lateral clearance

Across 6,016 measured interactions in Delhi, Guwahati, Kolkata, Bengaluru, Pune and Mumbai, mean lateral clearance rose with pair speed:

```latex
C\,[\mathrm{cm}] = 0.615\,\bar{v}\,[\mathrm{km/h}] + 120.9
```

That gives about 1.46 m at 40 km/h and 1.33 m at 20 km/h ([Budhkar & Maurya 2017](https://link.springer.com/article/10.1007/s40534-017-0130-1)). Pair effects to build in:

- Two-wheelers keep the smallest clearances of any class.
- Same-type pairs (car–car, auto–auto, bike–bike) keep less clearance than mixed pairs.
- Car–bike pairs: the clearance–speed slope is about 35% steeper than for same-type pairs, so cars give bikes more room as speed rises.
- Car–auto pairs: slope about 25% flatter.

For force-based 2-D models, Kanagaraj & Treiber calibrated lateral interaction scales of 0.15 m for braking and 0.30 m for steering, a lateral time constant of 1 s, a maximum heading angle of 0.2 rad, and a MOBIL politeness factor of 0.2.

### 2.3 Reaction time

No Indian mixed-traffic reaction-time study turned up in this search, so this is grade B. Use a right-skewed (lognormal) distribution. A video study of first-stopping drivers at amber found a 15th percentile of 0.68 s, mean 1.02 s and 85th percentile 1.28 s ([Wong & Goh](https://swov.nl/en/publicatie/driver-perception-response-time-braking-action-during-signal-change-interval), Singapore). If your car-following model has no explicit delay, add one only if string stability or start-up waves look wrong.

### 2.4 Gap acceptance at uncontrolled junctions

Critical gaps depend on both the entering vehicle and the vehicle it must cut across. At two Warangal intersections, clearing-behaviour critical gaps were ([Abhigna et al. 2020](https://pp.bme.hu/tr/article/download/9744/7927)):

| Entering vehicle | Critical gap (s) |
| --- | --- |
| Two-wheeler | 4.0–4.8 |
| Three-wheeler | 6.5–7.1 |
| Car | 7.7–8.6 |
| LCV | 7.0–9.9 |
| Bus | 8.6 |
| Tractor | 10.0–11.5 |

- Against a stream of two-wheelers, two-wheelers accepted gaps as short as 2.37 s. Gaps against heavy-vehicle streams were 30–35% longer than against two-wheeler streams.
- Other estimation methods give lower values: Raff's method gave about 2.6 s for two-wheelers, and other studies report critical gaps as low as 1.6 s.
- Young drivers have significantly smaller critical gaps than middle-aged and older drivers ([Dutta & Ahmed 2018](https://link.springer.com/doi/10.1007/s40534-017-0151-9)).
- **Forced entry** is a separate behaviour from accepting a gap. Model it as a probability that a waiting vehicle enters anyway, rising with waiting time and persona aggressiveness. This forces the major-road vehicle to brake. Section 4 treats it as a violation.

## 3. Driver personas

Model personas as clusters in a continuous trait space, not as separate rulebooks. A Mumbai study of drivers approaching a signal found no clear boundaries between aggressive, normal and timid drivers; a single normal distribution described them better ([IATSS Research 2022](https://www.scribd.com/document/603484926/Driver-Behaviour-Modelling)). Personas are useful labels for the dense regions of that space and for policy scenarios ("what if delivery riders were paid per hour?").

### 3.1 Trait dimensions every agent carries

| Trait | What it scales | Evidence it varies |
| --- | --- | --- |
| Aggressiveness κ | Desired speed, time gap, accepted gap, lateral clearance, politeness | Indian survey of 405 drivers clustered into aggressive, normal and cautious groups. Men were 2.57× more likely to drive aggressively, and aggressive drivers were 2.79× more likely to crash ([Traffic Injury Prevention 2021](https://informahealthcare.com/doi/abs/10.1080/15389588.2021.1965590)) |
| Time pressure | Willingness to defect when delayed; rises with waiting time | Gig riders report more speeding (56% vs 39%) and red-light running (21% vs 12%) than salaried riders ([Christie & Ward 2022](https://www.eurekalert.org/news-releases/970802), UK) |
| Risk tolerance | Accepted time-to-collision, forced entry, wrong-way use | Young drivers accept smaller gaps (Section 2.4) |
| Conformity | Probability of copying a nearby violator | Grade C. See Section 4.3 |
| Norm sensitivity | Response to visible enforcement and to others' compliance | Grade C. See Section 5 |
| Skill | Variance of speed and position control; reaction time | Licences often issued without a test ([Bertrand et al. 2007](https://www.nber.org/system/files/working_papers/w12274/w12274.pdf)): 69% of fast-tracked Delhi licensees failed an independent test |
| Distraction | Phone use; slower reaction; drifting | Delivery riders use phones while riding far more than other riders |

Add **within-trip variation** on top of between-driver differences. Car-following calibration errors come largely from the same driver behaving differently over time (Kesting & Treiber 2008). Indian naturalistic data show individual drivers switching between aggressive and non-aggressive patterns within one trip ([RSS 2022](https://www.nrso.ntua.gr/rss2022/wp-content/uploads/2022/06/RSS2022_Presentation_247.pdf)). A slow random walk on κ, plus a jump when delay builds up, is a simple way to do this.

### 3.2 Starter personas

| Persona | Typical vehicle | Trait settings (relative to average) | Signature behaviours | Evidence grade |
| --- | --- | --- | --- | --- |
| Average commuter | Car, scooty, motorcycle | All traits at population mean | Mostly compliant; defects when others do or when badly delayed | B |
| Rushed driver | Car, motorcycle | High κ and time pressure | Squeezing, late merging, amber running, honking | B |
| Delivery rider (Zomato, Swiggy, Blinkit, Zepto) | Motorcycle, scooty, e-scooter | Very high time pressure, high distraction | Wrong-way, red-light, footpath riding, phone use | B: [Chennai observation](https://citizenmatters.in/65-food-delivery-personnel-surveyed-in-chennai-violated-traffic-rules-finds-satta-panchayat-iyakkam/) found 73% of riders violating rules in 2018 and 65% in 2019 |
| Bike-taxi rider (Rapido, Uber Moto) | Motorcycle | High time pressure, moderate risk (carries a passenger) | Seepage, gap riding, kerbside pick-up stops | C: no Indian study found |
| E-rickshaw driver | E-rickshaw | Low speed, low skill, high stop propensity | Stopping anywhere, clustering at metro exits, wrong-way, turning right from the leftmost position | B: Delhi enforcement data (below) |
| Auto-rickshaw driver | CNG auto | High κ, fare-seeking | Sudden kerbside stops for passengers, forced entry, U-turns at median gaps | C |
| Bus driver (DTC, cluster) | 12 m bus | Moderate κ, schedule pressure | Stopping away from the bay, pulling out without yielding | B: buses more likely to choose "go" at amber, linked to competition for passengers ([Kathmandu 2026](https://www.nepjol.info/index.php/jotse/article/view/97211)) |
| Truck / LCV driver | Truck, LCV | Low acceleration, schedule pressure | Kerbside loading, slow lane-hogging | C |
| Cautious / older driver | Car | Low κ, high norm sensitivity | Compliant; often the one who gets cut in front of | B |
| Cyclist / cycle-rickshaw | Bicycle, cycle-rickshaw | Low speed, high wrong-way propensity (shorter detours matter more) | Wrong-way along kerbs, crossing at any point | C |

**E-rickshaw enforcement data, Delhi, 1 Jan–15 Jun 2025** ([Business Standard](https://www.business-standard.com/india-news/over-230000-traffic-challans-issued-to-e-rickshaw-drivers-in-delhi-125062001120_1.html)). There were 230,617 challans. Improper or obstructive parking accounted for 140,776 on-the-spot challans, against 3,749 for wrong-way driving and 1,190 for red-light jumping. These counts measure where police enforce, not true prevalence. Still, the 37:1 ratio of parking to wrong-way challans supports making **stopping behaviour** the defining e-rickshaw parameter.

A systematic review of food-delivery riders ([Velaga et al., TR-F 2026](https://trid.trb.org/View/2721800)) finds speeding, wrong-way riding, red-light violation, phone use and tailgating to be the dominant risky behaviours. Use that list to define the delivery-rider persona.

**Calibrating the persona mix:** the shares of each persona are grade C for Delhi. Sweep them, and report results as a function of the delivery-rider and e-rickshaw shares, since those are the two groups growing fastest.

## 4. Violations and strategic behaviours

Model each violation as a decision an agent takes when a trigger appears, not as a fixed rate. Indian evidence consistently shows violations cluster where they save time: two-thirds of Mumbai red-light running happens just before green ([TRR 2017](https://journals.sagepub.com/doi/10.3141/2619-08)), and wrong-way riders mainly avoid long median detours ([Vellore field study 2026](https://trid.trb.org/View/2707922)). Enforcement counts (challans) tell you where police look, not how common a violation is. Use them only to rank violations, never as rates.

### 4.1 One decision rule for all violations

```latex
P(\text{violate}) = \sigma\big(\beta_0^{v} + \beta_\kappa\,\kappa_i + \beta_t\,\text{time saved} + \beta_w\,\text{waiting time} + \beta_s\,\text{share of nearby violators} - \beta_e\,\text{perceived enforcement} - \beta_r\,\text{perceived risk}\big)
```

Here σ is the logistic function, v is the violation type and κᵢ the agent's aggressiveness (Section 3). This form makes the zone effects in Section 5 and the imitation effects in Section 4.3 enter through the same coefficients. The coefficients are grade C. Calibrate the intercepts β₀ to whatever observed rates exist, and sweep the rest.

### 4.2 Violation catalogue

| Violation | Who does it most | Trigger to model | Indian evidence | Grade |
| --- | --- | --- | --- | --- |
| Red-light running | Two-wheelers; buses at amber | Time into red; red-to-green ratio; visible police; countdown timer | Mumbai: about 1 in 17 drivers ran red. A third of events came right after red started (exponential timing), two-thirds just before green (extreme-value timing). Policing and the red-to-green ratio mattered | A |
| Early start with a countdown timer | Two-wheelers at the front of the queue | Seconds of red left on the display | Chennai: with timers, the share of cycles with violations at the *end* of red rose from 12% to 75%, while violations at the start of red fell from 59% to 31% ([Sharma et al. 2012](https://digitalcommons.unl.edu/civilengfacpub/53)) | A |
| Amber "go" | Buses, accelerating vehicles | Distance to stop line, speed, vehicle type | Vehicle type significantly affects the stop/go decision in Indian traffic ([Pathivada & Perumal 2019](https://bvbvf1.bib-bvb.de/vufind/PrimoRecord/cdi_proquest_journals_2187911483)). Kathmandu: 58% stopped, 20% crossed on amber, 22% ran red | B |
| Stop-line encroachment / seepage to front | Two-wheelers, autos | Gaps between queued vehicles | Vehicles standing past the stop line raise measured saturation flow ([Ramireddy et al. 2020](https://bvbvf1.bib-bvb.de/vufind/PrimoRecord/cdi_proquest_journals_2414021184)). This is a defection that can *raise* throughput | A |
| Wrong-way riding | Two- and three-wheelers | Detour saved versus the legal route; service roads; median length | Intentional, mostly by two- and three-wheelers. Authorities informally allow two-way use of some service roads (Vellore). Wrong-way crashes are more likely by day and at intersections ([Meganathan & Pulipati 2025](https://bvbvf1.bib-bvb.de/vufind/PrimoRecord/cdi_proquest_journals_3153698854)) | B |
| Footpath riding | Two-wheelers, sometimes autos | Queue length beside an accessible footpath (no bollards) | Bengaluru police booked [15,174 cases in 2024](https://www.newsminimalist.com/articles/motorists-increasingly-violate-pavements-in-bengaluru-a89adeaf). Prevalence not measured | C |
| Forced entry at uncontrolled junctions | Two-wheelers, autos, young drivers | Waiting time; size of the gap; size of the conflicting vehicle | Aggressive clearing behaviour modelled at Tirupati junctions ([Dutta et al. 2022](https://link.springer.com/article/10.1007/s40030-022-00634-4)) | B |
| Late merge / cut-in at a lane drop | All, more by aggressive drivers | Queue length difference between streams | n-person Prisoner's Dilemma in the metastable phase ([Nakata et al. 2010](https://econpapers.repec.org/RePEc:eee:phsmap:v:389:y:2010:i:23:p:5353-5361)), from lane-based models | B |
| Box-blocking | All | Green with no exit space downstream | No Indian measurement found | C |
| Opposite-carriageway encroachment at signals and jams | Two-wheelers, cars on undivided roads | Own queue length; oncoming gap | No study found | C |
| Stopping anywhere / improper parking | E-rickshaws, autos, buses | Passenger hail; metro exits; markets | Largest e-rickshaw challan category in Delhi (Section 3.2) | B |
| Phone use while moving | Delivery riders | Order alerts | Raises reaction time and lateral wander | B |

### 4.3 Imitation: "copying others to get ahead"

Social influence works in *both* directions, and the model needs both terms:

- **Violators recruit violators.** Pedestrians are more likely to cross on red when others are already crossing, and the probability grows with the number crossing. French pedestrians were about twice as influenced by others as Japanese pedestrians ([Pelé et al. 2017](https://www.ncbi.nlm.nih.gov/pmc/articles/PMC5367276/)). Make the imitation coefficient βₛ a zone- or culture-level parameter, not a constant.
- **Waiting crowds deter.** Among 1,381 cyclists observed in Bologna, 62.9% ran the red. With more cyclists waiting, risk-taking violations fell, but "opportunistic" ones (waiting, then going early) stayed high. Followers tended to make opportunistic violations ([Fraboni et al. 2016](https://www.frontiersin.org/articles/10.3389/fpsyg.2016.01834)).
- **Disorder spreads across norms.** Seeing one rule broken raises the breaking of other rules ([Keizer et al. 2008](https://pubmed.ncbi.nlm.nih.gov/19023045/)). A US replication largely failed, so keep this as a sensitivity case rather than a default.
- **Indian pedestrians:** 44.6% of those arriving on red at 12 Indian signalised intersections crossed against the signal, mostly in the second half of the red phase ([Kumar & Ghosh 2022](https://bvbvf1.bib-bvb.de/vufind/PrimoRecord/cdi_proquest_journals_2638772627)).

Implement imitation as two counts in the agent's field of view: violators moving (adds to βₛ) and compliers waiting (subtracts). With these two terms, the model can show tipping: once a few riders go early, the rest follow.

### 4.4 Behaviour that looks like violation but is not

Gap filling, staggered following and moving between notional lanes are the normal mode of Indian traffic (Section 2), not defections. Keep them in the baseline physics. If they are coded as violations, every scenario will show "defection", and the dilemma analysis becomes meaningless.

## 5. Area-level compliance (Shahdara versus Lutyens' Delhi)

Build each zone's compliance from causes you can observe (enforcement, road design, land use, vehicle mix, starting norms), not from a single "discipline" dial. Then run the experiment that answers your question directly: put the *same* agent population into a Lutyens-type zone and a Shahdara-type zone, and measure how much of the difference the zone produces on its own.

### 5.1 Evidence that compliance varies by place

- **Helmet use, Bengaluru:** 87.6% of riders at inner-city junctions versus 74.9% on the Outer Ring Road ([IJPHRD 2019](https://mail.i-scholar.in/index.php/ijphrd/article/view/192073)).
- **Helmet use, Tamil Nadu:** Chennai traffic police report about 90% rider compliance, while seven other districts had notably low adult compliance ([CAG 2025](https://cag.org.in/sites/default/files/database/Helmet%20Compliance%20Report%20with%20data.%20FINAL.pdf.pdf)). The gap tracks enforcement intensity.
- **Mumbai red-light running** depends on whether police are present ([TRR 2017](https://journals.sagepub.com/doi/10.3141/2619-08)).
- **Built environment, Beijing:** commercial activity and building density within 150 m predicted the violation types of cyclists and pedestrians (grade B for Delhi).

**Do not calibrate zones from challan counts.** In Delhi in 2024, New Delhi district recorded the most red-light challans (17,741) and the northeast district the fewest (1,140) ([Pioneer, Sept 2024](https://www.dailypioneer.com/2024/state-editions/traffic-cops-issued-over-87-000--challans-this-year-till-september-15.html)). That almost certainly reflects where cameras and officers are, not where drivers run more reds. In Ahmedabad, the top five locations produced about 25% of 3.57 million e-challans, and red-light challans followed the camera network ([Precog, IIIT Delhi](https://arxiv.org/pdf/1909.08106)).

### 5.2 Zone parameters

| Parameter | What it controls | How to estimate it | Evidence |
| --- | --- | --- | --- |
| Detection probability | βₑ term in the violation rule | Officers and cameras per junction or km; share of junctions with ITMS cameras | Grade C locally; map Delhi camera locations |
| Effective penalty | Fine × chance it is actually paid | Fine schedule × payment rate | Ahmedabad: about ₹216M paid of ₹808M owed, 153 days average to pay, about 74% re-offended (Precog) |
| Enforcement halo | How far and how long a police post deters | Exponential decay with distance and time | Speed effect of a marked police car halved every 900 m and faded within 3 days after a one-day operation ([Hauer et al. 1982](https://trid.trb.org/View/187827)). Physical policing has a minimum distance halo of about 2.4 km versus 500 m for cameras ([EU review](https://road-safety.transport.ec.europa.eu/eu-road-safety-policy/priorities/safe-road-use/safe-speed/archive/speed-enforcement/additional-considerations-speed-enforcement/time-and-distance-halo-effects_en)) |
| Median continuity and opening spacing | Detour length, so wrong-way temptation | From road geometry (OpenStreetMap plus a site check) | Hyderabad police cite long dividers as the riders' stated reason ([Deccan Chronicle](https://www.pressreader.com/india/deccan-chronicle/20200314/281681141946185)) |
| Footpath accessibility | Whether footpath riding is possible | Bollards present or absent; kerb height | Grade C |
| Kerbside control | Parking and stopping friction | Share of kerb occupied | Section 6 |
| Land-use intensity | Pedestrian volume, hail-and-stop events | Markets, metro exits, schools per km | Grade B |
| Persona mix | Share of e-rickshaws, delivery riders, cycles | Classified counts at sample points | Grade C until measured |
| Starting violation share | Seeds the imitation term (Section 4.3) | Observed share of red-light runners at a sample junction | Grade C |

### 5.3 Two illustrative zone profiles

These are hypotheses based on your observation and on how the parameters above typically differ. Verify both by observing sample junctions.

| Parameter | Lutyens-type zone | Shahdara-type zone |
| --- | --- | --- |
| Detection probability | High: dense police presence, cameras | Low |
| Road width and medians | Wide roads, continuous medians, roundabouts | Narrow, often undivided; informal median gaps |
| Footpaths | Wide, mostly clear | Narrow, encroached or missing |
| Kerbside friction | Low | High: vendors, e-rickshaw stands, double parking |
| Persona mix | Few e-rickshaws or cycles; more cars | Many e-rickshaws, two-wheelers, cycles, pedestrians |
| Starting violation share | Low | High |

**The test that matters:** run (a) Shahdara agents in a Lutyens zone, and (b) Lutyens agents in a Shahdara zone. If behaviour converges to the zone, then infrastructure and enforcement, not "local character", drive the difference. That would be the evidence-based answer to the culture-versus-conditions question, and the model can produce it.

## 6. Road and infrastructure situations

This section covers the "bad roads" half of your thesis. The design rule matters more than any single number. **Every road defect should act through the same mechanisms the agents already use.** A defect can do four things: take away usable width, impose a local speed profile, change the supply of gaps, or change the incentive to violate. Avoid global capacity multipliers such as "potholes = −30% capacity". If you hard-code those, the simulator cannot separate roads from behaviour, and the factorial decomposition in your review (roads × behaviour) becomes circular. Use the published capacity and speed losses below as **validation targets**: your mechanisms should reproduce them without being told to.

A second rule: defects should be **objects with positions**, not section attributes. A pothole 40 m before a bus stop on the kerb side behaves very differently from the same pothole mid-carriageway, because of who has to move around it and where they go.

### 6.1 Potholes and rough surfaces

**What the evidence says**

- **Responses differ by class, which matters for your simulator.** A study of pavement distress in Bandung found that [motorcycles tend to swerve or change lanes, heavy vehicles mostly slow down, and cars do both](https://ouci.dntb.gov.ua/works/4V2r0oY4). In mixed traffic, a pothole therefore creates lateral turbulence (two-wheelers cutting across), not just a slow zone. The same study warns that vehicles swerving into the opposing lane is the dangerous case, and that rain-filled potholes are worse still.
- **Lateral placement.** On deformed sections in Izmir, [Aydin & Topal (2016)](https://journals.vilniustech.lt/index.php/Transport/article/view/1455) found that lateral positions stop being normally distributed. Wider, shallower deformations affect lane use less. Driver perception and aggressiveness were the strongest influences on longitudinal behaviour. That last point is useful: your persona κ should modulate pothole response.
- **Magnitude, used as a validation target.** Aydin & Topal summarise Ben-Edigbe's measurements: [about 30% capacity loss from surface deformations, with speed reductions of up to 50%](https://journals.vilniustech.lt/index.php/Transport/article/download/1455/1157). On a potholed Indonesian road, [measured free-flow speed was 24.23 km/h against 38.05 km/h on the unpotholed section](https://etd.repository.ugm.ac.id/penelitian/detail/202541). That is a 36% loss, with similar traffic.
- **India.** [Chandra (2004)](https://ascelibrary.com/doi/10.1061/%28ASCE%290733-947X%282004%29130%3A3%28360%29) showed that roughness lowers capacity on Indian two-lane roads. As summarised by [Abdel-Wahed & Hashim](https://iccae.journals.ekb.eg/article_44383_7417b8f5ce7d29ddc1f783067e3b9342.pdf), free-flow speed falls with roughness, and cars lose more speed than heavy vehicles. Use the class ordering (car > heavy) as a calibration check.
- **Safety.** MoRTH data tabled in Parliament put pothole deaths at [9,438 for 2020–2024, rising from 1,555 to 2,385 a year](https://www.devdiscourse.com/article/business/3804410-pothole-peril-alarming-spike-in-road-accident-fatalities). This is a floor, not an estimate. A crash is [only counted as pothole-related if the FIR names the pothole](https://carkhabri.com/carblogs/9400-die-in-india-due-to-pothole-accidents), and several states report zero.

**Parameters**

| Parameter | Suggested form | Grade |
| --- | --- | --- |
| Pothole density (per km per lane-metre) | Poisson, rate by road class and zone; higher on kerb side | C (survey your own corridors) |
| Pothole size and depth | Lognormal diameter (0.3–1.5 m), depth 3–15 cm | C |
| Detection distance | Falls with speed, darkness, rain (water-filled) and leader occlusion | C |
| Response choice | Multinomial by class: swerve / slow / straddle / hit. Two-wheeler mostly swerves; heavy mostly slows | B (Bandung) |
| Crossing speed if hit | Class-specific cap (two-wheeler lowest) | C |
| Lateral avoidance offset | Must clear the hole by half a tyre width at least; bounded by lateral clearance model (§2) | B |
| Roughness (IRI) of section | Lowers desired speed, car more than heavy vehicle | A/B (Chandra 2004) |

The swerve response is what links potholes to the behaviour side of the model. A two-wheeler that swerves round a kerb-side pothole enters the gap a car was about to take. Swerving should therefore go through the same lateral gap-acceptance logic as voluntary filtering, so a pothole field produces exactly the kind of jam your review describes.

### 6.2 Kerbside parking, stopped vehicles and vendors (side friction)

Model a parked vehicle as a static obstacle that cuts effective width. Model parking and unparking as dynamic events: a vehicle blocks space for a manoeuvre time, and drivers cruising for a slot slow down. Effective width at position x is then:

```latex
W_{eff}(x) = W - \sum_i w_i(x) - c_{shy}
```

Here w\_i are the widths of obstacles at x, and c\_shy is the shy distance kept from them. Your lateral-clearance model (§2) supplies c\_shy; you do not need a separate number.

**Validation targets (Indian):**

- **Varanasi.** Capacity loss tracked effective-width loss roughly one for one: [6.42% at a 7.00% width reduction and 35.09% at 36.02%](https://etasr.com/index.php/ETASR/article/view/5897). Other sections gave [34.10% at 29.31% and 40.54% at 40.98%](https://distantreader.org/stacks/journals/etasr/etasr-5897.pdf). Earlier work in the same series ([Srivastava & Kumar, 2023](https://doi.org/10.48084/etasr.5603)) found that a 21.8% width loss cut capacity by 26%. If your effective-width mechanism reproduces this near-linear curve on its own, that is a strong check of the lateral model.
- **Undivided streets.** With parking, pedestrians and non-motorised vehicles together, [capacity was up to 60.73% below a base section of the same width](https://doaj.org/article/135bc20a47b94cc29e82d3c6635eeabb) (Biswas, Chandra & Ghosh, 2021).
- **Speed.** Across studies reviewed in the Varanasi paper, on-street parking [cut stream speed by 15–44% (5.1–21 km/h)](https://distantreader.org/stacks/journals/etasr/etasr-5897.pdf). Pradhan & Dey found in Bhubaneswar that [parking manoeuvres reduce speed, by different amounts for each vehicle type and more at higher flows](https://www.springerprofessional.de/en/effect-of-on-street-parking-maneuver-on-traffic-speed/51642930).

**Warning about regression results.** These side-friction regressions are statistically fragile. The predictors (parking, bus stops, pedestrians) are [strongly correlated with each other](https://typeset.io/pdf/dealing-with-multicollinearity-problem-in-analysis-of-side-4iktjzbnop.pdf), so individual coefficients are unstable even when R² looks good. Treat them as aggregate targets, not per-element inputs.

**Parameters**

| Parameter | Suggested form | Grade |
| --- | --- | --- |
| Kerb occupancy (share of kerb length parked) | 0–100%, by land use and time of day; key zone factor (§5) | A (measure from video or imagery) |
| Double-parking probability | Conditional on kerb occupancy and enforcement detection probability | C |
| Parked-vehicle offset from kerb | Distribution; cars rarely tight to the kerb | C |
| Manoeuvre rate | Events per 100 m per hour, by land use | B |
| Manoeuvre duration | Lognormal; parallel parking in a stream takes long | B |
| Cruising-for-parking share | Share of vehicles slowing near kerbs | C |
| Vendors and encroachments | Static polygons; footpath encroachment pushes pedestrians onto the carriageway (§6.4) | A (map them) |
| "Stop anywhere" events (autos, e-rickshaws, cabs) | Poisson by persona; dwell lognormal; link to §3 persona | B (challan shares) / C (rates) |

### 6.3 Bus stops and buses stopping in the stream

Kerbside bus stops are the cleanest Indian-measured side-friction element, so use them to calibrate early.

- **Capacity drop.** On four- and six-lane divided roads in Warangal and Hyderabad, [Pallela & Mehar (2024)](https://doaj.org/article/83120e38c2774ddd9f4b28d69772a972) found that dwell time, roadway width lost and the presence of an extra lane all drive the capacity drop, as well as bus frequency. Indo-HCM 2017, which uses bus frequency alone, gave noticeably higher reductions than their model. One Warangal four-lane section showed a [7.92% capacity reduction](https://katalog.lib.cas.cz/KNAV/EdsRecord/edo,ejs60270790).
- **Speed.** Mean vehicle speed fell [by up to 22% at kerbside stops](https://springerlink.bibliotecabuap.elogim.com/content/pdf/10.1007/s40030-023-00720-1.pdf). The size of the affected zone grows with the disturbance.
- **Where buses actually stop.** Delhi buses often do not stop at the platform. Analysis of two New Delhi arterials found [passengers waiting on the roadway, stops without enough bus capacity, and auto-rickshaws standing in the kerb lane](https://link.springer.com/doi/10.1007/s40890-017-0033-1). This is a multi-agent loop your simulator can reproduce: autos wait where passengers are, passengers stand where buses stop, so buses stop further out, so the effective width loss grows.
- **Simulation precedent.** [Koshy & Arasan (2005)](https://data.zhenzhi.com/details/1/872154936) is the classic Indian heterogeneous-traffic simulation of kerbside stops versus bus bays.

**Parameters:** bus frequency (A); dwell time (lognormal, A); stopping position, both longitudinal offset from the platform and lateral offset from the kerb (B); auto and e-rickshaw clustering near stops (C); width taken by passengers waiting on the carriageway (C).

### 6.4 Pedestrians crossing anywhere

Pedestrians at undesignated points force gaps in the stream, so model them as agents with their own gap acceptance, not as fixed delays.

- **Threshold.** On six-lane arterials in three Indian cities, [Dhamaniya & Chandra (2014)](https://journals.sagepub.com/doi/abs/10.3141/2461-17) found practically no capacity effect below about 200 pedestrians per hour. That threshold is a good validation target. The same study gives a [base capacity of 1,500–2,100 PCU/h/lane](https://journals.sagepub.com/doi/ref/10.3141/2461-17), with the spread put down to differences in free-flow speed.
- **Upper range.** On six-lane midblocks in Hyderabad, [capacity reductions ran from 19.05% to 56.06%](https://trid.trb.org/View/2671543), worst where crossings and a bus stop coincided. One simulation study found [stream speed cut by 58.33% by crossing pedestrians](https://aminer.org/pub/636ddaa590e50fcafdbbfd0f/analysis-of-the-impact-of-pedestrian-crossing-activity-on-the-traffic-characteristics).
- Red-light violation by pedestrians (44.6% in Kumar & Ghosh) is covered in §4.

**Parameters:** crossing flow (pedestrians per hour per 100 m, A); accepted vehicle gap (distribution, B); rolling crossing, lane by lane, with waiting in the middle of the carriageway (B); group size, since groups force wider gaps (C); share walking on the carriageway because the footpath is blocked, which is driven by the encroachments in §6.2 and the footpath riding in §4 (C).

### 6.5 Animals

Animals are a real Delhi-NCR feature, but almost no flow measurement exists, so treat them as grade C scenario switches.

- Gurugram Traffic Police link stray cattle to [about 30% of the city's accidents](https://www.tribuneindia.com/news/delhi/stray-cattle-take-over-gurugram-roads-residents-fear-for-safety/amp). The municipal estimate rose from about 8,000 cows (2022 survey) to about 11,000. Treat this as a police claim, not a measured share.
- At an eastern-India trauma centre, [dogs caused 69% of stray-animal crashes and cattle 21%](https://doaj.org/article/dba2617454984732985acc591ba0c32a). Many injuries come from losing balance while avoiding the animal, and [two-wheelers are most at risk](https://pmc.ncbi.nlm.nih.gov/articles/PMC8054802).

**Model:** cattle as slow or static obstacles that like medians and verges (an obstacle polygon with a dwell time). Dogs as sudden crossing events that trigger hard braking or a swerve (a Poisson rate, higher at night). Both run through the same swerve logic as potholes.

### 6.6 Signals: working, blinking, dark and police-controlled

Give every signalised junction a **control state** that can change during a run:

| State | Behaviour to model | Delhi evidence |
| --- | --- | --- |
| Normal fixed-time | Your violation rules (§4) apply | — |
| Countdown timer working or broken | Shifts red-light running between start and end of red (§4) | The CAG found [40% of countdown timers not working](https://www.magzter.com/en/stories/newspaper/The-Morning-Standard/CAG-40-COUNTDOWN-TIMERS-POOR) (2025) |
| Blinker (flashing amber) | Becomes an uncontrolled junction | Delhi Police put low-flow junctions on [blinker mode from 11 PM to 7 AM](https://eparlib.nic.in/bitstream/123456789/692250/1/4405.pdf) (864 signals in the NCT at the time of that answer) |
| Dark (failed) | Uncontrolled junction, with drivers unsure who should yield | Old signals [fail when rain gets into the cables; officials put rain-time failures at 7–8%](https://www.business-standard.com/article/news-ians/why-delhi-s-traffic-lights-go-on-the-blink-113090100394_1.html) (2013 claim) |
| Police manual control | Long cycles chosen by the officer; partial compliance | Delhi Police SOP: [manual regulation at important junctions when power fails](https://eparlib.nic.in/bitstream/123456789/692250/1/4405.pdf) |

**Behaviour at a dark or blinking junction.** Indian studies of uncontrolled junctions describe the regime your model should switch into:

- Minor-street drivers [do not yield to higher-priority movements](https://link.springer.com/article/10.1007/s40534-017-0151-9). Dutta & Ahmed (2018) model aggressive and non-aggressive drivers separately, with forced entry.
- Occupation time in the conflict area is [higher at an uncontrolled junction than a semi-controlled one](https://idr.nitk.ac.in/jspui/handle/123456789/9953?mode=full) (Asaithambi & Anuroop, 2016).
- Stop- and yield-controlled junctions in India [operate much like uncontrolled ones](https://sjsutst.polsl.pl/archives/2025/vol126/097_SJSUTST126_2025_Kota_Pallela_Lalam.pdf), so a sign does not restore priority.

Outside India: when a Dhaka AI signal failed for four and a half hours, [many motorists ignored the officers and tried to cross from several directions at once](https://www.dhakatribune.com/bangladesh/dhaka/414437/ai-traffic-signal-out-of-service-for-4.5hrs-at). During a one-hour outage in central Athens, [vehicles kept flowing and pedestrians were the ones left waiting](https://www.nrso.ntua.gr/geyannis/wp-content/uploads/geyannis-pc689.pdf), often for over a minute.

**Police control is not simply better or worse.** In a VISSIM study of four Sri Lankan junctions, [manual police control gave lower delays than the posted (outdated) signal plans when the junction was oversaturated, but re-optimised signal timings beat both, and minor-road movements did worse under police control](https://dl.lib.uom.lk/items/f0a993cb-9288-4f85-919d-d1f517cc6374). Model the officer as an adaptive controller with long cycles and a bias towards the major approach. Make compliance with the officer a parameter (it is lower when enforcement capacity is visibly overwhelmed).

**Why this matters for the game.** A working signal makes the junction a coordination game with an external referee. A dark signal turns it into repeated Chicken between approaches, which is exactly where your "negative-sum game played as zero-sum" claim can be tested directly. The experiment: identical demand and personas, with the control state switched from normal to dark. Measure throughput, delay and conflicts. Then vary the share of aggressive personas.

**Parameters:** outage probability per junction-day, with **rain as a correlated driver** (A/B from complaint data if you can get it); outage duration (lognormal); blinker schedule (A); officer arrival delay after an outage (C); officer cycle length and phase bias (C); compliance with the officer (C).

### 6.7 Splits, lane drops and being on the wrong side for your turn

Your example (the road splits in two and right-turners are bunched on the far left) is a **mandatory lane change with a deadline**. It is one of the most important things to get right, because late crossing movements create the voids and stoppages that turn a merely busy road into a jam.

**What the general literature gives you** (mostly freeway, little Indian work):

- **Triggers.** When a driver attempts a mandatory change depends on [distance to the diverge, number of lanes to cross, and the density difference between current and target lane](https://escholarship.org/content/qt2970816w/qt2970816w.pdf?t=lat7i0).
- **Timing.** In Worrall's early weaving data, [76% of merging and 74% of diverging drivers made their move in the first 300 ft of the weaving area](https://lirias.kuleuven.be/retrieve/722302). The concentration of lane changes is the main cause of speed drop there.
- **Why it costs capacity.** A vehicle mid-change [occupies two lanes for a few seconds](https://transp-or.epfl.ch/heart/2022/abstracts/183.pdf). Reported capacity drops at merges and weaving sections are [normally 3–20%](https://dx.doi.org/10.1016/j.trc.2018.09.014). The distribution of where lane changes happen matters a great deal.
- **Imitation again.** On UK motorway weaving sections, [10.5% of lane changes were pairs swapping lanes and 5.6% followed a preceding lane-changer](https://eprints.whiterose.ac.uk/155273/1/Kusuma%20et%20al%20-%20TransportmetricaB2020-WRR.pdf). This is the same follow-the-leader mechanism as in §4.

**Adapting it to lane-less traffic.** Replace "lanes to cross" with the **lateral displacement needed**, Δy, from the agent's current position to the branch it needs, at a deadline position x\_d. A simple urgency term is:

```latex
u = \frac{\Delta y}{\max(x_d - x,\ \epsilon)}
```

As u rises, the agent accepts smaller lateral gaps (§2), uses forced entry (§4), and finally either **stops and waits** in place, which blocks the stream, or **gives up** and takes the wrong branch.

**Parameters (mostly grade C; I found no Indian study of late crossing at splits, so this is a genuine gap you could fill with video):**

| Parameter | Suggested form | Notes |
| --- | --- | --- |
| Route-awareness distance | Mixture: most drivers know well upstream; a "late realiser" tail (unfamiliar, late navigation prompt, sign hidden behind a bus) | The tail is the key quantity; sweep it |
| Sign visibility distance | Per split, cut by occlusion from large vehicles | Map from site |
| Urgency threshold for forcing | Persona-specific via κ | Ties to §3 |
| Stop-and-wait probability at the deadline | By persona and class; cars more than two-wheelers | This creates the blockage |
| Give-up behaviour | Continue and U-turn at the next median opening; or reverse or go wrong-way (§4) | Links to the median-opening model below |
| Destination split at the diverge | From counts | A |

**Median openings and U-turns.** Missed turns often end at a median opening, so model these properly.

- [Mohapatra, Dey & Chandra (2016)](https://idr.iitbbs.ac.in/items/3dbdccf4-9832-4cd9-bb7c-d30ab9fd88bb/full) measured the path of U-turning vehicles at 21 Indian median openings. It depends on vehicle size and road width. On six-lane roads it can be bimodal, depending on the two-wheeler share; on four-lane roads a gamma distribution fits.
- On four-lane roads [almost all opposing traffic becomes conflicting traffic](https://scite.ai/authors/smruti-sourava-mohapatra-aykwJ1); on six-lane roads much less does.
- Indian critical gaps for U-turns are [smaller than those reported in developed countries](https://journals.sagepub.com/doi/10.1177/03611981211035761). Aggressive minor-stream drivers often [force the major stream to slow down and make the gap](https://katalog.lib.cas.cz/KNAV/EdsRecord/edb,136150370).
- Several vehicles often [merge together on a single gap](https://journals.vilniustech.lt/index.php/Transport/article/view/193) (single versus multiple entry).

### 6.8 Speed breakers

- **Magnitude.** Indian measurements of humps by geometry show approach speeds of roughly 33–44 km/h falling to about 14–23 km/h on the hump. That is a [reduction of about 30–64% depending on vehicle class and hump shape](https://www.wctrs-society.com/wp-content/uploads/abstracts/mumbai/WCTR2019_Mumbai_435.pdf). Hump geometry explained more than 70% of the variation in speed reduction.
- **Delay and fuel.** One study measured [10–13.44 s of delay per hump](https://www.wctrs-society.com/wp-content/uploads/abstracts/mumbai/WCTR2019_Mumbai_440.pdf). Another found a test car used [68% more fuel at 1.66 humps per km](https://www.jstage.jst.go.jp/article/easts/13/0/13_2073/_article/-char/en) than on a free stretch.
- **Class conflict.** The Indian Roads Congress guideline notes that [a breaker designed for trucks can be dangerous to motorcyclists, and one designed for motorcyclists does nothing to trucks](https://law.resource.org/pub/in/bis/irc/irc.gov.in.099.1988.pdf).

**Model:** a local speed cap by class and geometry (height, width), with deceleration and acceleration from §1. Add a two-wheeler "edge bypass" option where the hump does not reach the kerb (grade C). Bypassing pulls two-wheelers to the edges and adds lateral conflict.

### 6.9 Gradients: flyover and underpass ramps

Chandra & Goyal (2001), as summarised by [Arasan & Arkatkar (2010)](https://jau.vgtu.lt/index.php/Transport/article/download/5758/5002), give free-speed loss per 1% of upgrade:

| Class | km/h lost per 1% upgrade |
| --- | --- |
| Car | 2.96 |
| Motorised two-wheeler | 2.50 |
| Bus | 2.43 |
| Tractor-trailer | 2.06 |
| LCV | 1.26 |
| Bicycle | 0.51 |
| Cycle-rickshaw | 0.36 |

These come from intercity two-lane roads, so they are grade B for Delhi flyover ramps. E-rickshaws are missing. Derive their climbing speed from the legal power cap (≤2 kW) and a loaded mass, and treat it as grade C. A loaded e-rickshaw crawling up a flyover ramp is a moving bottleneck that the rest of the stream will try to pass on both sides.

### 6.10 Rain and waterlogging

**Rain lowers capacity and speed.**

- One urban study found capacity falling by [7.26%, 10.87% and 17.09% under light, medium and heavy rain, with free-flow speed down 3.07%, 5.29% and 6.64%](https://unpaywall.org/10.1007%2Fs13177-018-0162-x).
- Hong Kong urban-road data ([Lam et al.](https://ascelibrary.com/doi/10.1061/%28ASCE%29TE.1943-5436.0000544)) show reductions growing with rainfall intensity, to roughly 22% capacity and 10% free-flow speed at the heaviest levels.
- Two-lane highways in Malaysia showed [capacity drops of 2–32%](https://0-doi-org.brum.beds.ac.uk/10.11113/jt.v70.3490).
- At network level, using the macroscopic fundamental diagram, rain cut weighted free-flow speed by [9.7% and jam density by 12.3% on average](https://katalog.lib.cas.cz/KNAV/EdsRecord/edb,94524430).

**Waterlogging is local and far worse.** It closes part of the carriageway and creates a depth-dependent speed cap. The Mumbai flood study used a [depth–speed relationship from Pregnolato et al. (2017)](https://www.wctrs-society.com/wp-content/uploads/abstracts/mumbai/WCTR2019_Mumbai_516.pdf) to restrict links. A DA-IICT thesis built [visibility and water-depth effects into SUMO's Krauss car-following model](https://ir.daiict.ac.in/items/0f967eb3-8fa5-4e66-8d89-5b5de6b1987b), configurable per segment. That is close to what you want.

**Model rain as a scenario that moves several parameters together.** In Delhi a heavy shower at once:

- lowers desired speeds and visibility;
- hides potholes under water (later detection, §6.1);
- knocks out a share of signals (§6.6);
- floods specific low points, which you can map from Delhi Traffic Police advisories;
- changes demand and mode (two-wheeler riders sheltering under flyovers is a common sight, grade C).

If you vary these one at a time you will underestimate monsoon gridlock, because the losses compound.

### 6.11 Construction zones, barricades and diversions

Metro and drain works create irregular lane drops with poor signage, often next to signals that are not working. One Okhla diversion produced [hours of jams from non-functional signals plus no traffic police](https://www.millenniumpost.in/cramped-and-cluttered-212473). Model a work zone as an obstacle polygon plus a lane-drop deadline (§6.7) plus a possible signal outage (§6.6). Use the same objects; no new behaviour is needed.

### 6.12 Summary: which road situations to build first

| Priority | Situation | Why first | Calibrate against |
| --- | --- | --- | --- |
| 1 | Kerbside parking and stopping (§6.2–6.3) | Best Indian data; near-linear width–capacity curve to reproduce | Varanasi, Warangal and Hyderabad curves |
| 2 | Splits and mandatory lane changes (§6.7) | Your own example; biggest behavioural lever on jams | Your own video (gap in literature) |
| 3 | Signal states (§6.6) | Directly tests the game-theory claim | Before and after an outage, if you can film one |
| 4 | Potholes (§6.1) | Core of the "bad roads" thesis | Speed loss of about 36% on potholed sections; class ordering |
| 5 | Pedestrians (§6.4) | Threshold of about 200 per hour is a clean check | Dhamaniya & Chandra 2014 |
| 6 | Rain bundle (§6.10) | Compound scenario; build last because it reuses everything | Rain capacity curves |
| 7 | Humps, gradients, animals, work zones | Local effects; quick to add once the objects exist | Hump speed profiles; grade table |

## 7. Demand, traffic composition and environment

The sections above describe how agents behave. This one describes **who is on the road, how many, when and under what conditions**. Many simulation papers get the behaviour roughly right and the inputs badly wrong, then blame the behaviour model.

### 7.1 Composition: registrations are not the traffic stream

The most common mistake is to take vehicle shares from the registration database.

- **Registered fleet.** Delhi had [87.61 lakh registered vehicles in 2025–26, of which 67.65% were two-wheelers](https://www.millenniumpost.in/delhi/delhi-vehicle-count-up-79-to-876-lakh-653097), and [522 vehicles per 1,000 people](https://dailypioneer.com/news/delhis-vehicle-count-hits-87-6-lakh-8-rise-in-2025-26).
- **What is actually on an arterial.** On Delhi midblock sections, [Dhamaniya & Chandra (2017)](https://link.springer.com/content/pdf/10.1007/s40999-017-0206-7.pdf) counted only **21–33% two-wheelers**. Small cars made up 38–58%, three-wheelers 8–21%, big cars 5–11% and heavy vehicles 2–4%. Jaipur, by contrast, had 52% two-wheelers.

There are several reasons for the gap. Registrations include dead and out-of-town vehicles, and the Economic Survey itself notes that [many Delhi-registered vehicles run in the NCR and vice versa](https://www.thestatesman.com/india/every-2nd-person-delhi-owns-vehicle-says-economic-survey-1502605348.html). Two-wheelers make shorter trips, and arterials draw longer car trips. **Composition is a property of a road and an hour, not of the city.** Count it from video at your study sites. It is the cheapest grade A input you can get.

**Composition is also moving fast, so make it a scenario variable:**

- E-rickshaws were [42.6% of Delhi EV registrations in 2025, up from 26.4% in 2023](https://thepatriot.in/?p=80704).
- Then e-rickshaw registrations [fell 93% in January–May 2026](https://www.inkl.com/news/electric-two-wheelers-private-ev-cars-record-highest-registrations-since-2019-in-delhi) as policy changed.
- The draft Delhi EV policy proposes [ending new petrol and diesel three-wheeler registrations from January 2027 and two-wheelers from April 2028](https://intelligence.crisil.com/en/homepage/what-we-think/all-our-thinking/reports/2026/04/driving-past-the-ice-age.html).

Electric two-wheelers accelerate differently from petrol ones. A fleet shift therefore changes the kinematics in §1, not just the labels. That makes "what happens to filtering and red-light running when the two-wheeler fleet goes electric" a cheap, policy-relevant experiment.

### 7.2 Densities to validate against

[Kanagaraj & Treiber (2018)](https://arxiv.org/pdf/1805.05076) give observed partial densities from a Chennai midblock, which are a useful sanity check for your generator:

| Class | Medium traffic (veh/km) | Congested (veh/km) |
| --- | --- | --- |
| Motorcycles | 170 | 450 |
| Cars | 55 | 225 |
| Buses | 10 | 35 |
| Auto-rickshaws | 15 | 80 |

Their calibrated model reproduced the observed flow characteristics [to within 20%](https://arxiv.org/pdf/1805.05076). That is a realistic accuracy target for your first validation, not a disappointing one.

**Capacity is tied to speed.** Across 12 Indian midblock sections, lane capacity [ranged from 1,482 to 2,105 PCU/h](https://link.springer.com/doi/10.1007/s40999-017-0206-7). It was strongly related to operating speed through a second-degree polynomial, and the spread was explained by city size and driving behaviour. This is important for your thesis. Anything that lowers operating speed (potholes, friction, aggressive weaving) lowers capacity through the same curve, so you can check whether your simulated capacity moves along it.

### 7.3 Arrivals: how vehicles enter the simulation

- For urban mixed traffic dominated by two-wheelers, [Arasan & Koshy found headways fit a negative exponential distribution over a wide range of flows](https://publications.iitm.ac.in/publication/headway-distribution-of-heterogeneous-traffic-on-urban-arterials). In other words, arrivals are close to Poisson.
- That does not hold everywhere. On Indian two-lane highways, [Roy & Saha (2017)](https://link.springer.com/doi/10.1007/s12544-017-0276-2) found the exponential inadequate once vehicle-following becomes frequent. Elsewhere, [shifted exponential and gamma fit better at medium flows (400–1,200 veh/h)](https://link.springer.com/doi/10.1007/s12544-017-0276-2).

**Recommendation.** Generate arrivals per class with a shifted exponential: a minimum headway by class plus a random part. Start from negative exponential, and refit to your own counts. If your study section is downstream of a signal, do not use any renewal process at all. Feed it from a simulated upstream signal, because platoons released by green are the dominant structure, and they interact with §4 (red-light running at the tail of the platoon).

**Entry position matters too.** Vehicles should enter at a lateral position drawn from the observed class-specific distribution, not uniformly. [Balakrishnan & Sivanandan (2015)](https://repository.tudelft.nl/record/uuid:8a7f8349-49c3-48c3-af8b-07f81a480252) found that both lane and vehicle sub-class affect free-flow speed on Chennai roads. Vehicles in the median-side lane ran about 4.35–5.78 km/h faster.

### 7.4 Time of day and day of week

Delhi's arterials barely have an off-peak, which changes how you should design runs.

- In Google Maps data for 13 arterials, [peak and off-peak speeds were 26 and 27 km/h](https://www.cseindia.org/congestion-on-delhi-roads-has-worsened-6994). For about 75% of the 8 am to 8 pm window, speeds were between 25 and 30 km/h, and weekends were worse than weekdays (CSE, 2017).
- Lutyens' Delhi was the exception, with [44 km/h at peak and 52 km/h off-peak](https://scroll.in/latest/843382/scroll_in) and clearly defined peaks.
- A later CSE analysis (around Diwali 2020) found a [mean of 26 km/h, 23 km/h at peak and 32 km/h off-peak](https://www.cseindia.org/source-contribution-and-congestion-impacts.pdf), with the main build-up from 6 pm to 8 pm.

**Two cautions.** First, these are Google-derived corridor averages: good validation targets for an aggregate speed profile, not for parameters. Second, the Lutyens' comparison is confounded. Its arterials are [up to 50 m wide](https://www.pressreader.com/india/hindustan-times-delhi/20170711/281633895275606) and carry different traffic, so a speed gap there is not evidence of better behaviour by itself. This is exactly why §5 recommends the population-swap experiment: put Shahdara-type personas on a Lutyens-type geometry and the reverse.

**Parameters:** a demand profile by 15-minute interval (A, from counts); a composition profile by interval (A), since two-wheeler and auto shares change across the day; heavy-vehicle entry windows (A, from Delhi Traffic Police notifications); and day-to-day demand variation (B; sweep ±10–15% to test robustness).

### 7.5 Turning movements and origin–destination

- **Turning shares** at each junction: A, from counts. These matter more than they look, because they set the share of agents with a mandatory lateral move before each junction (§6.7).
- **Origin–destination and route choice:** keep demand and routes fixed at first. That isolates the behaviour you are testing. Add route choice later as a separate experiment, because selfish rerouting is itself a social dilemma (the price-of-anarchy literature in your review) and should not be mixed with lane-level dilemmas until each is understood.
- **Navigation-app use:** the share of drivers whose route reacts to live congestion (C). It also affects the "late realiser" tail in §6.7, because app prompts often come late at splits.

### 7.6 Night, lighting and visibility

Night changes several things at once: the persona mix (more trucks, fewer cautious drivers), the signal state (blinker mode, §6.6), enforcement presence, and visibility of potholes, animals and pedestrians.

- On Karnataka rural curves, [speeds were consistently lower at night, with cars slowing the most and two-wheelers keeping moderate speeds](https://idr.nitk.ac.in/handle/123456789/19999).
- In Swedish data on almost 60 million vehicle passes, [light conditions alone made no difference to speed, but rain or snow cut speed more on unlit roads](https://eprints.lib.hokudai.ac.jp/dspace/handle/2115/62388). That interaction is the useful part: lighting matters through hazard detection, not directly through desired speed.
- European studies also note that [night speeds can be higher because faster drivers make up more of the night population](https://eprints.lib.hokudai.ac.jp/dspace/bitstream/2115/62388/1/art%253A10.1186%252Fs40064-016-2124-6.pdf). This is a persona-mix effect, not a change in any individual's behaviour.

**Model night as a scenario bundle:** a shifted persona mix (C), blinker or dark signals (A, schedule), lower enforcement detection probability (C), and shorter hazard-detection distances on unlit links (B). Do not model it as a single speed multiplier.

### 7.7 Weather and air quality

Rain is covered in §6.10. Fog matters in Delhi winters, through visibility and therefore detection distance. Smog episodes trigger the Graded Response Action Plan, which changes composition overnight. At Stage III, for example, [BS-III petrol and BS-IV diesel four-wheelers are barred and non-essential diesel goods vehicles are curbed](https://www.autocarindia.com/car-news-amp/bs-iii-petrol-bs-iv-diesel-vehicles-banned-in-delhi-ncr-438428). At Stage IV, [trucks carrying non-essential goods are stopped at the border](https://www.spinny.com/blog/delhi-vehicle-entry-ban-from-november-1-bs4-cars-from-noida-gurugram-not-allowed/). Both fog and GRAP can be scenario switches that reuse existing parameters (detection distance, composition). You do not need new mechanisms.

### 7.8 Summary of demand-side inputs

| Input | Source | Grade | Notes |
| --- | --- | --- | --- |
| Composition by site and hour | Your video counts | A | Do not use registration shares |
| Flow by 15-minute interval | Your counts | A | Includes upstream signal platoons |
| Arrival headways | Fit to counts; start negative or shifted exponential | A/B | Per class |
| Entry lateral position | Video | A | Class-specific |
| Turning shares | Counts | A | Drives mandatory lane changes |
| Partial densities | Kanagaraj & Treiber as a check | B | Validation, not input |
| Composition scenarios (EV shift, e-rickshaw caps) | Policy documents | C | Experimental factor |
| Night and fog bundles | Combination of existing parameters | B/C | Scenario switches |
| Route choice | Fixed first; add later | — | Separate dilemma |

## 8. Which parameters matter most, and the order to tune them

You now have well over a hundred candidate parameters. You cannot calibrate them all, and the literature says you should not try.

### 8.1 What the calibration literature warns

- **Most parameters barely matter for a given output.** In a variance-based sensitivity analysis of the IDM on the NGSIM I-80 trajectories, [Punzo, Montanino & Ciuffo (2015)](https://www.iris.unina.it/handle/11588/587745) found that a three-parameter version (out of six) performed as well as the full model after calibration, and converged faster. They also found that **the leader's trajectory mattered more than any parameter**. In your terms: the traffic a driver is placed in drives outcomes more than the driver's own settings, which supports the population-swap logic in §5.
- **Different parameters govern different traffic states.** A sensitivity study of AIMSUN found [large differences in which parameters affect which traffic phases](https://bibliotek.vti.se/bib/272512), and concluded that parameters can be calibrated separately using data from different locations. That is the basis for the staged order in §8.3.
- **A large share of calibration error is noise inside one driver.** Kesting & Treiber found that [intra-driver variability, not differences between drivers, accounts for a large part of the calibration error](https://typeset.io/papers/trajectory-data-and-flow-characteristics-of-mixed-traffic-32z105vdjy). Do not over-fit personas to explain noise that is really within-trip variation (§3).
- **Many agent-based models cannot be identified at all.** Testing 9 estimation algorithms on 41 models, [Carrella (2021)](https://www.jasss.org/24/2/7.html) found that **about half of the agent-based models had at least one parameter that could not be recovered from data**. No algorithm won everywhere, and the best one changed even between parameters of the same model.
- **Bayesian methods tend to do better.** [Platt (2020)](https://www.inet.ox.ac.uk/publications/a-comparison-of-economic-agent-based-model-calibration-methods-1) found that a simple likelihood-based Bayesian approach consistently beat the more popular simulated-minimum-distance methods. Performance only degraded when fitting many parameters of a large model.
- **Heterogeneity costs identifiability.** In a contagion model calibrated with approximate Bayesian computation, [adding agent heterogeneity reduced the accuracy of parameter estimates](https://www.jasss.org/29/2/5/5.pdf). This is a direct warning for your persona system: every extra persona trait you calibrate makes the others harder to pin down.

**The consequence for your design.** Split parameters into three groups and treat each differently:

1. **Measured directly** (dimensions, kinematics, composition, geometry, signal timings). Fix them from data and never calibrate them.
2. **Calibrated** (following, lateral, gap acceptance, a few violation coefficients). Estimate them in stages against Indian data.
3. **Swept** (persona shares, imitation strength, enforcement perception, zone factors). Do not calibrate these. Treat them as experimental factors, and report results across their range.

This three-way split is also your defence against the reviewer question "how do you know these numbers are right?". You can say exactly which numbers are measured, which are fitted, and which are deliberately varied.

### 8.2 Indian calibration practice to borrow from

- **Non-lane-based traffic in VISSIM.** [Mathew & Radhakrishnan (2010)](https://data.zhenzhi.com/details/1/907895408) set out a method for representing non-lane-based behaviour and calibrating VISSIM at signalised junctions in two Indian cities. Composition and the static and dynamic characteristics of each vehicle class are part of the calibration itself.
- **Following behaviour from trajectories.** [Chaudhari, Srinivasan, Chilukuri & Treiber (2021)](https://www.bohrium.com/paper-details/calibrating-wiedemann-99-model-parameters-to-trajectory-data-of-mixed-vehicular-traffic/811845796193370112-4628) calibrated Wiedemann-99 to mixed-traffic trajectory data. [Kashyap et al. (2024)](https://ideas.repec.org/a/eee/phsmap/v646y2024ics0378437124003753.html) calibrated the longitudinal part of the high-speed social-force model on Chennai trajectories. Both are close to what you will be doing.
- **A review.** A [2023 survey of VISSIM calibration for heterogeneous traffic](https://researchgate.net/profile/Muhammad-Azam-49/publication/369037482_Calibration_methodologies_of_VISSIM-based_microsimulation_model_for_heterogeneous_traffic_conditions_-_a_survey/links/6560480d3fa26f66f421fe0f/Calibration-methodologies-of-VISSIM-based-microsimulation-model-for-heterogeneous-traffic-conditions-a-survey.pdf) collects the approaches and the parameters others chose to tune.
- **Data.** Public Indian trajectory data are scarce. Kanagaraj et al. noted that [trajectory data from cities in the developing world are not publicly available](https://typeset.io/papers/trajectory-data-and-flow-characteristics-of-mixed-traffic-32z105vdjy), which is why they built the Chennai set. Ask for it before filming your own.

### 8.3 Recommended tuning order

Calibrate from the most physical to the most social. Freeze each stage before moving on. Each stage uses data that the earlier stages do not depend on.

| Stage | What you fix or fit | Data | Check that must pass before the next stage |
| --- | --- | --- | --- |
| 0. Measured inputs | Dimensions, acceleration and braking, composition, geometry, signal plans, defect maps | Literature (§1) and your site survey | Nothing to fit: just audit the inputs |
| 1. Free flow | Desired-speed distributions by sub-class and lateral position | Early-morning video | Reproduce class speed means and spreads (Balakrishnan & Sivanandan values) |
| 2. Following | Time gap and standstill gap by leader–follower pair; reaction time | Trajectories (Chennai set or your own) | Headway distributions by pair; capacity on the speed–capacity curve (§7.2) |
| 3. Lateral | Clearance coefficients, lateral scales, filtering thresholds | Trajectories at medium density | Lateral position distributions by class; partial densities (§7.2) |
| 4. Gap acceptance | Critical gaps by class at junctions, median openings, merges | Junction video | Accepted and rejected gap distributions; forced-entry share |
| 5. Side friction | Shy distances and manoeuvre durations | Sections with parking and bus stops | Reproduce the width–capacity curve (§6.2) without tuning it in |
| 6. Violations | Baseline violation intercept per type; time-saving and waiting coefficients | Red-light and wrong-way counts at your sites | Observed violation rates by signal phase (countdown effects, §4) |
| 7. Social factors | Imitation strength, enforcement perception, persona shares | **Swept, not fitted** | Report outcomes across the whole range |

Two rules make this work.

- **Never re-open an earlier stage to fix a later mismatch.** If side friction doesn't reproduce the width–capacity curve, the fault is in stage 3 or 5, not in the desired speeds. Re-tuning free flow to hide it is how models become unfalsifiable.
- **Keep some data back.** For each stage, validate on a site or time period that was not used for fitting. The speed–capacity curve in §7.2 and the Lutyens'–arterial speed gap in §7.4 are good held-out checks for the whole model.

### 8.4 Sensitivity analysis, in two passes

1. **Screen with Morris elementary effects** over all calibrated and swept parameters. This is cheap (tens of runs per parameter) and finds the parameters that do nothing, so you can fix them.
2. **Run Sobol variance decomposition** on the survivors (typically 10–20). This gives first-order and total-effect indices, and the gap between them shows interactions. Interactions are what you care about: "roads × behaviour" is literally an interaction term.

If runs are expensive, fit a surrogate (a Gaussian process or a random forest) on a few hundred runs and compute indices on the surrogate. Ciuffo, Casas, Montanino, Perarnau & Punzo (2013, *Transportation Research Record* 2390) used Gaussian-process metamodels for exactly this with traffic simulators.

**Choose outputs before running the analysis.** Sensitivity depends on the output. Use the outputs that carry your argument:

- throughput at the bottleneck;
- mean and 95th-percentile delay;
- the social efficiency deficit (from your review);
- conflict counts (time-to-collision below 1.5 s);
- the share of time in gridlock.

A parameter can be unimportant for mean speed and decisive for gridlock onset. Gridlock onset is the result your thesis needs.

### 8.5 Pattern-oriented validation

Do not validate on one number. Pattern-oriented modelling (Grimm et al., 2005) asks the model to reproduce **several patterns at different scales at once**, because many wrong models can match one pattern but few can match five. For this simulator, a reasonable set is:

1. the speed distribution by class at free flow (§1, §7.3);
2. lateral position distributions by class (§2);
3. the speed–flow–capacity curve (§7.2);
4. the near-linear width–capacity loss from side friction (§6.2);
5. violation rates by signal phase, with and without a countdown timer (§4);
6. the about-200-pedestrians-per-hour threshold (§6.4);
7. the corridor speed profile across the day (§7.4).

If the calibrated model gets all seven roughly right without any being tuned in directly, you have a strong case that its counterfactuals (the population swap, the dark signal, the pothole repair) mean something.

## 9. Master parameter register

One table per group, with every parameter from Sections 1–7 in one place. "Role" uses the three-way split from §8.1: **Fix** (measured, never calibrated), **Fit** (calibrated in the stage shown) and **Sweep** (experimental factor). Copy this into your code as a config schema, so that every run records which value each parameter took.

### 9.1 Vehicles

| Parameter | Starting value or range | Distribution | Source (§) | Grade | Role |
| --- | --- | --- | --- | --- | --- |
| Two-wheeler length × width | 1.87 × 0.64 m | Normal, small spread | Dhamaniya & Chandra 2017; Agrawal et al. 2026 (§1) | A | Fix |
| Auto-rickshaw length × width | 3.20 × 1.40 m (Delhi) | Normal | §1 | A | Fix |
| Hatchback length × width | 3.72 × 1.44 m | Normal | §1 | A | Fix |
| All-car length × width | 4.46 ± 0.42 × 1.85 ± 0.12 m | Normal (use the SD) | §1 | A | Fix |
| SUV length × width | 4.58 × 1.77 m | Normal | §1 | A | Fix |
| Bus length × width | 10.10 × 2.43 m | Fixed by fleet | §1 | A | Fix |
| Heavy commercial length × width | 11.97 × 2.92 m (median) | Empirical | §1 | A | Fix |
| E-rickshaw envelope | ≤ 2.6 × 1.0 m; ≤ 25 km/h; ≤ 2 kW; 4 passengers | Legal cap; observed speed lower | CMVR (§1) | B | Fix |
| Mean discharge accel / decel | 2W 0.60/0.55; 3W 0.43/0.59; car 0.49/0.62; LCV 0.46/0.78 m/s² | Validation target, not an input | Ramireddy et al. 2021 (§1) | A | Check |
| Free-flow speed by sub-class | e.g. motorbike 49.2 ± 12.4; scooter 43.6 ± 10.4; moped 36.0 ± 8.0; hatchback 60.6 ± 15.5 km/h | Normal per sub-class | Balakrishnan & Sivanandan 2015 (§1, §7.3) | A | Fit (stage 1) |
| Median-lane speed bonus | +4.35 to +5.78 km/h | Additive by lateral band | §7.3 | A | Fit (stage 1) |
| Dynamic PCU | 2W 0.21–0.24; 3W 0.97–1.04; big car 1.53–1.66; HV 5.7–6.3; e-rickshaw 0.71–0.82; cycle 0.31–0.37 | Emergent | Dhamaniya & Chandra; Khan & Dass 2025 (§1) | A | Check |
| Upgrade speed loss per 1% | Car 2.96; 2W 2.50; bus 2.43; LCV 1.26 km/h | Linear | Chandra & Goyal 2001 (§6.9) | B | Fix |

### 9.2 Interaction

| Parameter | Starting value or range | Distribution | Source (§) | Grade | Role |
| --- | --- | --- | --- | --- | --- |
| Desired time gap T | 2W 0.3–0.49 s; auto 0.81–1.0 s; car 0.8–1.21 s; bus 1.0 s | Lognormal per class | Agrawal 2026; Kanagaraj & Treiber 2018 (§2.1) | A | Fit (stage 2) |
| Standstill gap s₀ | 2W 0.5–0.55 m; auto 0.83–2.0 m; car 1.32–2.0 m | Lognormal | §2.1 | A | Fit (stage 2) |
| Max accel a / comfortable decel b | 2W 2.0/2.0; car, bus, auto 1.0/1.0 m/s² | Normal, truncated | §2.1 | A | Fit (stage 2) |
| Emergency braking cap | 9 m/s² | Constant | §2.1 | B | Fix |
| Leader–follower gap multipliers | Matrix initialised to 1 | Per pair | Munigety 2018 (§2.1) | B | Fit (stage 2) |
| Lateral clearance vs speed | C \[cm\] = 0.615 v̄ + 120.9 | Plus pair-type slope changes (+35% car–bike; −25% car–auto) | Budhkar & Maurya 2017 (§2.2) | A | Fit (stage 3) |
| Lateral interaction scales | 0.15 m (braking), 0.30 m (steering) | Constant | Kanagaraj & Treiber (§2.2) | A | Fit (stage 3) |
| Max heading angle; lateral time constant | 0.2 rad; 1 s | Constant | §2.2 | A | Fit (stage 3) |
| MOBIL politeness | 0.2 | Scaled by κ | §2.2 | B | Fit (stage 3) |
| Reaction time | Mean 1.02 s (15th pct 0.68, 85th 1.28) | Lognormal | Wong & Goh (§2.3) | B | Fit or fix |
| Critical gap, uncontrolled junction | 2W 4.0–4.8; 3W 6.5–7.1; car 7.7–8.6; LCV 7.0–9.9; bus 8.6; tractor 10–11.5 s | By entering × conflicting class; heavy stream +30–35% | Abhigna et al. 2020 (§2.4) | A | Fit (stage 4) |
| U-turn critical gap and path | Smaller than developed-country values; gamma lateral path on 4-lane roads | Gamma | Mohapatra et al. (§6.7) | A | Fit (stage 4) |

### 9.3 Personas

| Parameter | Starting value or range | Distribution | Source (§) | Grade | Role |
| --- | --- | --- | --- | --- | --- |
| Aggressiveness κ | Standard normal; scales T, s₀, v₀, gaps, clearance, politeness | Continuous, not clusters | IATSS 2022 (§3.1) | B | Fit spread; sweep mean |
| Time pressure | Rises with waiting time | Persona-specific | Christie & Ward 2022 (§3.1) | B/C | Sweep |
| Risk tolerance; conformity; norm sensitivity | Unit-free traits | Normal | §3.1 | C | Sweep |
| Skill; distraction | Position-control noise; reaction-time penalty | Normal; Bernoulli phone use | §3.1 | C | Sweep |
| Within-trip variation of κ | Slow random walk plus jump on delay | Ornstein–Uhlenbeck | Kesting & Treiber; RSS 2022 (§3.1) | B | Fit spread |
| Persona shares | Delivery rider, bike taxi, e-rickshaw, auto, bus, truck, cautious, cyclist, rushed, average | Multinomial by zone and hour | §3.2 | C | Sweep |
| Delivery-rider violation propensity | 65–73% observed violating (Chennai) | Intercept shift | §3.2 | B | Fit intercept |
| E-rickshaw stop propensity | Defining trait; challan ratio parking : wrong-way ≈ 37 : 1 | Poisson rate by land use | §3.2 | B (ranking) / C (rate) | Sweep |

### 9.4 Violations

| Parameter | Starting value or range | Distribution | Source (§) | Grade | Role |
| --- | --- | --- | --- | --- | --- |
| Intercept β₀ per violation type | Fit to observed rates | — | §4.1 | A where measured | Fit (stage 6) |
| Coefficients on time saved and waiting time | Positive | Logistic | §4.1 | C | Fit if data allow; else sweep |
| Imitation coefficient βₛ | Zone- or culture-level; two counts (violators moving, compliers waiting) | Logistic term | Pelé et al. 2017; Fraboni et al. 2016 (§4.3) | C | Sweep |
| Enforcement coefficient βₑ | Through perceived detection | Logistic term | §4.1, §5.2 | C | Sweep |
| Red-light running share | About 1 in 17 (Mumbai) | Two timing regimes (just after red; just before green) | TRR 2017 (§4.2) | A | Fit (stage 6) |
| Countdown-timer effect | End-of-red violation cycles 12% → 75%; start-of-red 59% → 31% | By timer state | Sharma et al. 2012 (§4.2) | A | Check |
| Pedestrian red crossing | 44.6% of red arrivals | Rising through the red | Kumar & Ghosh 2022 (§4.3) | A | Fit |
| Wrong-way propensity | Rises with detour saved | Logistic in detour length | §4.2 | B | Fit or sweep |
| Footpath riding | Only where no bollards; rises with queue length | Logistic | §4.2 | C | Sweep |
| Forced entry probability | Rises with waiting time and κ | Logistic | Dutta & Ahmed 2018 (§2.4, §6.6) | B | Fit (stage 4) |
| Box-blocking; opposite-carriageway encroachment | — | Logistic | §4.2 | C | Sweep |

### 9.5 Zones

| Parameter | Starting value or range | Distribution | Source (§) | Grade | Role |
| --- | --- | --- | --- | --- | --- |
| Detection probability | By officers and cameras per junction | Per zone | §5.2 | C | Sweep |
| Effective penalty | Fine × payment rate (Ahmedabad about 27% paid) | Per zone | Precog (§5.2) | B | Fix per scenario |
| Enforcement halo | Halves every 900 m; fades in about 3 days; police ≥ 2.4 km vs camera 500 m | Exponential decay | Hauer et al. 1982; EU review (§5.2) | B | Fix |
| Median opening spacing | From geometry | Per link | §5.2 | A | Fix |
| Footpath bollards | Present or absent | Per link | §5.2 | A | Fix |
| Kerb occupancy | 0–100% | By land use and hour | §6.2 | A | Fix (measure) |
| Land-use intensity | Markets, metro exits, schools per km | Per link | §5.2 | B | Fix |
| Starting violation share | Seeds imitation | Per zone | §5.2 | C | Sweep |

### 9.6 Road situations

| Parameter | Starting value or range | Distribution | Source (§) | Grade | Role |
| --- | --- | --- | --- | --- | --- |
| Pothole density, size and depth | Diameter 0.3–1.5 m; depth 3–15 cm | Poisson positions; lognormal size | §6.1 | C | Fix (survey) |
| Pothole response by class | 2W mostly swerve; heavy mostly slow | Multinomial | Bandung study (§6.1) | B | Fit |
| Roughness effect on desired speed | Cars lose more than heavy vehicles | Linear in IRI | Chandra 2004 (§6.1) | A/B | Fit (stage 1) |
| Parked-vehicle offset; double-parking probability | — | Empirical | §6.2 | C | Fix (survey) |
| Parking manoeuvre rate and duration | Per 100 m per hour; lognormal duration | Poisson; lognormal | §6.2 | B | Fit (stage 5) |
| Bus dwell time; stopping offset | From observation | Lognormal | Pallela & Mehar 2024 (§6.3) | A | Fit (stage 5) |
| Pedestrian crossing flow; accepted gap | No capacity effect below about 200 ped/h | Poisson flow; gap distribution | Dhamaniya & Chandra 2014 (§6.4) | A | Fit |
| Animal events | Cattle dwell on medians; dog crossings | Poisson; higher at night | §6.5 | C | Sweep |
| Signal state | Normal, timer broken, blinker, dark, police | Markov chain per junction-day; rain raises outage | §6.6 | B/C | Sweep |
| Officer cycle length, bias, compliance | Long cycles; major-approach bias | Per scenario | Vajeeran & De Silva 2022 (§6.6) | C | Sweep |
| Route-awareness distance at splits | Main mass upstream plus "late realiser" tail | Mixture | §6.7 | C | Sweep |
| Stop-and-wait probability at a missed split | Cars more than 2W | Logistic in urgency u | §6.7 | C | Sweep |
| Speed-hump crossing speed | About 14–23 km/h (30–64% reduction) | By class and geometry | WCTR 2019 (§6.8) | A | Fix |
| Rain bundle | Capacity −7% to −22%; free-flow speed −3% to −10% by intensity | Scenario | §6.10 | B | Scenario |
| Waterlogging depth–speed cap | Per flooded segment | Depth–speed curve | Pregnolato et al. 2017 (§6.10) | B | Scenario |

### 9.7 Demand and environment

| Parameter | Starting value or range | Distribution | Source (§) | Grade | Role |
| --- | --- | --- | --- | --- | --- |
| Composition by site and hour | Count it (Delhi arterials: 21–33% 2W, 38–58% small cars, 8–21% 3W) | Multinomial | Dhamaniya & Chandra 2017 (§7.1) | A | Fix |
| Flow by 15-minute interval | From counts | Per interval | §7.4 | A | Fix |
| Arrival headways | Shifted exponential per class | Shifted exponential | Arasan & Koshy (§7.3) | A/B | Fit |
| Entry lateral position | Class-specific | Empirical | §7.3 | A | Fix |
| Turning shares | From counts | Multinomial | §7.5 | A | Fix |
| Day-to-day demand variation | ±10–15% | Normal | §7.4 | B | Sweep |
| Night bundle | Persona shift, blinker signals, lower detection, shorter detection distance | Scenario | §7.6 | B/C | Scenario |
| EV and e-rickshaw composition scenarios | Policy-driven | Scenario | §7.1 | C | Scenario |

### 9.8 Validation targets (not inputs)

| Pattern | Target | Source (§) |
| --- | --- | --- |
| Partial densities, medium and congested | 2W 170 / 450; car 55 / 225; bus 10 / 35; auto 15 / 80 veh/km | Kanagaraj & Treiber 2018 (§7.2) |
| Lane capacity versus operating speed | 1,482–2,105 PCU/h/lane on a quadratic in speed | Dhamaniya & Chandra 2017 (§7.2) |
| Width loss versus capacity loss | Roughly 1 : 1 (7% → 6.4%; 36% → 35.1%) | Varanasi (§6.2) |
| Kerbside bus stop | Capacity down about 8%; speed down up to 22% | §6.3 |
| Potholed section free-flow speed | About 36% lower | §6.1 |
| Pedestrian threshold | No capacity effect below about 200 ped/h | §6.4 |
| Corridor speeds | Arterials about 26–28 km/h most of the day; Lutyens' 44 peak / 52 off-peak | CSE (§7.4) |
| Fit accuracy | Within about 20% on macroscopic characteristics | Kanagaraj & Treiber (§7.2) |

## 10. Sources

Every source linked in this guide, grouped by the section that first cites it. Many are abstracts or secondary summaries; where a number came from a summary rather than the paper itself, the section text says so. Before you cite a value in your paper, open the original.

### Vehicles (§1)

- [Balakrishnan & Sivanandan (2015): free-flow speed by lane and sub-class, Chennai](https://publications.iitm.ac.in/publication/influence-of-lane-and-vehicle-subclass-on-free-flow-speeds)
- [Dhamaniya & Chandra (2017): operating speed and capacity of urban midblocks](https://link.springer.com/content/pdf/10.1007/s40999-017-0206-7.pdf)
- [Agrawal et al. (2026): Chennai drone trajectories](https://arxiv.org/abs/2608.00602)
- [Ramireddy et al. (2021): acceleration and deceleration at signals](https://pp.bme.hu/tr/article/download/16796/9272)
- [Khan & Dass (2025): dynamic PCU including e-rickshaws, Sonipat](https://komunikacie.uniza.sk/pdfs/csl/2025/02/10.pdf)
- [CMVR Rule 2(cb): e-rickshaw definition](https://advocategandhi.com/?p=28460)
- [Deccan Herald: 2014 e-rickshaw notification](https://www.deccanherald.com/archives/e-rickshaws-likely-used-goods-2208844)
- [Choudhary (CRRI): auto-rickshaw operation, Guwahati](https://2021.ursi.org/proceedings/procAP19/papers2019/URSISummaryPaperArtiChoudhary.pdf)

### Interaction (§2)

- [Kanagaraj & Treiber (2018): self-driven particle model for mixed traffic](https://arxiv.org/pdf/1805.05076)
- [Kanagaraj et al. (2015): trajectory data of mixed traffic, Chennai](https://idr.nitk.ac.in/handle/123456789/13557)
- [Asaithambi et al. (2016): following behaviour in mixed traffic](https://link.springer.com/content/pdf/10.1007/s40890-016-0025-6.pdf)
- [Munigety (2018): leader-type-dependent car following](https://jtte.chd.edu.cn/article/doi/10.1016/j.jtte.2017.12.002)
- [Budhkar & Maurya (2017): lateral clearance in mixed traffic](https://link.springer.com/article/10.1007/s40534-017-0130-1)
- [Wong & Goh: perception–response time at amber, Singapore](https://swov.nl/en/publicatie/driver-perception-response-time-braking-action-during-signal-change-interval)
- [Abhigna et al. (2020): critical gaps by vehicle class, Warangal](https://pp.bme.hu/tr/article/download/9744/7927)
- [Dutta & Ahmed (2018): gap acceptance and aggressive drivers at uncontrolled T-junctions](https://link.springer.com/doi/10.1007/s40534-017-0151-9)

### Personas (§3)

- [IATSS Research (2022): driver behaviour at signals, Mumbai](https://www.scribd.com/document/603484926/Driver-Behaviour-Modelling)
- [Traffic Injury Prevention (2021): aggressive driving clusters, India](https://informahealthcare.com/doi/abs/10.1080/15389588.2021.1965590)
- [Christie & Ward (2022): gig-economy riders, UK](https://www.eurekalert.org/news-releases/970802)
- [Bertrand et al. (2007): driving licences in Delhi](https://www.nber.org/system/files/working_papers/w12274/w12274.pdf)
- [RSS 2022: within-trip switching of driving style, India](https://www.nrso.ntua.gr/rss2022/wp-content/uploads/2022/06/RSS2022_Presentation_247.pdf)
- [Citizen Matters: Chennai delivery-rider violations survey](https://citizenmatters.in/65-food-delivery-personnel-surveyed-in-chennai-violated-traffic-rules-finds-satta-panchayat-iyakkam/)
- [Kathmandu amber-phase study (2026)](https://www.nepjol.info/index.php/jotse/article/view/97211)
- [Business Standard: 230,617 e-rickshaw challans, Delhi 2025](https://www.business-standard.com/india-news/over-230000-traffic-challans-issued-to-e-rickshaw-drivers-in-delhi-125062001120_1.html)
- [Velaga et al. (2026): review of food-delivery rider behaviour](https://trid.trb.org/View/2721800)

### Violations (§4)

- [TRR (2017): red-light running in Mumbai](https://journals.sagepub.com/doi/10.3141/2619-08)
- [Vellore wrong-way riding field study (2026)](https://trid.trb.org/View/2707922)
- [Sharma et al. (2012): countdown timers and red-light violations, Chennai](https://digitalcommons.unl.edu/civilengfacpub/53)
- [Pathivada & Perumal (2019): amber stop/go by vehicle type](https://bvbvf1.bib-bvb.de/vufind/PrimoRecord/cdi_proquest_journals_2187911483)
- [Ramireddy et al. (2020): stop-line encroachment and saturation flow](https://bvbvf1.bib-bvb.de/vufind/PrimoRecord/cdi_proquest_journals_2414021184)
- [Meganathan & Pulipati (2025): wrong-way crashes](https://bvbvf1.bib-bvb.de/vufind/PrimoRecord/cdi_proquest_journals_3153698854)
- [Bengaluru footpath-riding cases, 2024](https://www.newsminimalist.com/articles/motorists-increasingly-violate-pavements-in-bengaluru-a89adeaf)
- [Dutta et al. (2022): aggressive clearing behaviour, Tirupati](https://link.springer.com/article/10.1007/s40030-022-00634-4)
- [Nakata et al. (2010): lane changing as a social dilemma](https://econpapers.repec.org/RePEc:eee:phsmap:v:389:y:2010:i:23:p:5353-5361)
- [Pelé et al. (2017): social influence on pedestrian red crossing, France and Japan](https://www.ncbi.nlm.nih.gov/pmc/articles/PMC5367276/)
- [Fraboni et al. (2016): cyclist red-light violations, Bologna](https://www.frontiersin.org/articles/10.3389/fpsyg.2016.01834)
- [Keizer et al. (2008): the spreading of disorder](https://pubmed.ncbi.nlm.nih.gov/19023045/)
- [Kumar & Ghosh (2022): pedestrian red-light violation at 12 Indian junctions](https://bvbvf1.bib-bvb.de/vufind/PrimoRecord/cdi_proquest_journals_2638772627)

### Zones (§5)

- [IJPHRD (2019): helmet use, Bengaluru](https://mail.i-scholar.in/index.php/ijphrd/article/view/192073)
- [CAG (2025): helmet compliance, Tamil Nadu](https://cag.org.in/sites/default/files/database/Helmet%20Compliance%20Report%20with%20data.%20FINAL.pdf.pdf)
- [Pioneer (Sept 2024): Delhi red-light challans by district](https://www.dailypioneer.com/2024/state-editions/traffic-cops-issued-over-87-000--challans-this-year-till-september-15.html)
- [Precog, IIIT Delhi: Ahmedabad e-challan analysis](https://arxiv.org/pdf/1909.08106)
- [Hauer et al. (1982): time and distance halo of speed enforcement](https://trid.trb.org/View/187827)
- [European Commission: time and distance halo effects of enforcement](https://road-safety.transport.ec.europa.eu/eu-road-safety-policy/priorities/safe-road-use/safe-speed/archive/speed-enforcement/additional-considerations-speed-enforcement/time-and-distance-halo-effects_en)
- [Deccan Chronicle (2020): wrong-way riding and long dividers, Hyderabad](https://www.pressreader.com/india/deccan-chronicle/20200314/281681141946185)

### Road situations (§6)

- [Pavement distress and vehicle manoeuvres, Bandung](https://ouci.dntb.gov.ua/works/4V2r0oY4)
- [Aydin & Topal (2016): road surface deformations and driving behaviour](https://journals.vilniustech.lt/index.php/Transport/article/view/1455)
- [Aydin & Topal (2016), full text (cites Ben-Edigbe)](https://journals.vilniustech.lt/index.php/Transport/article/download/1455/1157)
- [Klaten–Ngupit potholed road free-flow speed (UGM thesis)](https://etd.repository.ugm.ac.id/penelitian/detail/202541)
- [Chandra (2004): road roughness and capacity of two-lane roads](https://ascelibrary.com/doi/10.1061/%28ASCE%290733-947X%282004%29130%3A3%28360%29)
- [Abdel-Wahed & Hashim: capacity loss review (summarises Chandra 2004)](https://iccae.journals.ekb.eg/article_44383_7417b8f5ce7d29ddc1f783067e3b9342.pdf)
- [Devdiscourse: MoRTH pothole deaths 2020–2024](https://www.devdiscourse.com/article/business/3804410-pothole-peril-alarming-spike-in-road-accident-fatalities)
- [CarKhabri: pothole death counting depends on FIR](https://carkhabri.com/carblogs/9400-die-in-india-due-to-pothole-accidents)
- [Srivastava: side friction in Varanasi (abstract)](https://etasr.com/index.php/ETASR/article/view/5897)
- [Srivastava: side friction in Varanasi (full text)](https://distantreader.org/stacks/journals/etasr/etasr-5897.pdf)
- [Srivastava & Kumar (2023): road side friction on an urban arterial](https://doi.org/10.48084/etasr.5603)
- [Biswas, Chandra & Ghosh (2021): side friction on undivided streets](https://doaj.org/article/135bc20a47b94cc29e82d3c6635eeabb)
- [Pradhan & Dey (2025): parking manoeuvres and speed, Bhubaneswar](https://www.springerprofessional.de/en/effect-of-on-street-parking-maneuver-on-traffic-speed/51642930)
- [Multicollinearity in side-friction analysis (Hyderabad, Vellore)](https://typeset.io/pdf/dealing-with-multicollinearity-problem-in-analysis-of-side-4iktjzbnop.pdf)
- [Pallela & Mehar (2024): capacity drop at bus stops](https://doaj.org/article/83120e38c2774ddd9f4b28d69772a972)
- [Kerbside bus stop capacity reduction, Warangal](https://katalog.lib.cas.cz/KNAV/EdsRecord/edo,ejs60270790)
- [Lane-wise headway and speed at kerbside bus stops (2023)](https://springerlink.bibliotecabuap.elogim.com/content/pdf/10.1007/s40030-023-00720-1.pdf)
- [Improper bus stopping on New Delhi arterials](https://link.springer.com/doi/10.1007/s40890-017-0033-1)
- [Koshy & Arasan (2005): bus stops in heterogeneous traffic simulation](https://data.zhenzhi.com/details/1/872154936)
- [Dhamaniya & Chandra (2014): undesignated pedestrian crossings and capacity](https://journals.sagepub.com/doi/abs/10.3141/2461-17)
- [Dhamaniya & Chandra (2014), references and base capacity](https://journals.sagepub.com/doi/ref/10.3141/2461-17)
- [Pedestrian crossings and midblock capacity, Hyderabad](https://trid.trb.org/View/2671543)
- [Crossing pedestrians and midblock speed, simulation (2022)](https://aminer.org/pub/636ddaa590e50fcafdbbfd0f/analysis-of-the-impact-of-pedestrian-crossing-activity-on-the-traffic-characteristics)
- [Tribune: stray cattle in Gurugram](https://www.tribuneindia.com/news/delhi/stray-cattle-take-over-gurugram-roads-residents-fear-for-safety/amp)
- [Mohanty et al. (2021): injuries from stray-animal crashes](https://doaj.org/article/dba2617454984732985acc591ba0c32a)
- [Mohanty et al. (2021), full text](https://pmc.ncbi.nlm.nih.gov/articles/PMC8054802)
- [Morning Standard (2025): CAG on countdown timers](https://www.magzter.com/en/stories/newspaper/The-Morning-Standard/CAG-40-COUNTDOWN-TIMERS-POOR)
- [Lok Sabha answer: Delhi traffic signals and blinker mode](https://eparlib.nic.in/bitstream/123456789/692250/1/4405.pdf)
- [Business Standard / IANS (2013): why Delhi's signals fail](https://www.business-standard.com/article/news-ians/why-delhi-s-traffic-lights-go-on-the-blink-113090100394_1.html)
- [Dutta & Ahmed (2018): gap acceptance at uncontrolled T-junctions](https://link.springer.com/article/10.1007/s40534-017-0151-9)
- [Asaithambi & Anuroop (2016): occupation time at unsignalised junctions](https://idr.nitk.ac.in/jspui/handle/123456789/9953?mode=full)
- [Kota, Pallela & Lalam (2025): uncontrolled junction capacity, Warangal](https://sjsutst.polsl.pl/archives/2025/vol126/097_SJSUTST126_2025_Kota_Pallela_Lalam.pdf)
- [Dhaka Tribune: signal outage at Banglamotor](https://www.dhakatribune.com/bangladesh/dhaka/414437/ai-traffic-signal-out-of-service-for-4.5hrs-at)
- [Roussou et al. (2026): signal failure in central Athens](https://www.nrso.ntua.gr/geyannis/wp-content/uploads/geyannis-pc689.pdf)
- [Vajeeran & De Silva (2022): manual police control vs signals](https://dl.lib.uom.lk/items/f0a993cb-9288-4f85-919d-d1f517cc6374)
- [Mandatory lane changes at weaving bottlenecks (UC Berkeley report)](https://escholarship.org/content/qt2970816w/qt2970816w.pdf?t=lat7i0)
- [Arman & Tampère: lane-change manoeuvres in weaving areas](https://lirias.kuleuven.be/retrieve/722302)
- [hEART 2022: empirical lane changes in a weaving area](https://transp-or.epfl.ch/heart/2022/abstracts/183.pdf)
- [TR Part C (2018): weaving capacity and capacity drop](https://dx.doi.org/10.1016/j.trc.2018.09.014)
- [Kusuma et al. (2020): lane changing on motorway weaving sections](https://eprints.whiterose.ac.uk/155273/1/Kusuma%20et%20al%20-%20TransportmetricaB2020-WRR.pdf)
- [Mohapatra, Dey & Chandra (2016): path of U-turning vehicles](https://idr.iitbbs.ac.in/items/3dbdccf4-9832-4cd9-bb7c-d30ab9fd88bb/full)
- [Mohapatra et al.: conflicting volume at median openings](https://scite.ai/authors/smruti-sourava-mohapatra-aykwJ1)
- [TRR (2021): critical gaps for U-turns at median openings, India](https://journals.sagepub.com/doi/10.1177/03611981211035761)
- [Dash, Mohapatra & Dey (2019): U-turn critical gap](https://katalog.lib.cas.cz/KNAV/EdsRecord/edb,136150370)
- [Sil et al. (2016): merging of U-turns at median openings](https://journals.vilniustech.lt/index.php/Transport/article/view/193)
- [Korra Ravi Kiran et al. (WCTR 2019): speed humps in India](https://www.wctrs-society.com/wp-content/uploads/abstracts/mumbai/WCTR2019_Mumbai_435.pdf)
- [WCTR 2019: delay and fuel at speed humps, Hyderabad](https://www.wctrs-society.com/wp-content/uploads/abstracts/mumbai/WCTR2019_Mumbai_440.pdf)
- [EASTS (2019): speed humps, speed profiles and fuel](https://www.jstage.jst.go.jp/article/easts/13/0/13_2073/_article/-char/en)
- [IRC:99-1988: guidelines on speed breakers](https://law.resource.org/pub/in/bis/irc/irc.gov.in.099.1988.pdf)
- [Arasan & Arkatkar (2010): heterogeneous traffic on upgrades (cites Chandra & Goyal 2001)](https://jau.vgtu.lt/index.php/Transport/article/download/5758/5002)
- [Rainfall and urban road capacity (2018)](https://unpaywall.org/10.1007%2Fs13177-018-0162-x)
- [Rainfall and speed–flow–density on Hong Kong urban roads](https://ascelibrary.com/doi/10.1061/%28ASCE%29TE.1943-5436.0000544)
- [Rainfall and traffic flow, Johor and Terengganu](https://0-doi-org.brum.beds.ac.uk/10.11113/jt.v70.3490)
- [Xu et al.: rainfall and the macroscopic fundamental diagram](https://katalog.lib.cas.cz/KNAV/EdsRecord/edb,94524430)
- [WCTR 2019: rainfall impacts on Mumbai transport](https://www.wctrs-society.com/wp-content/uploads/abstracts/mumbai/WCTR2019_Mumbai_516.pdf)
- [DA-IICT thesis: rain and waterlogging in SUMO](https://ir.daiict.ac.in/items/0f967eb3-8fa5-4e66-8d89-5b5de6b1987b)
- [Millennium Post: Okhla diversion jams](https://www.millenniumpost.in/cramped-and-cluttered-212473)

### Demand and environment (§7)

- [Millennium Post: Delhi Economic Survey 2025–26 vehicle count](https://www.millenniumpost.in/delhi/delhi-vehicle-count-up-79-to-876-lakh-653097)
- [Pioneer: vehicles per 1,000 people, 2025–26](https://dailypioneer.com/news/delhis-vehicle-count-hits-87-6-lakh-8-rise-in-2025-26)
- [Statesman: registered versus on-road vehicles (Economic Survey)](https://www.thestatesman.com/india/every-2nd-person-delhi-owns-vehicle-says-economic-survey-1502605348.html)
- [The Patriot (2026): e-rickshaw share of Delhi EV registrations](https://thepatriot.in/?p=80704)
- [Delhi EV registrations, January–May 2026](https://www.inkl.com/news/electric-two-wheelers-private-ev-cars-record-highest-registrations-since-2019-in-delhi)
- [Crisil (2026): Delhi EV Policy Draft 2026–2030](https://intelligence.crisil.com/en/homepage/what-we-think/all-our-thinking/reports/2026/04/driving-past-the-ice-age.html)
- [Dhamaniya & Chandra (2017), abstract: lane capacity and operating speed](https://link.springer.com/doi/10.1007/s40999-017-0206-7)
- [Arasan & Koshy: headway distribution on urban arterials](https://publications.iitm.ac.in/publication/headway-distribution-of-heterogeneous-traffic-on-urban-arterials)
- [Roy & Saha (2017): headway models on two-lane roads](https://link.springer.com/doi/10.1007/s12544-017-0276-2)
- [Balakrishnan & Sivanandan (2015), TU Delft record](https://repository.tudelft.nl/record/uuid:8a7f8349-49c3-48c3-af8b-07f81a480252)
- [CSE (2017): congestion on Delhi arterials, Google Maps data](https://www.cseindia.org/congestion-on-delhi-roads-has-worsened-6994)
- [Scroll: CSE study, Lutyens' speeds](https://scroll.in/latest/843382/scroll_in)
- [CSE (2020): source contribution and congestion impacts](https://www.cseindia.org/source-contribution-and-congestion-impacts.pdf)
- [Hindustan Times: CSE study, Lutyens' road widths](https://www.pressreader.com/india/hindustan-times-delhi/20170711/281633895275606)
- [Night-time speeds on Karnataka rural curves (2025)](https://idr.nitk.ac.in/handle/123456789/19999)
- [Jägerbrand & Sjöbergh (2016): light, weather and speed](https://eprints.lib.hokudai.ac.jp/dspace/handle/2115/62388)
- [Jägerbrand & Sjöbergh (2016), full text](https://eprints.lib.hokudai.ac.jp/dspace/bitstream/2115/62388/1/art%253A10.1186%252Fs40064-016-2124-6.pdf)
- [Autocar India: GRAP Stage III restrictions](https://www.autocarindia.com/car-news-amp/bs-iii-petrol-bs-iv-diesel-vehicles-banned-in-delhi-ncr-438428)
- [Spinny: revised GRAP vehicle rules](https://www.spinny.com/blog/delhi-vehicle-entry-ban-from-november-1-bs4-cars-from-noida-gurugram-not-allowed/)

### Calibration (§8)

- [Punzo, Montanino & Ciuffo (2015): do we need to calibrate all parameters?](https://www.iris.unina.it/handle/11588/587745)
- [Punzo & Ciuffo: parameters and traffic dynamics in AIMSUN](https://bibliotek.vti.se/bib/272512)
- [Kanagaraj et al. (2015) and Kesting & Treiber (2008), summary page](https://typeset.io/papers/trajectory-data-and-flow-characteristics-of-mixed-traffic-32z105vdjy)
- [Carrella (2021): no free lunch when estimating simulation parameters](https://www.jasss.org/24/2/7.html)
- [Platt (2020): comparison of agent-based model calibration methods](https://www.inet.ox.ac.uk/publications/a-comparison-of-economic-agent-based-model-calibration-methods-1)
- [Olukan et al. (2026): heterogeneity and parameter identification](https://www.jasss.org/29/2/5/5.pdf)
- [Mathew & Radhakrishnan (2010): calibrating microsimulation for non-lane-based traffic](https://data.zhenzhi.com/details/1/907895408)
- [Chaudhari et al. (2021): calibrating Wiedemann-99 to mixed-traffic trajectories](https://www.bohrium.com/paper-details/calibrating-wiedemann-99-model-parameters-to-trajectory-data-of-mixed-vehicular-traffic/811845796193370112-4628)
- [Kashyap et al. (2024): longitudinal dynamics in disordered traffic](https://ideas.repec.org/a/eee/phsmap/v646y2024ics0378437124003753.html)
- [Azam et al. (2023): survey of VISSIM calibration for heterogeneous traffic](https://researchgate.net/profile/Muhammad-Azam-49/publication/369037482_Calibration_methodologies_of_VISSIM-based_microsimulation_model_for_heterogeneous_traffic_conditions_-_a_survey/links/6560480d3fa26f66f421fe0f/Calibration-methodologies-of-VISSIM-based-microsimulation-model-for-heterogeneous-traffic-conditions-a-survey.pdf)

Also cited without a link: Grimm et al. (2005), pattern-oriented modelling; Ciuffo, Casas, Montanino, Perarnau & Punzo (2013), Gaussian-process metamodels for sensitivity analysis, *Transportation Research Record* 2390; Kesting & Treiber (2008), car-following calibration; Chandra & Goyal (2001), grade and free speed.
