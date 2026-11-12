//! Scenario specification: network, demand, detectors, outputs and parameter
//! overrides (plan/03-model-design.md, plan/04-data-outputs-analytics.md).

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
}

impl ObjectKind {
    pub fn key(self) -> &'static str {
        match self {
            ObjectKind::Obstacle => "obstacle",
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

/// Optional outputs.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OutputSpec {
    /// Trajectory sampling; omit to write no trajectories.
    #[serde(default)]
    pub trajectories: Option<TrajectorySpec>,
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
