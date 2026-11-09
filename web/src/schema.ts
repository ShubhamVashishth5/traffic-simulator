/* Generated from schema/scenario.schema.json by scripts/gen-schema.sh. Do not edit. */

/**
 * A scenario: what to simulate, for how long, and what to record.
 */
export interface Scenario {
  /**
   * Free-text description.
   */
  description?: string;
  /**
   * Time step in seconds.
   */
  dt_s?: number;
  /**
   * Simulated duration including warm-up, in seconds.
   */
  duration_s: number;
  /**
   * Short identifier, used in run folder names.
   */
  name: string;
  /**
   * Placeholder demand for the phase-P0 skeleton: Poisson arrivals per hour.
   */
  toy_arrivals_per_h: number;
}
