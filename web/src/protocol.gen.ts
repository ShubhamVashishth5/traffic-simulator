/* Generated from schema/viewer_protocol.schema.json by scripts/gen-schema.sh. Do not edit. */

/**
 * A message from the viewer.
 */
export type ClientMessage =
  | {
      cmd: "play";
      [k: string]: unknown;
    }
  | {
      cmd: "pause";
      [k: string]: unknown;
    }
  | {
      cmd: "step";
      n?: number;
      [k: string]: unknown;
    }
  | {
      cmd: "speed";
      x: number;
      [k: string]: unknown;
    }
  | {
      cmd: "set_param";
      path: string;
      resample?: boolean;
      value: ParamValue;
      [k: string]: unknown;
    }
  | {
      cmd: "resample";
      path: string;
      [k: string]: unknown;
    }
  | {
      cmd: "undo";
      [k: string]: unknown;
    }
  | {
      cmd: "select_agent";
      id?: string | null;
      [k: string]: unknown;
    }
  | {
      cmd: "breakpoint";
      on: BreakpointOn;
      [k: string]: unknown;
    }
  | {
      cmd: "clear_breakpoints";
      [k: string]: unknown;
    }
  | {
      cmd: "toggle_signal_state";
      junction: string;
      state: string;
      [k: string]: unknown;
    }
  | {
      cmd: "add_object";
      object: ObjectSpec;
      [k: string]: unknown;
    }
  | {
      cmd: "remove_object";
      id: string;
      [k: string]: unknown;
    }
  | {
      cmd: "save";
      [k: string]: unknown;
    }
  | {
      cmd: "view";
      colour?: ColourBy | null;
      fps?: number | null;
      ts_ribbon?: string | null;
      /**
       * @minItems 4
       * @maxItems 4
       */
      viewport?: [number, number, number, number] | null;
      [k: string]: unknown;
    };
/**
 * A parameter value: a plain number or a distribution.
 */
export type ParamValue = number | Dist;
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
 * When to pause automatically.
 */
export type BreakpointOn =
  | {
      kind: string;
      on: "event";
      [k: string]: unknown;
    }
  | {
      on: "time";
      t: number;
      [k: string]: unknown;
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
 * What the per-agent `colour` value of a frame holds.
 */
export type ColourBy =
  | "class"
  | "speed"
  | "speed_ratio"
  | "accel"
  | "lateral"
  | "desired_speed"
  | "persona"
  | "kappa"
  | "violation"
  | "zone";
/**
 * A message from the server: `{"type": …, "data": …}`.
 */
export type ServerMessage =
  | {
      data: Hello;
      type: "hello";
      [k: string]: unknown;
    }
  | {
      data: Status;
      type: "status";
      [k: string]: unknown;
    }
  | {
      data: {
        cmd: string;
        message: string;
        ok: boolean;
        seq?: number | null;
        [k: string]: unknown;
      };
      type: "ack";
      [k: string]: unknown;
    }
  | {
      data: Kpi;
      type: "kpi";
      [k: string]: unknown;
    }
  | {
      data: Inspector;
      type: "inspector";
      [k: string]: unknown;
    }
  | {
      data: {
        changes: ChangeView[];
        [k: string]: unknown;
      };
      type: "history";
      [k: string]: unknown;
    }
  | {
      data: {
        event?: EventRow | null;
        reason: string;
        step: number;
        t: number;
        [k: string]: unknown;
      };
      type: "breakpoint";
      [k: string]: unknown;
    }
  | {
      data: {
        output_hash: string;
        path: string;
        replay_matches: boolean;
        replay_output_hash: string;
        run_hash: string;
        steps: number;
        [k: string]: unknown;
      };
      type: "saved";
      [k: string]: unknown;
    }
  | {
      data: {
        message: string;
        [k: string]: unknown;
      };
      type: "error";
      [k: string]: unknown;
    }
  | {
      data: {
        objects: ObjectView[];
        [k: string]: unknown;
      };
      type: "objects";
      [k: string]: unknown;
    };
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
 * Both directions, for the JSON Schema that generates the TypeScript types.
 */
export interface ViewerProtocol {
  client: ClientMessage;
  server: ServerMessage;
  [k: string]: unknown;
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
 * A period `from_s <= t < to_s` (simulated seconds): a barrier is closed,
 * or an object is on the road.
 */
export interface ClosedInterval {
  from_s: number;
  to_s: number;
}
/**
 * First message of every connection.
 */
export interface Hello {
  /**
   * Network bounds `[x0, y0, x1, y1]`, relative to `origin`.
   *
   * @minItems 4
   * @maxItems 4
   */
  bounds: [number, number, number, number];
  /**
   * Class keys in index order.
   */
  classes: string[];
  colour_modes: ColourMode[];
  detectors: DetectorView[];
  dt_s: number;
  duration_s: number;
  frame_columns: string[];
  /**
   * Run hash of the scenario and seed before any command.
   */
  initial_run_hash: string;
  /**
   * Interval of the live detector windows in `Kpi.windows` (s).
   */
  live_window_s: number;
  marg_version: string;
  /**
   * World coordinates of the frame origin: every position sent is relative to it.
   *
   * @minItems 2
   * @maxItems 2
   */
  origin: [number, number];
  protocol: number;
  ribbons: RibbonView[];
  scenario: Scenario;
  seed: number;
  session: string;
  /**
   * Vehicle types in index order.
   */
  vehicle_types: TypeView[];
  warmup_s: number;
  [k: string]: unknown;
}
export interface ColourMode {
  available: boolean;
  /**
   * `true` if the value is categorical (an index into `classes`).
   */
  categorical: boolean;
  key: ColourBy;
  label: string;
  note: string;
  unit: string;
  [k: string]: unknown;
}
/**
 * A detector, for drawing.
 */
export interface DetectorView {
  id: string;
  /**
   * `screenline` or `area`.
   */
  kind: string;
  ribbon: string;
  s_from: number;
  s_to: number;
  /**
   * Line (screenline) or polygon (area), relative to `origin`.
   */
  shape: [number, number][];
  [k: string]: unknown;
}
/**
 * A ribbon, for drawing.
 */
export interface RibbonView {
  /**
   * Reference line, relative to `origin`.
   */
  centre: [number, number][];
  id: string;
  length: number;
  /**
   * Median-side band of width `vehicles.median_band.width` (polygon).
   */
  median_band: [number, number][];
  next?: string | null;
  /**
   * Carriageway polygon (right edge forward, left edge back), relative to `origin`.
   */
  outline: [number, number][];
  [k: string]: unknown;
}
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
 * A fixed-time signal upstream of an entry, so that traffic arrives in
 * the platoons green releases rather than as a renewal process (G§7.3).
 * With `stop_line_s`, the signal is simulated: arrivals enter at the
 * entry, meet the signal's stop line that far along the ribbon, queue at
 * it on red (and on amber, if they can still stop at the safe braking
 * limit) and discharge as the model's own queues do. Without it, arrivals
 * wait in the entry's queue unless the signal shows green.
 */
export interface UpstreamSignalSpec {
  /**
   * Amber after green (s); red fills the rest of the cycle.
   */
  amber_s?: number;
  cycle_s: number;
  green_s: number;
  /**
   * Time into the cycle at simulated time 0 (s); green comes first.
   */
  offset_s?: number;
  /**
   * Station of the signal's stop line on the entry's ribbon (m), beyond
   * the entry's own station.
   */
  stop_line_s?: number | null;
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
export interface TypeView {
  class: string;
  key: string;
  [k: string]: unknown;
}
export interface Status {
  agents: number;
  breakpoints: BreakpointOn[];
  finished: boolean;
  playing: boolean;
  /**
   * Measured simulated seconds per wall second over the last second.
   */
  real_time_factor: number;
  /**
   * Run identity including the commands so far.
   */
  run_hash: string;
  /**
   * Simulated seconds per wall second (0 = as fast as possible).
   */
  speed: number;
  step: number;
  t: number;
  [k: string]: unknown;
}
/**
 * Sent once per wall second (and after steps while paused).
 */
export interface Kpi {
  agents: number;
  /**
   * Headline KPIs if the run ended now (the computation of `kpis.parquet`).
   */
  kpis: KpiValue[];
  speed_hist: ClassHist[];
  step: number;
  t: number;
  ts?: TsSamples | null;
  /**
   * Vehicles waiting at entries.
   */
  waiting: number;
  /**
   * Live detector windows that closed since the last message.
   */
  windows: DetectorRow[];
  [k: string]: unknown;
}
export interface KpiValue {
  name: string;
  /**
   * `null` = not measured yet.
   */
  value?: number | null;
  [k: string]: unknown;
}
/**
 * Speeds of the agents in the network now, by class.
 */
export interface ClassHist {
  /**
   * Bin width (m/s); bin `k` is `[k·w, (k+1)·w)`.
   */
  bin_width: number;
  class: string;
  counts: number[];
  [k: string]: unknown;
}
/**
 * Time–space samples of one ribbon (positions rounded to 1 cm, speeds to 0.01 m/s).
 */
export interface TsSamples {
  ribbon: string;
  s: number[];
  t: number[];
  v: number[];
  [k: string]: unknown;
}
/**
 * One row of `detectors.parquet` (see the data dictionary); `null` = not measured.
 */
export interface DetectorRow {
  class: string;
  count: number;
  density?: number | null;
  detector_id: string;
  flow?: number | null;
  in_warmup: boolean;
  kind: string;
  lateral_bin?: number | null;
  lateral_hist?: number[] | null;
  lateral_mean?: number | null;
  lateral_sd?: number | null;
  lateral_y0?: number | null;
  space_mean_speed?: number | null;
  t_end: number;
  t_start: number;
  time_mean_speed?: number | null;
  ttd?: number | null;
  tts?: number | null;
  vehicle_type: string;
  [k: string]: unknown;
}
export interface Inspector {
  /**
   * Open violation episodes with their logit breakdown (P8.3).
   */
  decisions: DecisionView[];
  /**
   * `false` if the agent is not in the network (not entered yet, or exited).
   */
  found: boolean;
  /**
   * Free acceleration `a[1 − (v/v0)^δ]` (m/s²).
   */
  free_accel?: number | null;
  id: string;
  neighbours: Neighbour[];
  note: string;
  record?: AgentRow | null;
  state?: AgentState | null;
  t: number;
  [k: string]: unknown;
}
/**
 * An open violation episode of the inspected agent, as its last reading
 * left it (ADR-0018): each term is its coefficient times its feature, and
 * they add up to `eta`.
 */
export interface DecisionView {
  /**
   * It has acted (a violation under way).
   */
  acted: boolean;
  coefficients: Named[];
  episode: number;
  eta: number;
  features: Named[];
  /**
   * Continuous types: hazard λ = λ_max σ(η) (1/s), the hazard integrated
   * so far Λ, and the threshold E it acts at. Null for one-shot types.
   */
  hazard?: number | null;
  integrated?: number | null;
  /**
   * Violation type (`forced_entry`, …).
   */
  kind: string;
  /**
   * 1 − exp(−Λ) (continuous) or σ(η) (one-shot).
   */
  probability: number;
  /**
   * When the features were last read.
   */
  t_read: number;
  t_start: number;
  terms: Named[];
  threshold?: number | null;
  [k: string]: unknown;
}
/**
 * A named number (a feature, a coefficient or a term of a decision).
 */
export interface Named {
  name: string;
  value: number;
  [k: string]: unknown;
}
/**
 * One agent the inspected agent's leader scan considers (plan/03 §2).
 */
export interface Neighbour {
  clearance: number;
  ds: number;
  gap_s: number;
  id: string;
  lateral_gap: number;
  /**
   * IDM acceleration toward this leader alone (null for side neighbours).
   */
  leader_accel?: number | null;
  /**
   * `leader` or `side` (level with the agent).
   */
  role: string;
  vehicle_type: string;
  /**
   * Leader weight ω (null for side neighbours).
   */
  weight?: number | null;
  [k: string]: unknown;
}
/**
 * One row of `agents.parquet`, as it would be if the run ended now.
 */
export interface AgentRow {
  agent_id: string;
  class: string;
  comfortable_decel: number;
  conformity: number;
  delay: number;
  distance: number;
  distraction: number;
  distraction_episodes: number;
  entry: string;
  entry_y: number;
  exited: boolean;
  free_flow_time: number;
  kappa: number;
  /**
   * κ now (moves within a trip with the OU process or the jump on delay).
   */
  kappa_now: number;
  length: number;
  max_accel: number;
  norm_sensitivity: number;
  /**
   * Persona key (null: no persona shares set) and traits at entry (G§3).
   */
  persona?: string | null;
  phone_user: boolean;
  risk_tolerance: number;
  skill: number;
  spawn_delay: number;
  standstill_gap: number;
  stops: number;
  t_arrival: number;
  t_entry: number;
  t_exit?: number | null;
  time_gap: number;
  time_pressure: number;
  travel_time: number;
  v0: number;
  vehicle_type: string;
  violations: number;
  width: number;
  [k: string]: unknown;
}
export interface AgentState {
  acc: number;
  heading: number;
  ribbon: string;
  s: number;
  v: number;
  /**
   * World position relative to `origin`, and heading (rad).
   */
  x_world: number;
  y: number;
  y_world: number;
  [k: string]: unknown;
}
export interface ChangeView {
  after: ParamValue;
  before?: ParamValue | null;
  path: string;
  /**
   * Position in the command log.
   */
  seq: number;
  step: number;
  t: number;
  undone: boolean;
  [k: string]: unknown;
}
export interface EventRow {
  agent_id: string;
  attrs: string;
  kind: string;
  ribbon: string;
  s: number;
  t: number;
  y: number;
  [k: string]: unknown;
}
/**
 * A road object and when it is on the road.
 */
export interface ObjectView {
  id: string;
  length: number;
  /**
   * Periods `[from, to)` on the road; `null` = no bound on that side.
   */
  present: [number | null, number | null][];
  ribbon: string;
  s: number;
  /**
   * Corners of the footprint, relative to `Hello.origin`.
   */
  shape: [number, number][];
  width: number;
  y: number;
  [k: string]: unknown;
}
