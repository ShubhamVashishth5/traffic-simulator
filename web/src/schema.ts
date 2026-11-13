/* Generated from schema/scenario.schema.json by scripts/gen-schema.sh. Do not edit. */

/**
 * A virtual detector.
 */
export type DetectorSpec =
  | {
      id: string;
      interval_s?: number;
      kind: "screenline";
      ribbon: string;
      s: number;
    }
  | {
      id: string;
      interval_s?: number;
      kind: "area";
      ribbon: string;
      s_from: number;
      s_to: number;
    };
/**
 * A univariate distribution.
 *
 * Several lognormal forms are accepted for readability in the register; they
 * are converted to `(mu, sigma)` of `ln X` by [`Dist::canonical`].
 */
export type Dist =
  | {
      dist: "constant";
      value: number;
    }
  | {
      dist: "normal";
      mean: number;
      sd: number;
    }
  | {
      dist: "trunc_normal";
      max: number;
      mean: number;
      min: number;
      sd: number;
    }
  | {
      dist: "log_normal";
      mu: number;
      sigma: number;
    }
  | {
      dist: "log_normal_median";
      median: number;
      sigma: number;
    }
  | {
      dist: "log_normal_moments";
      mean: number;
      sd: number;
    }
  | {
      dist: "log_normal_quantiles";
      p_hi: number;
      p_lo: number;
      x_hi: number;
      x_lo: number;
    }
  | {
      dist: "gamma";
      scale: number;
      shape: number;
    }
  | {
      dist: "exponential";
      mean: number;
    }
  | {
      dist: "shifted_exponential";
      mean: number;
      shift: number;
    }
  | {
      dist: "uniform";
      max: number;
      min: number;
    }
  | {
      dist: "empirical";
      points: [number, number][];
    }
  | {
      components: MixtureComponent[];
      dist: "mixture";
    };
/**
 * Vehicle class: the level at which most interaction parameters are given.
 */
export type VehicleClass =
  | "bicycle"
  | "cycle_rickshaw"
  | "two_wheeler"
  | "e_rickshaw"
  | "auto_rickshaw"
  | "car"
  | "lcv"
  | "bus"
  | "truck"
  | "tractor";
/**
 * How one vehicle class responds to a road object.
 */
export type ObjectResponse = "block" | "ignore";
/**
 * A parameter value: a plain number or a distribution.
 */
export type ParamValue = number | Dist;

/**
 * A scenario: what to simulate, for how long, and what to record.
 */
export interface Scenario {
  demand: DemandSpec;
  /**
   * Free-text description.
   */
  description?: string;
  /**
   * Virtual detectors (screenlines and areas).
   */
  detectors?: DetectorSpec[];
  /**
   * Time step in seconds.
   */
  dt_s?: number;
  /**
   * Simulated duration including warm-up, in seconds.
   */
  duration_s: number;
  kpi?: KpiSpec;
  /**
   * Short identifier, used in run folder names.
   */
  name: string;
  network: NetworkSpec;
  outputs?: OutputSpec;
  /**
   * Parameter overrides by register path, in the register entry's unit.
   */
  overrides?: {
    [k: string]: ParamValue;
  };
  /**
   * Initial period excluded from KPIs and detector summaries, in seconds.
   */
  warmup_s?: number;
}
/**
 * Arrivals at network entries.
 */
export interface DemandSpec {
  entries: EntrySpec[];
}
/**
 * One entry: a flow profile and a vehicle composition (G§7.1–7.3).
 */
export interface EntrySpec {
  /**
   * Share of each vehicle type in the stream (normalised; count it at the
   * site — registration shares are not the stream, G§7.1).
   */
  composition: {
    auto_rickshaw?: number;
    bicycle?: number;
    bus?: number;
    cycle_rickshaw?: number;
    e_rickshaw?: number;
    e_two_wheeler?: number;
    hatchback?: number;
    lcv?: number;
    moped?: number;
    motorcycle?: number;
    scooter?: number;
    sedan?: number;
    suv?: number;
    tractor?: number;
    truck?: number;
  };
  /**
   * Piecewise-constant flow: each interval applies from `from_s` until the next.
   */
  flow: FlowInterval[];
  id: string;
  ribbon: string;
  /**
   * Station of the entry on the ribbon.
   */
  s?: number;
  /**
   * Feed the entry from a signal upstream instead of letting arrivals
   * in as they come (G§7.3).
   */
  upstream_signal?: UpstreamSignalSpec | null;
}
/**
 * Flow from time `from_s` onwards.
 */
export interface FlowInterval {
  from_s: number;
  veh_per_h: number;
}
/**
 * A signal upstream of an entry, not otherwise simulated: arrivals wait
 * in the entry's queue while it is red and are let in while it is green,
 * so they enter in platoons (G§7.3).
 */
export interface UpstreamSignalSpec {
  cycle_s: number;
  green_s: number;
  /**
   * Time into the cycle at simulated time 0 (s); green comes first.
   */
  offset_s?: number;
}
/**
 * Which detectors the headline KPIs use.
 */
export interface KpiSpec {
  /**
   * Screenline whose count is the throughput KPI.
   */
  throughput_detector?: string | null;
}
/**
 * Road geometry.
 */
export interface NetworkSpec {
  /**
   * Places where vehicles stop to pick up and set down more often (a
   * metro exit, a market): G§6.2, ADR-0014.
   */
  attractors?: AttractorSpec[];
  /**
   * Stop lines that are closed during given periods (a level crossing, a
   * police barrier). Signals arrive in phase P7.
   */
  barriers?: BarrierSpec[];
  /**
   * Kerbside bus stops (G§6.3).
   */
  bus_stops?: BusStopSpec[];
  /**
   * Speed humps (G§6.8).
   */
  humps?: HumpSpec[];
  /**
   * Junctions: places where ribbons meet and vehicles choose where to
   * go (G§2.4, §6.6, §6.7; ADR-0009, ADR-0016).
   */
  junctions?: JunctionSpec[];
  /**
   * Road objects: things with a position and a footprint that vehicles
   * react to (G§6, plan/03-model-design.md §10). They act only through
   * usable width, speed caps, gap supply and incentives; there is no
   * capacity or flow factor to set (ADR-0005).
   */
  objects?: ObjectSpec[];
  /**
   * Stretches of kerb where vehicles park (G§6.2). Each parked vehicle
   * is a road object; parking and unparking block space for a while.
   */
  parking?: ParkingSpec[];
  /**
   * Stretches where pedestrians cross anywhere (G§6.4).
   */
  pedestrian_crossings?: PedestrianCrossingSpec[];
  /**
   * Stretches with potholes (G§6.1).
   */
  pothole_fields?: PotholeFieldSpec[];
  ribbons: RibbonSpec[];
  /**
   * Stretches with a gradient or a rough surface (G§6.9, G§6.1): they
   * lower desired speeds by class and do nothing else.
   */
  surfaces?: SurfaceSpec[];
  /**
   * Standing water (G§6.10).
   */
  waterlogging?: WaterSpec[];
}
/**
 * A stretch of a ribbon along which the stop-anywhere rate of every class
 * (`road.stop_anywhere.rate.*`) is multiplied. It changes an incentive and
 * nothing else (ADR-0005).
 */
export interface AttractorSpec {
  id: string;
  ribbon: string;
  s_from: number;
  s_to: number;
  /**
   * Factor on the rate of stops per vehicle-kilometre (not on flow or
   * capacity).
   */
  stop_rate_multiplier: number;
}
/**
 * A stop line across the whole carriageway. While it is closed, vehicles
 * that have not crossed it stop before it (ADR-0010; Kanagaraj & Treiber
 * model a red light as a standing virtual vehicle across the road).
 */
export interface BarrierSpec {
  /**
   * Periods during which the barrier is closed.
   */
  closed: ClosedInterval[];
  id: string;
  ribbon: string;
  /**
   * Station of the stop line on the ribbon.
   */
  s: number;
}
/**
 * A period `from_s <= t < to_s` (simulated seconds): a barrier is closed,
 * or an object is on the road.
 */
export interface ClosedInterval {
  from_s: number;
  to_s: number;
}
/**
 * A kerbside bus stop at the left edge. Buses that serve it pull in, stand
 * for a dwell time and drive on (ADR-0014's stop mechanism); where they
 * stand is drawn from the register's offsets (`road.bus.*`).
 */
export interface BusStopSpec {
  id: string;
  /**
   * Length of the platform along the road (m).
   */
  length: number;
  ribbon: string;
  /**
   * Station of the middle of the platform (m).
   */
  s: number;
  /**
   * Share of passing buses that stop here, 0–1.
   */
  serving_share: number;
  /**
   * Factor on the stop-anywhere rate along the platform (autos and
   * e-rickshaws gather at stops); omit for none.
   */
  stop_rate_multiplier?: number | null;
  /**
   * Width of carriageway taken by passengers waiting on the road along
   * the platform (m); omit for the register's
   * `road.bus.waiting_passenger_width`, 0 for none.
   */
  waiting_passenger_width?: number | null;
}
/**
 * A speed hump across the road, or part of it.
 */
export interface HumpSpec {
  id: string;
  length: number;
  ribbon: string;
  /**
   * Station of its middle and its extent along the road (m).
   */
  s: number;
  /**
   * Lateral extent; omit either for the carriageway edge. A hump that
   * stops short of an edge leaves a gap narrow vehicles can use.
   */
  y_from?: number | null;
  y_to?: number | null;
}
/**
 * A junction: arms (a ribbon arriving, a ribbon leaving, or both), the
 * share of each arriving stream that leaves by each arm, and who goes
 * first. The builder makes the ribbons inside it.
 *
 * - `box`: one wide, lane-free movement ribbon from each arriving arm to
 *   each arm its traffic leaves by, and the areas where two of them overlap.
 *   A median opening is a box with two opposite carriageways as arms and
 *   U-turn shares.
 * - `split`: the arriving ribbon hands over directly to the leaving
 *   ribbons, which must start where it ends and share its frame (each
 *   takes part of its width). A vehicle must be inside its branch's part of
 *   the width when it gets there (G§6.7).
 * - `roundabout`: a ring of arc ribbons round `centre`, with a connector
 *   from each arriving arm onto the ring and one off it to each leaving
 *   arm (ADR-0009). Traffic circulates clockwise (left-hand traffic).
 */
export interface JunctionSpec {
  arms: ArmSpec[];
  /**
   * Roundabout: centre of the island (world metres), radius of the
   * middle of the circulating carriageway and its width (m).
   *
   * @minItems 2
   * @maxItems 2
   */
  centre?: [number, number] | null;
  /**
   * Who goes first.
   */
  control?:
    | {
        kind: "uncontrolled";
        major?: string[];
      }
    | {
        /**
         * The countdown display. It changes nothing while every driver
         * complies (P7); violations read it from P8 on.
         */
        countdown?: "none" | "working" | "broken";
        kind: "signal";
        /**
         * Time into the cycle at simulated time 0 (s).
         */
        offset_s?: number;
        /**
         * Stages in order; the cycle is the sum of their times.
         */
        phases: PhaseSpec[];
      };
  id: string;
  /**
   * How a junction is built.
   */
  kind?: "box" | "split" | "roundabout";
  ring_radius?: number | null;
  ring_width?: number | null;
  /**
   * Split: how far before the split its sign can be read (m); omit for
   * the register's `split.sign_visibility_distance`.
   */
  sign_visibility_distance?: number | null;
  /**
   * Turning shares: arriving arm → leaving arm → share (normalised per
   * arriving arm; count them at the site, G§7.5). A pair that is absent
   * or zero gets no movement.
   */
  turns: {
    [k: string]: {
      [k: string]: number;
    };
  };
  /**
   * U-turns (a movement back out by the arm it came in by, or whose
   * leaving ribbon runs against the arriving one): how far from the
   * median edge of the receiving carriageway a U-turning vehicle joins
   * it (m). Omit for the register's `gap_acceptance.u_turn.path.*`.
   */
  u_turn_path?: Dist | null;
}
/**
 * One arm of a junction.
 */
export interface ArmSpec {
  id: string;
  /**
   * The ribbon that arrives here; its end is the stop line.
   */
  in?: string | null;
  /**
   * The ribbon that leaves from here.
   */
  out?: string | null;
}
/**
 * One stage of a fixed-time plan: green for every movement from `arms`
 * (and for the listed `movements`, written `from>to`), then amber, then
 * red for all.
 */
export interface PhaseSpec {
  all_red_s?: number;
  amber_s: number;
  arms?: string[];
  green_s: number;
  movements?: string[];
}
/**
 * One weighted component of a [`Dist::Mixture`].
 */
export interface MixtureComponent {
  dist: Dist;
  weight: number;
}
/**
 * A road object: a rectangle lying along a ribbon (a parked vehicle, a
 * vendor's cart, debris). Vehicles see it as a standing body: they keep
 * their lateral clearance to it, brake for it and steer around it like
 * they do for a stopped vehicle.
 */
export interface ObjectSpec {
  /**
   * The vehicle class whose lateral-clearance line and following
   * multipliers apply to this object (a parked car is passed like a
   * car). Omit for the pooled clearance line.
   */
  clearance_as?: VehicleClass | null;
  id: string;
  /**
   * What a road object is.
   */
  kind?:
    | "obstacle"
    | "parked_vehicle"
    | "parking_manoeuvre"
    | "waiting_passengers"
    | "pedestrians"
    | "pothole"
    | "hump"
    | "water";
  /**
   * Extent of the footprint along the ribbon (m).
   */
  length: number;
  /**
   * Periods during which the object is on the road; empty means always.
   */
  present?: ClosedInterval[];
  /**
   * How each vehicle class responds; classes not listed are blocked.
   */
  response?: {
    auto_rickshaw?: ObjectResponse;
    bicycle?: ObjectResponse;
    bus?: ObjectResponse;
    car?: ObjectResponse;
    cycle_rickshaw?: ObjectResponse;
    e_rickshaw?: ObjectResponse;
    lcv?: ObjectResponse;
    tractor?: ObjectResponse;
    truck?: ObjectResponse;
    two_wheeler?: ObjectResponse;
  };
  ribbon: string;
  /**
   * Station of the footprint's centre on the ribbon (m).
   */
  s: number;
  /**
   * Extent of the footprint across the ribbon (m).
   */
  width: number;
  /**
   * Lateral position of the footprint's centre (m).
   */
  y: number;
}
/**
 * Kerbside parking along one side of a ribbon (G§6.2). Parked vehicles are
 * drawn at the start of the run to fill `occupancy` of the kerb length;
 * they are road objects, so they act only by taking width (ADR-0005).
 *
 * The survey quantities (`offset_from_kerb`, `double_parking_probability`,
 * `manoeuvres_per_100m_h`, `cruising_share`) may be given here for this
 * stretch; where omitted, the register value is used
 * (`road.parking.*`), and the run is refused if that is unset too.
 */
export interface ParkingSpec {
  /**
   * Vehicle types that park here, with weights.
   */
  composition: {
    auto_rickshaw?: number;
    bicycle?: number;
    bus?: number;
    cycle_rickshaw?: number;
    e_rickshaw?: number;
    e_two_wheeler?: number;
    hatchback?: number;
    lcv?: number;
    moped?: number;
    motorcycle?: number;
    scooter?: number;
    sedan?: number;
    suv?: number;
    tractor?: number;
    truck?: number;
  };
  /**
   * Share of passing vehicles (of the types that park here) that are
   * looking for a space and slow down along this stretch.
   */
  cruising_share?: number | null;
  /**
   * Probability that a parked vehicle has another standing beside it.
   */
  double_parking_probability?: number | null;
  id: string;
  /**
   * Parking and unparking manoeuvres per 100 m of kerb per hour.
   */
  manoeuvres_per_100m_h?: number | null;
  /**
   * Share of the kerb length taken by parked vehicles at the start, 0–1
   * (measure it from video or imagery).
   */
  occupancy: number;
  /**
   * Gap between the kerb and a parked vehicle (m).
   */
  offset_from_kerb?: Dist | null;
  ribbon: string;
  /**
   * The stretch of kerb, by station (m).
   */
  s_from: number;
  s_to: number;
  /**
   * Which edge of the carriageway the vehicles park at.
   */
  side?: "left" | "right";
}
/**
 * A stretch along which pedestrians cross wherever they are (G§6.4).
 */
export interface PedestrianCrossingSpec {
  /**
   * Share that starts from the left edge (the kerb); the rest start from
   * the right edge (the median).
   */
  from_left_share: number;
  id: string;
  /**
   * Crossings (groups) per hour over the whole stretch.
   */
  per_hour: number;
  ribbon: string;
  s_from: number;
  s_to: number;
}
/**
 * A stretch with potholes, placed at random at the start of the run.
 */
export interface PotholeFieldSpec {
  id: string;
  /**
   * How much more often they lie towards the kerb (left): 0 is uniform
   * across the road, 1 is none at the median edge rising linearly to
   * twice the mean at the kerb.
   */
  kerb_bias: number;
  /**
   * Potholes per kilometre of road; omit for `road.pothole.density`.
   */
  per_km?: number | null;
  ribbon: string;
  s_from: number;
  s_to: number;
}
/**
 * A ribbon: a reference polyline with a lateral width profile. `y` is
 * positive to the left of the direction of travel; for left-hand traffic the
 * kerb is on the left (`y_max`) and the median on the right (`y_min`).
 */
export interface RibbonSpec {
  id: string;
  /**
   * The ribbon traffic continues onto at the end, if any.
   */
  next?: string | null;
  /**
   * Reference line vertices in world metres (UTM or local), at least two.
   */
  polyline: [number, number][];
  /**
   * Width profile: lateral bounds at stations along the ribbon
   * (piecewise linear between stations, constant beyond the ends).
   */
  width: WidthPoint[];
}
/**
 * Lateral bounds of the carriageway at station `s`.
 */
export interface WidthPoint {
  s: number;
  y_max: number;
  y_min: number;
}
/**
 * A stretch with an upgrade and/or a rough surface.
 */
export interface SurfaceSpec {
  /**
   * Upgrade in percent (0 or omitted: level).
   */
  gradient_percent?: number | null;
  id: string;
  ribbon: string;
  /**
   * Roughness in IRI units above a smooth surface (omitted: smooth).
   */
  roughness_iri?: number | null;
  s_from: number;
  s_to: number;
}
/**
 * A patch of standing water. It caps speed on it (from its depth: G§6.10
 * cites the depth–speed curve of Pregnolato et al. 2017), or closes the
 * patch altogether, and hides the potholes under it.
 */
export interface WaterSpec {
  /**
   * Too deep to drive through: the patch takes its width.
   */
  closed?: boolean;
  id: string;
  ribbon: string;
  s_from: number;
  s_to: number;
  /**
   * Speed vehicles can keep through it (km/h); omit if `closed`.
   */
  speed_cap_kmh?: number | null;
  /**
   * Lateral extent; omit either for the carriageway edge.
   */
  y_from?: number | null;
  y_to?: number | null;
}
/**
 * Optional outputs.
 */
export interface OutputSpec {
  /**
   * Effective-width observers: how much of the carriageway traffic
   * uses, against what road objects leave free (G§6.2).
   */
  effective_width?: EffectiveWidthSpec[];
  /**
   * Trajectory sampling; omit to write no trajectories.
   */
  trajectories?: TrajectorySpec | null;
}
/**
 * An effective-width observer over `[s_from, s_to]` of a ribbon, in bins
 * of `bin_m` along the road. Per bin it reports the width road objects
 * leave free, the shy distance the clearance model predicts for the
 * vehicles that passed, and the width those vehicles actually used
 * (`effective_width.parquet`). It reports; it changes nothing.
 */
export interface EffectiveWidthSpec {
  /**
   * Length of a bin along the road (m).
   */
  bin_m: number;
  id: string;
  ribbon: string;
  s_from: number;
  s_to: number;
}
/**
 * Trajectory sampling rate and window.
 */
export interface TrajectorySpec {
  /**
   * Sampling interval in seconds (a multiple of `dt_s`).
   */
  every_s: number;
  /**
   * Only record from this time on (default: end of warm-up).
   */
  from_t?: number | null;
  /**
   * Only record agents with `s` in `[s_from, s_to]` (any ribbon).
   */
  s_from?: number | null;
  s_to?: number | null;
}
