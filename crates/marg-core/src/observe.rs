//! Online observers: screenline and area detectors (plan/04 §2–3).
//!
//! Screenlines record every crossing with its interpolated time, spot speed
//! and lateral position. Areas use Edie's generalised definitions: total time
//! spent and distance travelled inside `[s_from, s_to]`, from each step's
//! linear motion clipped to the area.

use crate::agents::AgentStore;
use crate::output::DetectorRecord;
use crate::params::N_TYPES;
use marg_params::VehicleType;
use marg_params::spec::DetectorSpec;

#[derive(Debug, Clone)]
struct ScreenStats {
    count: u64,
    sum_v: f64,
    sum_inv_v: f64,
    sum_y: f64,
    sum_y2: f64,
    hist: Vec<u32>,
}

#[derive(Debug, Clone, Default)]
struct AreaStats {
    entered: u64,
    tts: f64,
    ttd: f64,
}

#[derive(Debug, Clone)]
enum Kind {
    Screen {
        s: f64,
        y0: f64,
        bin: f64,
        stats: Vec<Vec<ScreenStats>>,
        measured_count: u64,
    },
    Area {
        s_from: f64,
        s_to: f64,
        stats: Vec<Vec<AreaStats>>,
    },
}

/// One detector at runtime.
#[derive(Debug, Clone)]
pub struct Detector {
    pub id: String,
    pub ribbon: usize,
    interval: f64,
    kind: Kind,
}

/// All detectors of a run.
#[derive(Debug, Clone)]
pub struct Detectors {
    pub list: Vec<Detector>,
    warmup: f64,
}

/// Lower-bound position in `order` (sorted by `s_prev`) of the first agent
/// with `s_prev >= x`.
fn lower_bound(order: &[u32], s_prev: &[f64], x: f64) -> usize {
    order.partition_point(|&i| s_prev[i as usize] < x)
}

impl Detectors {
    /// Build detectors. `ribbon_of` resolves ids; `bounds(r, s)` gives the
    /// lateral bounds used to place the screenline histogram.
    pub fn new(
        specs: &[DetectorSpec],
        ribbon_of: impl Fn(&str) -> Option<usize>,
        bounds: impl Fn(usize, f64) -> (f64, f64),
        duration: f64,
        warmup: f64,
        lateral_bin: f64,
    ) -> Result<Self, String> {
        let mut list = Vec::new();
        for d in specs {
            let ribbon = ribbon_of(d.ribbon())
                .ok_or_else(|| format!("detector `{}`: unknown ribbon `{}`", d.id(), d.ribbon()))?;
            let interval = d.interval_s();
            if interval.is_nan() || interval <= 0.0 {
                return Err(format!(
                    "detector `{}`: interval_s must be positive",
                    d.id()
                ));
            }
            let n = (duration / interval).ceil().max(1.0) as usize;
            let kind = match d {
                DetectorSpec::Screenline { s, .. } => {
                    let (y_min, y_max) = bounds(ribbon, *s);
                    let nbins = ((y_max - y_min) / lateral_bin).ceil().max(1.0) as usize;
                    let empty = ScreenStats {
                        count: 0,
                        sum_v: 0.0,
                        sum_inv_v: 0.0,
                        sum_y: 0.0,
                        sum_y2: 0.0,
                        hist: vec![0; nbins],
                    };
                    Kind::Screen {
                        s: *s,
                        y0: y_min,
                        bin: lateral_bin,
                        stats: vec![vec![empty; N_TYPES]; n],
                        measured_count: 0,
                    }
                }
                DetectorSpec::Area { s_from, s_to, .. } => {
                    if s_to.is_nan() || s_from.is_nan() || s_to <= s_from {
                        return Err(format!("detector `{}`: s_to must exceed s_from", d.id()));
                    }
                    Kind::Area {
                        s_from: *s_from,
                        s_to: *s_to,
                        stats: vec![vec![AreaStats::default(); N_TYPES]; n],
                    }
                }
            };
            list.push(Detector {
                id: d.id().to_string(),
                ribbon,
                interval,
                kind,
            });
        }
        Ok(Self { list, warmup })
    }

    /// Observe one step `[t0, t0 + dt]` of motion. `orders` must still be
    /// sorted by `s_prev` (the state before this step's integration), and
    /// `max_step` bounds how far any agent moved.
    pub fn observe(
        &mut self,
        a: &AgentStore,
        orders: &[Vec<u32>],
        t0: f64,
        dt: f64,
        max_step: f64,
    ) {
        for d in &mut self.list {
            let order = &orders[d.ribbon];
            let k_of = |t: f64, n: usize| ((t / d.interval) as usize).min(n - 1);
            match &mut d.kind {
                Kind::Screen {
                    s: sd,
                    y0,
                    bin,
                    stats,
                    measured_count,
                } => {
                    let start = lower_bound(order, &a.s_prev, *sd - max_step);
                    for &i in &order[start..] {
                        let i = i as usize;
                        if a.s_prev[i] >= *sd {
                            break;
                        }
                        if a.s[i] < *sd {
                            continue;
                        }
                        let ds = a.s[i] - a.s_prev[i];
                        let tc = t0 + dt * (*sd - a.s_prev[i]) / ds;
                        let v = ds / dt;
                        let y = a.y[i];
                        let n = stats.len();
                        let st = &mut stats[k_of(tc, n)][usize::from(a.vtype[i])];
                        st.count += 1;
                        st.sum_v += v;
                        st.sum_inv_v += 1.0 / v;
                        st.sum_y += y;
                        st.sum_y2 += y * y;
                        let b =
                            (((y - *y0) / *bin).floor().max(0.0) as usize).min(st.hist.len() - 1);
                        st.hist[b] += 1;
                        if tc >= self.warmup {
                            *measured_count += 1;
                        }
                    }
                }
                Kind::Area {
                    s_from,
                    s_to,
                    stats,
                } => {
                    let start = lower_bound(order, &a.s_prev, *s_from - max_step);
                    let n = stats.len();
                    let k = k_of(t0, n);
                    for &i in &order[start..] {
                        let i = i as usize;
                        let (s0, s1) = (a.s_prev[i], a.s[i]);
                        if s0 > *s_to {
                            break;
                        }
                        let st = &mut stats[k][usize::from(a.vtype[i])];
                        let overlap = (s1.min(*s_to) - s0.max(*s_from)).max(0.0);
                        if s1 > s0 {
                            st.ttd += overlap;
                            st.tts += dt * overlap / (s1 - s0);
                        } else if (*s_from..=*s_to).contains(&s1) {
                            st.tts += dt;
                        }
                        if s0 < *s_from && s1 >= *s_from {
                            st.entered += 1;
                        }
                    }
                }
            }
        }
    }

    /// Crossings after warm-up at a screenline, by id.
    pub fn measured_count(&self, id: &str) -> Option<u64> {
        self.list
            .iter()
            .find(|d| d.id == id)
            .and_then(|d| match &d.kind {
                Kind::Screen { measured_count, .. } => Some(*measured_count),
                Kind::Area { .. } => None,
            })
    }

    /// Whether `id` is a screenline.
    pub fn is_screenline(&self, id: &str) -> bool {
        self.list
            .iter()
            .any(|d| d.id == id && matches!(d.kind, Kind::Screen { .. }))
    }

    /// Detector records, one per detector × interval × vehicle type with data.
    pub fn records(&self, duration: f64) -> Vec<DetectorRecord> {
        let mut out = Vec::new();
        for d in &self.list {
            let n = match &d.kind {
                Kind::Screen { stats, .. } => stats.len(),
                Kind::Area { stats, .. } => stats.len(),
            };
            for k in 0..n {
                let t_start = k as f64 * d.interval;
                let t_end = ((k + 1) as f64 * d.interval).min(duration);
                let span = (t_end - t_start).max(f64::MIN_POSITIVE);
                for t in VehicleType::ALL {
                    let mut r = DetectorRecord {
                        detector_id: d.id.clone(),
                        kind: "screenline",
                        t_start,
                        t_end,
                        in_warmup: t_start < self.warmup,
                        vehicle_type: t.key(),
                        class: t.class().key(),
                        count: 0,
                        time_mean_speed: f64::NAN,
                        space_mean_speed: f64::NAN,
                        density: f64::NAN,
                        flow: f64::NAN,
                        tts: f64::NAN,
                        ttd: f64::NAN,
                        lateral_mean: f64::NAN,
                        lateral_sd: f64::NAN,
                        lateral_y0: f64::NAN,
                        lateral_bin: f64::NAN,
                        lateral_hist: Vec::new(),
                    };
                    match &d.kind {
                        Kind::Screen { y0, bin, stats, .. } => {
                            let st = &stats[k][t.index()];
                            if st.count == 0 {
                                continue;
                            }
                            let c = st.count as f64;
                            let mean_y = st.sum_y / c;
                            r.count = st.count;
                            r.time_mean_speed = st.sum_v / c;
                            r.space_mean_speed = c / st.sum_inv_v;
                            r.flow = c / span;
                            r.lateral_mean = mean_y;
                            r.lateral_sd = (st.sum_y2 / c - mean_y * mean_y).max(0.0).sqrt();
                            r.lateral_y0 = *y0;
                            r.lateral_bin = *bin;
                            r.lateral_hist = st.hist.clone();
                        }
                        Kind::Area {
                            s_from,
                            s_to,
                            stats,
                        } => {
                            let st = &stats[k][t.index()];
                            if st.tts == 0.0 && st.entered == 0 {
                                continue;
                            }
                            let len = s_to - s_from;
                            r.kind = "area";
                            r.count = st.entered;
                            r.tts = st.tts;
                            r.ttd = st.ttd;
                            r.density = st.tts / (len * span);
                            r.flow = st.ttd / (len * span);
                            r.space_mean_speed = if st.tts > 0.0 {
                                st.ttd / st.tts
                            } else {
                                f64::NAN
                            };
                        }
                    }
                    out.push(r);
                }
            }
        }
        out
    }
}
