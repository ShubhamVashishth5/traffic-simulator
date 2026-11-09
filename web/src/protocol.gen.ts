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
      object: unknown;
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
  ribbons: RibbonSpec[];
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
   * `false` if the agent is not in the network (not entered yet, or exited).
   */
  found: boolean;
  /**
   * Free acceleration `a[1 − (v/v0)^δ]` (m/s²).
   */
  free_accel?: number | null;
  id: string;
  neighbours: Neighbour[];
  /**
   * Persona, traits and decision explanations arrive with P8.
   */
  note: string;
  record?: AgentRow | null;
  state?: AgentState | null;
  t: number;
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
  delay: number;
  distance: number;
  entry: string;
  entry_y: number;
  exited: boolean;
  free_flow_time: number;
  length: number;
  max_accel: number;
  spawn_delay: number;
  standstill_gap: number;
  stops: number;
  t_arrival: number;
  t_entry: number;
  t_exit?: number | null;
  time_gap: number;
  travel_time: number;
  v0: number;
  vehicle_type: string;
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
