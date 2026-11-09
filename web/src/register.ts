/* Generated from schema/register_file.schema.json by scripts/gen-schema.sh. Do not edit. */

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
 * One register file: a group of entries sharing a path prefix.
 */
export interface RegisterFile {
  description?: string;
  /**
   * Entries keyed by their path below the group, e.g. `following.time_gap.car`.
   */
  entries: {
    [k: string]: ParamEntry;
  };
  /**
   * Path prefix of every entry in the file, e.g. `interaction`.
   */
  group: string;
}
/**
 * One register entry.
 */
export interface ParamEntry {
  /**
   * Trust grade.
   */
  grade: ("A/B" | "B/C") | "A" | "B" | "C";
  /**
   * Guide section(s), e.g. `"§2.1"`.
   */
  guide: string;
  /**
   * Whether the live viewer may change it during a run.
   */
  live?: boolean;
  /**
   * Assumptions, caveats, how a range was turned into a distribution.
   */
  notes?: string;
  /**
   * Hard bounds `[min, max]` for overrides and sweeps, in `unit`.
   *
   * @minItems 2
   * @maxItems 2
   */
  range?: [number, number] | null;
  /**
   * Which row of the guide's master register (§9) this entry implements,
   * e.g. `"9.2/desired_time_gap"`. Used to check every row is covered.
   */
  register_row?: string | null;
  /**
   * Fix, Fit, Sweep or Scenario.
   */
  role: "fix" | "fit" | "sweep" | "scenario";
  /**
   * Where the value comes from.
   */
  source: string;
  /**
   * Calibration stage 0–7 (G§8.3). Required for `fit`, absent otherwise.
   */
  stage?: number | null;
  /**
   * Unit the value (and `range`) are written in.
   */
  unit:
    | ("m" | "cm" | "km" | "s" | "min" | "h" | "day" | "m/s" | "km/h" | "m/s2" | "rad" | "1/h" | "1/km" | "kW" | "kg")
    | "1"
    | "%"
    | "cm/(km/h)"
    | "(km/h)/%"
    | "(km/h)/IRI"
    | "hour_of_day";
  /**
   * The starting value or distribution, in `unit`. `null` means "not yet
   * known": the parameter must be measured, or set by a scenario or sweep,
   * before any module may use it.
   */
  value?: ParamValue | null;
}
/**
 * One weighted component of a [`Dist::Mixture`].
 */
export interface MixtureComponent {
  dist: Dist;
  weight: number;
}
