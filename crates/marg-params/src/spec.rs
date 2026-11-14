//! Scenario specification: network, demand, detectors, outputs and parameter
//! overrides (plan/03-model-design.md, plan/04-data-outputs-analytics.md).

use crate::dist::Dist;
use crate::param::ParamValue;
use crate::vehicle::{VehicleClass, VehicleType};
use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A scenario: what to simulate, for how long, and what to record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    /// Short identifier, used in run folder names.
    pub name: String,
    /// Free-text description.
    #[serde(default)]
    pub description: String,
    /// Simulated duration including warm-up, in seconds.
    pub duration_s: f64,
    /// Initial period excluded from KPIs and detector summaries, in seconds.
    #[serde(default)]
    pub warmup_s: f64,
    /// Time step in seconds.
    #[serde(default = "default_dt")]
    pub dt_s: f64,
    /// Road geometry.
    pub network: NetworkSpec,
    /// Arrivals at network entries.
    pub demand: DemandSpec,
    /// Virtual detectors (screenlines and areas).
    #[serde(default)]
    pub detectors: Vec<DetectorSpec>,
    /// Optional outputs.
    #[serde(default)]
    pub outputs: OutputSpec,
    /// Which detectors the headline KPIs use.
    #[serde(default)]
    pub kpi: KpiSpec,
    /// Parameter overrides by register path, in the register entry's unit.
    #[serde(default)]
    pub overrides: IndexMap<String, ParamValue>,
}

fn default_dt() -> f64 {
    0.1
}

fn default_interval() -> f64 {
    60.0
}

/// Road geometry as a set of ribbons (ADR-0002).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetworkSpec {
    pub ribbons: Vec<RibbonSpec>,
    /// Stop lines that are closed during given periods (a level crossing, a
    /// police barrier). Signals arrive in phase P7.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub barriers: Vec<BarrierSpec>,
    /// Road objects: things with a position and a footprint that vehicles
    /// react to (G§6, plan/03-model-design.md §10). They act only through
    /// usable width, speed caps, gap supply and incentives; there is no
    /// capacity or flow factor to set (ADR-0005).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub objects: Vec<ObjectSpec>,
    /// Stretches of kerb where vehicles park (G§6.2). Each parked vehicle
    /// is a road object; parking and unparking block space for a while.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parking: Vec<ParkingSpec>,
    /// Places where vehicles stop to pick up and set down more often (a
    /// metro exit, a market): G§6.2, ADR-0014.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attractors: Vec<AttractorSpec>,
    /// Kerbside bus stops (G§6.3).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bus_stops: Vec<BusStopSpec>,
    /// Stretches with potholes (G§6.1).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pothole_fields: Vec<PotholeFieldSpec>,
    /// Speed humps (G§6.8).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub humps: Vec<HumpSpec>,
    /// Stretches with a gradient or a rough surface (G§6.9, G§6.1): they
    /// lower desired speeds by class and do nothing else.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub surfaces: Vec<SurfaceSpec>,
    /// Standing water (G§6.10).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub waterlogging: Vec<WaterSpec>,
    /// Stretches where pedestrians cross anywhere (G§6.4).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pedestrian_crossings: Vec<PedestrianCrossingSpec>,
    /// Junctions: places where ribbons meet and vehicles choose where to
    /// go (G§2.4, §6.6, §6.7; ADR-0009, ADR-0016).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub junctions: Vec<JunctionSpec>,
}

/// A junction: arms (a ribbon arriving, a ribbon leaving, or both), the
/// share of each arriving stream that leaves by each arm, and who goes
/// first. The builder makes the ribbons inside it.
///
/// - `box`: one wide, lane-free movement ribbon from each arriving arm to
///   each arm its traffic leaves by, and the areas where two of them overlap.
///   A median opening is a box with two opposite carriageways as arms and
///   U-turn shares.
/// - `split`: the arriving ribbon hands over directly to the leaving
///   ribbons, which must start where it ends and share its frame (each
///   takes part of its width). A vehicle must be inside its branch's part of
///   the width when it gets there (G§6.7).
/// - `roundabout`: a ring of arc ribbons round `centre`, with a connector
///   from each arriving arm onto the ring and one off it to each leaving
///   arm (ADR-0009). Traffic circulates clockwise (left-hand traffic).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct JunctionSpec {
    pub id: String,
    #[serde(default)]
    pub kind: JunctionKind,
    pub arms: Vec<ArmSpec>,
    /// Turning shares: arriving arm → leaving arm → share (normalised per
    /// arriving arm; count them at the site, G§7.5). A pair that is absent
    /// or zero gets no movement.
    pub turns: IndexMap<String, IndexMap<String, f64>>,
    /// Who goes first.
    #[serde(default)]
    pub control: ControlSpec,
    /// Roundabout: centre of the island (world metres), radius of the
    /// middle of the circulating carriageway and its width (m).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub centre: Option<[f64; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ring_radius: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ring_width: Option<f64>,
    /// Split: how far before the split its sign can be read (m); omit for
    /// the register's `split.sign_visibility_distance`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sign_visibility_distance: Option<f64>,
    /// U-turns (a movement back out by the arm it came in by, or whose
    /// leaving ribbon runs against the arriving one): how far from the
    /// median edge of the receiving carriageway a U-turning vehicle joins
    /// it (m). Omit for the register's `gap_acceptance.u_turn.path.*`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub u_turn_path: Option<Dist>,
}

/// How a junction is built.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum JunctionKind {
    #[default]
    Box,
    Split,
    Roundabout,
}

/// One arm of a junction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ArmSpec {
    pub id: String,
    /// The ribbon that arrives here; its end is the stop line.
    #[serde(default, rename = "in", skip_serializing_if = "Option::is_none")]
    pub inbound: Option<String>,
    /// The ribbon that leaves from here.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub out: Option<String>,
}

/// Who goes first at a junction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ControlSpec {
    /// No signal. Movements from `major` arms have priority over the
    /// others; lower-priority movements wait for a gap (G§2.4). With no
    /// major arm every movement has the same priority and nobody waits for
    /// a gap: vehicles sort it out inside the junction.
    Uncontrolled {
        #[serde(default)]
        major: Vec<String>,
    },
    /// A fixed-time signal (G§6.6, normal state).
    Signal {
        /// Stages in order; the cycle is the sum of their times.
        phases: Vec<PhaseSpec>,
        /// Time into the cycle at simulated time 0 (s).
        #[serde(default)]
        offset_s: f64,
        /// The countdown display. It changes nothing while every driver
        /// complies (P7); violations read it from P8 on.
        #[serde(default)]
        countdown: CountdownState,
    },
}

impl Default for ControlSpec {
    fn default() -> Self {
        ControlSpec::Uncontrolled { major: Vec::new() }
    }
}

/// A signal's countdown display.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CountdownState {
    #[default]
    None,
    Working,
    Broken,
}

/// One stage of a fixed-time plan: green for every movement from `arms`
/// (and for the listed `movements`, written `from>to`), then amber, then
/// red for all.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PhaseSpec {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arms: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub movements: Vec<String>,
    pub green_s: f64,
    pub amber_s: f64,
    #[serde(default)]
    pub all_red_s: f64,
}

/// A fixed-time signal upstream of an entry, so that traffic arrives in
/// the platoons green releases rather than as a renewal process (G§7.3).
/// With `stop_line_s`, the signal is simulated: arrivals enter at the
/// entry, meet the signal's stop line that far along the ribbon, queue at
/// it on red (and on amber, if they can still stop at the safe braking
/// limit) and discharge as the model's own queues do. Without it, arrivals
/// wait in the entry's queue unless the signal shows green.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct UpstreamSignalSpec {
    pub cycle_s: f64,
    pub green_s: f64,
    /// Amber after green (s); red fills the rest of the cycle.
    #[serde(default)]
    pub amber_s: f64,
    /// Time into the cycle at simulated time 0 (s); green comes first.
    #[serde(default)]
    pub offset_s: f64,
    /// Station of the signal's stop line on the entry's ribbon (m), beyond
    /// the entry's own station.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_line_s: Option<f64>,
}

impl UpstreamSignalSpec {
    /// Whether the signal shows green at time `t`.
    pub fn green_at(&self, t: f64) -> bool {
        (t + self.offset_s).rem_euclid(self.cycle_s) < self.green_s
    }
}

/// A kerbside bus stop at the left edge. Buses that serve it pull in, stand
/// for a dwell time and drive on (ADR-0014's stop mechanism); where they
/// stand is drawn from the register's offsets (`road.bus.*`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BusStopSpec {
    pub id: String,
    pub ribbon: String,
    /// Station of the middle of the platform (m).
    pub s: f64,
    /// Length of the platform along the road (m).
    pub length: f64,
    /// Share of passing buses that stop here, 0–1.
    pub serving_share: f64,
    /// Width of carriageway taken by passengers waiting on the road along
    /// the platform (m); omit for the register's
    /// `road.bus.waiting_passenger_width`, 0 for none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting_passenger_width: Option<f64>,
    /// Factor on the stop-anywhere rate along the platform (autos and
    /// e-rickshaws gather at stops); omit for none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_rate_multiplier: Option<f64>,
}

/// A stretch with potholes, placed at random at the start of the run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PotholeFieldSpec {
    pub id: String,
    pub ribbon: String,
    pub s_from: f64,
    pub s_to: f64,
    /// Potholes per kilometre of road; omit for `road.pothole.density`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_km: Option<f64>,
    /// How much more often they lie towards the kerb (left): 0 is uniform
    /// across the road, 1 is none at the median edge rising linearly to
    /// twice the mean at the kerb.
    pub kerb_bias: f64,
}

/// A speed hump across the road, or part of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HumpSpec {
    pub id: String,
    pub ribbon: String,
    /// Station of its middle and its extent along the road (m).
    pub s: f64,
    pub length: f64,
    /// Lateral extent; omit either for the carriageway edge. A hump that
    /// stops short of an edge leaves a gap narrow vehicles can use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y_from: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y_to: Option<f64>,
}

/// A stretch with an upgrade and/or a rough surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SurfaceSpec {
    pub id: String,
    pub ribbon: String,
    pub s_from: f64,
    pub s_to: f64,
    /// Upgrade in percent (0 or omitted: level).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gradient_percent: Option<f64>,
    /// Roughness in IRI units above a smooth surface (omitted: smooth).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roughness_iri: Option<f64>,
}

/// A patch of standing water. It caps speed on it (from its depth: G§6.10
/// cites the depth–speed curve of Pregnolato et al. 2017), or closes the
/// patch altogether, and hides the potholes under it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WaterSpec {
    pub id: String,
    pub ribbon: String,
    pub s_from: f64,
    pub s_to: f64,
    /// Lateral extent; omit either for the carriageway edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y_from: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y_to: Option<f64>,
    /// Speed vehicles can keep through it (km/h); omit if `closed`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed_cap_kmh: Option<f64>,
    /// Too deep to drive through: the patch takes its width.
    #[serde(default)]
    pub closed: bool,
}

/// A stretch along which pedestrians cross wherever they are (G§6.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PedestrianCrossingSpec {
    pub id: String,
    pub ribbon: String,
    pub s_from: f64,
    pub s_to: f64,
    /// Crossings (groups) per hour over the whole stretch.
    pub per_hour: f64,
    /// Share that starts from the left edge (the kerb); the rest start from
    /// the right edge (the median).
    pub from_left_share: f64,
}

/// A stretch of a ribbon along which the stop-anywhere rate of every class
/// (`road.stop_anywhere.rate.*`) is multiplied. It changes an incentive and
/// nothing else (ADR-0005).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AttractorSpec {
    pub id: String,
    pub ribbon: String,
    pub s_from: f64,
    pub s_to: f64,
    /// Factor on the rate of stops per vehicle-kilometre (not on flow or
    /// capacity).
    pub stop_rate_multiplier: f64,
}

/// Kerbside parking along one side of a ribbon (G§6.2). Parked vehicles are
/// drawn at the start of the run to fill `occupancy` of the kerb length;
/// they are road objects, so they act only by taking width (ADR-0005).
///
/// The survey quantities (`offset_from_kerb`, `double_parking_probability`,
/// `manoeuvres_per_100m_h`, `cruising_share`) may be given here for this
/// stretch; where omitted, the register value is used
/// (`road.parking.*`), and the run is refused if that is unset too.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ParkingSpec {
    pub id: String,
    pub ribbon: String,
    /// The stretch of kerb, by station (m).
    pub s_from: f64,
    pub s_to: f64,
    /// Which edge of the carriageway the vehicles park at.
    #[serde(default)]
    pub side: RoadSide,
    /// Share of the kerb length taken by parked vehicles at the start, 0–1
    /// (measure it from video or imagery).
    pub occupancy: f64,
    /// Vehicle types that park here, with weights.
    pub composition: IndexMap<VehicleType, f64>,
    /// Gap between the kerb and a parked vehicle (m).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset_from_kerb: Option<Dist>,
    /// Probability that a parked vehicle has another standing beside it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub double_parking_probability: Option<f64>,
    /// Parking and unparking manoeuvres per 100 m of kerb per hour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manoeuvres_per_100m_h: Option<f64>,
    /// Share of passing vehicles (of the types that park here) that are
    /// looking for a space and slow down along this stretch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cruising_share: Option<f64>,
}

/// An edge of the carriageway, seen in the direction of travel.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RoadSide {
    /// `y_max`: the kerb in left-hand traffic.
    #[default]
    Left,
    /// `y_min`: the median side in left-hand traffic.
    Right,
}

/// A road object: a rectangle lying along a ribbon (a parked vehicle, a
/// vendor's cart, debris). Vehicles see it as a standing body: they keep
/// their lateral clearance to it, brake for it and steer around it like
/// they do for a stopped vehicle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ObjectSpec {
    pub id: String,
    #[serde(default)]
    pub kind: ObjectKind,
    pub ribbon: String,
    /// Station of the footprint's centre on the ribbon (m).
    pub s: f64,
    /// Lateral position of the footprint's centre (m).
    pub y: f64,
    /// Extent of the footprint along the ribbon (m).
    pub length: f64,
    /// Extent of the footprint across the ribbon (m).
    pub width: f64,
    /// The vehicle class whose lateral-clearance line and following
    /// multipliers apply to this object (a parked car is passed like a
    /// car). Omit for the pooled clearance line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clearance_as: Option<VehicleClass>,
    /// How each vehicle class responds; classes not listed are blocked.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub response: IndexMap<VehicleClass, ObjectResponse>,
    /// Periods during which the object is on the road; empty means always.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub present: Vec<ClosedInterval>,
}

/// What a road object is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ObjectKind {
    /// A solid body on the carriageway.
    #[default]
    Obstacle,
    /// A vehicle parked at the kerb, or standing beside one (double parked).
    ParkedVehicle,
    /// A vehicle manoeuvring into or out of a parking space.
    ParkingManoeuvre,
    /// Passengers waiting on the carriageway at a bus stop.
    WaitingPassengers,
    /// A pedestrian, or a group, crossing the road.
    Pedestrians,
    /// A pothole: not a body; vehicles swerve, slow, straddle or hit it.
    Pothole,
    /// A speed hump: not a body; a speed cap by vehicle.
    Hump,
    /// Standing water shallow enough to drive through: a speed cap.
    Water,
}

impl ObjectKind {
    pub fn key(self) -> &'static str {
        match self {
            ObjectKind::Obstacle => "obstacle",
            ObjectKind::ParkedVehicle => "parked_vehicle",
            ObjectKind::ParkingManoeuvre => "parking_manoeuvre",
            ObjectKind::WaitingPassengers => "waiting_passengers",
            ObjectKind::Pedestrians => "pedestrians",
            ObjectKind::Pothole => "pothole",
            ObjectKind::Hump => "hump",
            ObjectKind::Water => "water",
        }
    }
}

/// How one vehicle class responds to a road object.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ObjectResponse {
    /// The footprint is not usable: the class goes around it or stops.
    #[default]
    Block,
    /// The class takes no notice of the object.
    Ignore,
}

/// A stop line across the whole carriageway. While it is closed, vehicles
/// that have not crossed it stop before it (ADR-0010; Kanagaraj & Treiber
/// model a red light as a standing virtual vehicle across the road).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BarrierSpec {
    pub id: String,
    pub ribbon: String,
    /// Station of the stop line on the ribbon.
    pub s: f64,
    /// Periods during which the barrier is closed.
    pub closed: Vec<ClosedInterval>,
}

/// A period `from_s <= t < to_s` (simulated seconds): a barrier is closed,
/// or an object is on the road.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClosedInterval {
    pub from_s: f64,
    pub to_s: f64,
}

/// A ribbon: a reference polyline with a lateral width profile. `y` is
/// positive to the left of the direction of travel; for left-hand traffic the
/// kerb is on the left (`y_max`) and the median on the right (`y_min`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RibbonSpec {
    pub id: String,
    /// Reference line vertices in world metres (UTM or local), at least two.
    pub polyline: Vec<[f64; 2]>,
    /// Width profile: lateral bounds at stations along the ribbon
    /// (piecewise linear between stations, constant beyond the ends).
    pub width: Vec<WidthPoint>,
    /// The ribbon traffic continues onto at the end, if any.
    #[serde(default)]
    pub next: Option<String>,
}

/// Lateral bounds of the carriageway at station `s`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WidthPoint {
    pub s: f64,
    pub y_min: f64,
    pub y_max: f64,
}

/// Demand at the network's entries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DemandSpec {
    pub entries: Vec<EntrySpec>,
}

/// One entry: a flow profile and a vehicle composition (G§7.1–7.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EntrySpec {
    pub id: String,
    pub ribbon: String,
    /// Station of the entry on the ribbon.
    #[serde(default)]
    pub s: f64,
    /// Piecewise-constant flow: each interval applies from `from_s` until the next.
    pub flow: Vec<FlowInterval>,
    /// Share of each vehicle type in the stream (normalised; count it at the
    /// site — registration shares are not the stream, G§7.1).
    pub composition: IndexMap<VehicleType, f64>,
    /// Feed the entry from a signal upstream instead of letting arrivals
    /// in as they come (G§7.3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream_signal: Option<UpstreamSignalSpec>,
}

/// Flow from time `from_s` onwards.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FlowInterval {
    pub from_s: f64,
    pub veh_per_h: f64,
}

/// A virtual detector.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum DetectorSpec {
    /// Counts vehicles crossing station `s`.
    Screenline {
        id: String,
        ribbon: String,
        s: f64,
        #[serde(default = "default_interval")]
        interval_s: f64,
    },
    /// Measures density, flow and space-mean speed over `[s_from, s_to]`
    /// (Edie's generalised definitions).
    Area {
        id: String,
        ribbon: String,
        s_from: f64,
        s_to: f64,
        #[serde(default = "default_interval")]
        interval_s: f64,
    },
}

impl DetectorSpec {
    pub fn id(&self) -> &str {
        match self {
            DetectorSpec::Screenline { id, .. } | DetectorSpec::Area { id, .. } => id,
        }
    }
    pub fn ribbon(&self) -> &str {
        match self {
            DetectorSpec::Screenline { ribbon, .. } | DetectorSpec::Area { ribbon, .. } => ribbon,
        }
    }
    /// The same detector with another aggregation interval.
    pub fn with_interval(&self, interval: f64) -> Self {
        let mut d = self.clone();
        match &mut d {
            DetectorSpec::Screenline { interval_s, .. } | DetectorSpec::Area { interval_s, .. } => {
                *interval_s = interval
            }
        }
        d
    }
    pub fn interval_s(&self) -> f64 {
        match self {
            DetectorSpec::Screenline { interval_s, .. } | DetectorSpec::Area { interval_s, .. } => {
                *interval_s
            }
        }
    }
}

/// An effective-width observer over `[s_from, s_to]` of a ribbon, in bins
/// of `bin_m` along the road. Per bin it reports the width road objects
/// leave free, the shy distance the clearance model predicts for the
/// vehicles that passed, and the width those vehicles actually used
/// (`effective_width.parquet`). It reports; it changes nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EffectiveWidthSpec {
    pub id: String,
    pub ribbon: String,
    pub s_from: f64,
    pub s_to: f64,
    /// Length of a bin along the road (m).
    pub bin_m: f64,
}

/// Optional outputs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputSpec {
    /// Trajectory sampling; omit to write no trajectories.
    #[serde(default)]
    pub trajectories: Option<TrajectorySpec>,
    /// Effective-width observers: how much of the carriageway traffic
    /// uses, against what road objects leave free (G§6.2).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub effective_width: Vec<EffectiveWidthSpec>,
}

/// Trajectory sampling rate and window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TrajectorySpec {
    /// Sampling interval in seconds (a multiple of `dt_s`).
    pub every_s: f64,
    /// Only record agents with `s` in `[s_from, s_to]` (any ribbon).
    #[serde(default)]
    pub s_from: Option<f64>,
    #[serde(default)]
    pub s_to: Option<f64>,
    /// Only record from this time on (default: end of warm-up).
    #[serde(default)]
    pub from_t: Option<f64>,
}

/// Headline KPI settings.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KpiSpec {
    /// Screenline whose count is the throughput KPI.
    #[serde(default)]
    pub throughput_detector: Option<String>,
}

impl Scenario {
    /// Parse a scenario from YAML text.
    pub fn from_yaml(text: &str) -> Result<Self, serde_norway::Error> {
        serde_norway::from_str(text)
    }
}
