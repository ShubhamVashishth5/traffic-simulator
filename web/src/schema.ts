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
}
/**
 * Flow from time `from_s` onwards.
 */
export interface FlowInterval {
  from_s: number;
  veh_per_h: number;
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
   * Stop lines that are closed during given periods (a level crossing, a
   * police barrier). Signals arrive in phase P7.
   */
  barriers?: BarrierSpec[];
  /**
   * Road objects: things with a position and a footprint that vehicles
   * react to (G§6, plan/03-model-design.md §10). They act only through
   * usable width, speed caps, gap supply and incentives; there is no
   * capacity or flow factor to set (ADR-0005).
   */
  objects?: ObjectSpec[];
  ribbons: RibbonSpec[];
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
  kind?: "obstacle";
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
 * Optional outputs.
 */
export interface OutputSpec {
  /**
   * Trajectory sampling; omit to write no trajectories.
   */
  trajectories?: TrajectorySpec | null;
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
/**
 * One weighted component of a [`Dist::Mixture`].
 */
export interface MixtureComponent {
  dist: Dist;
  weight: number;
}
