//! The road network: ribbons and their connections.

use crate::junction::{self, Junction};
use crate::ribbon::Ribbon;
use indexmap::IndexMap;
use marg_params::spec::NetworkSpec;

/// Errors building a network.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum NetError {
    #[error("network has no ribbons")]
    Empty,
    #[error("duplicate ribbon id `{0}`")]
    Duplicate(String),
    #[error("ribbon `{0}`: polyline needs at least two distinct consecutive vertices")]
    BadPolyline(String),
    #[error("ribbon `{0}`: width profile needs at least one point with y_min < y_max")]
    BadWidth(String),
    #[error("ribbon `{from}`: next ribbon `{to}` does not exist")]
    UnknownNext { from: String, to: String },
    #[error("ribbons form a cycle through `{0}`")]
    Cycle(String),
    #[error("unknown ribbon `{0}`")]
    UnknownRibbon(String),
    #[error("junction `{0}`: {1}")]
    Junction(String, String),
    #[error("ribbon `{0}`: {1}")]
    Ribbon(String, String),
}

/// Largest distance between a ribbon's reference line and its opposite's,
/// reversed (m).
const OPPOSITE_TOLERANCE: f64 = 0.01;

/// A built network.
#[derive(Debug, Clone, PartialEq)]
pub struct Network {
    pub ribbons: Vec<Ribbon>,
    pub junctions: Vec<Junction>,
    by_id: IndexMap<String, usize>,
}

impl Network {
    /// Validate and build a network from its spec.
    pub fn build(spec: &NetworkSpec) -> Result<Self, NetError> {
        if spec.ribbons.is_empty() {
            return Err(NetError::Empty);
        }
        let mut by_id = IndexMap::new();
        for (i, r) in spec.ribbons.iter().enumerate() {
            if by_id.insert(r.id.clone(), i).is_some() {
                return Err(NetError::Duplicate(r.id.clone()));
            }
            let ok_poly = r.polyline.len() >= 2
                && r.polyline
                    .iter()
                    .all(|p| p[0].is_finite() && p[1].is_finite())
                && r.polyline
                    .windows(2)
                    .all(|w| libm::hypot(w[1][0] - w[0][0], w[1][1] - w[0][1]) > 1e-6);
            if !ok_poly {
                return Err(NetError::BadPolyline(r.id.clone()));
            }
            let ok_width = !r.width.is_empty()
                && r.width.iter().all(|w| {
                    w.s.is_finite()
                        && w.y_min.is_finite()
                        && w.y_max.is_finite()
                        && w.y_min < w.y_max
                });
            if !ok_width {
                return Err(NetError::BadWidth(r.id.clone()));
            }
        }
        let mut ribbons: Vec<Ribbon> = spec.ribbons.iter().map(Ribbon::from_spec).collect();
        for (i, r) in spec.ribbons.iter().enumerate() {
            let bad = |m: String| NetError::Ribbon(r.id.clone(), m);
            if let Some(f) = &r.footpath {
                let ok = f.width.is_finite()
                    && f.width > 0.0
                    && f.from_s.is_none_or(f64::is_finite)
                    && f.to_s.is_none_or(f64::is_finite)
                    && f.bollards
                        .iter()
                        .all(|b| b[0].is_finite() && b[1].is_finite());
                if !ok {
                    return Err(bad(
                        "footpath: width must be positive and stations finite".into()
                    ));
                }
            }
            if r.opposite.len() > 2 {
                return Err(bad("opposite: at most two ribbons".into()));
            }
            for o in &r.opposite {
                let j = *by_id
                    .get(o)
                    .ok_or_else(|| bad(format!("opposite: unknown ribbon `{o}`")))?;
                if j == i || !spec.ribbons[j].opposite.iter().any(|x| *x == r.id) {
                    return Err(bad(format!(
                        "opposite: `{o}` must be another ribbon that names `{}` as its opposite too",
                        r.id
                    )));
                }
                let (a, b) = (&ribbons[i], &ribbons[j]);
                let reversed = a.points().len() == b.points().len()
                    && a.points()
                        .iter()
                        .zip(b.points().iter().rev())
                        .all(|(p, q)| libm::hypot(p[0] - q[0], p[1] - q[1]) <= OPPOSITE_TOLERANCE);
                if !reversed {
                    return Err(bad(format!(
                        "opposite: `{o}`'s polyline must be this ribbon's reversed (to {OPPOSITE_TOLERANCE} m)"
                    )));
                }
                ribbons[i].opposite.push(j);
            }
        }
        for (i, r) in spec.ribbons.iter().enumerate() {
            if let Some(next) = &r.next {
                let j = *by_id.get(next).ok_or_else(|| NetError::UnknownNext {
                    from: r.id.clone(),
                    to: next.clone(),
                })?;
                ribbons[i].next = Some(j);
            }
        }
        // Junctions add the ribbons inside them and the branch tables.
        let mut junctions = Vec::new();
        for (k, js) in spec.junctions.iter().enumerate() {
            if spec.junctions[..k].iter().any(|o| o.id == js.id) {
                return Err(NetError::Junction(js.id.clone(), "duplicate id".into()));
            }
            junctions.push(junction::build(js, k, &mut ribbons, &mut by_id)?);
        }
        for r in 0..ribbons.len() {
            let mut succ: Vec<usize> = ribbons[r].branch.iter().flatten().copied().collect();
            succ.extend(ribbons[r].next);
            succ.sort_unstable();
            succ.dedup();
            for n in succ {
                ribbons[n].preds += 1;
            }
        }
        // `next` chains (every vehicle goes the same way) must terminate;
        // a ring is closed only through branch tables.
        for start in 0..ribbons.len() {
            let mut at = start;
            for _ in 0..=ribbons.len() {
                match ribbons[at].next {
                    Some(n) => at = n,
                    None => break,
                }
                if at == start {
                    return Err(NetError::Cycle(ribbons[start].id.clone()));
                }
            }
        }
        for start in 0..ribbons.len() {
            let mut at = start;
            ribbons[start].ahead = loop {
                if let (Some(j), Some(a)) = (ribbons[at].junction, ribbons[at].arm_in) {
                    break Some((j, a));
                }
                match ribbons[at].next {
                    Some(n) => at = n,
                    None => break None,
                }
            };
        }
        let mut net = Self {
            ribbons,
            junctions,
            by_id,
        };
        net.wrong_way_detours()?;
        Ok(net)
    }

    /// Length along `next` from the start of `r` to the end of the chain or
    /// the first stop line, and where it stops: `(length, junction and arm
    /// reached)`.
    fn run_length(&self, mut r: usize) -> (f64, Option<(usize, usize)>) {
        let mut len = 0.0;
        loop {
            let rib = &self.ribbons[r];
            len += rib.length();
            if let (Some(j), Some(a)) = (rib.junction, rib.arm_in) {
                return (len, Some((j, a)));
            }
            match rib.next {
                Some(n) => r = n,
                None => return (len, None),
            }
        }
    }

    /// The detour a wrong-way path saves at each wrong-way arm (ADR-0020
    /// part 4): the legal route (to the first box downstream with a U-turn
    /// from its approach, the U-turn, then on to where the network ends or
    /// the next stop line) less the path (to where it ends).
    fn wrong_way_detours(&mut self) -> Result<(), NetError> {
        for j in 0..self.junctions.len() {
            for a in 0..self.junctions[j].arms.len() {
                let Some(legal) = self.junctions[j].arms[a].wrong_way_legal else {
                    continue;
                };
                let bad = |m: &str| {
                    NetError::Junction(
                        self.junctions[j].id.clone(),
                        format!("arm `{}`: {m}", self.junctions[j].arms[a].id),
                    )
                };
                let path = self.junctions[j].arms[a]
                    .outbound
                    .expect("checked when built");
                let (wrong, _) = self.run_length(path);
                let mut r = self.junctions[j].arms[legal]
                    .outbound
                    .expect("checked when built");
                let mut legal_len = 0.0;
                let mut found = false;
                for _ in 0..self.junctions.len() + 1 {
                    let (len, at) = self.run_length(r);
                    legal_len += len;
                    let Some((j2, arm2)) = at else { break };
                    let jn = &self.junctions[j2];
                    if let Some(m) = jn.movement_of[arm2][arm2]
                        && jn.kind == marg_params::spec::JunctionKind::Box
                    {
                        let first = jn.movements[m].first;
                        legal_len += self.ribbons[first].length();
                        if let Some(out) = jn.arms[arm2].outbound {
                            legal_len += self.run_length(out).0;
                        }
                        found = true;
                        break;
                    }
                    // Straight on through any other junction.
                    let through = jn.movement_of[arm2]
                        .iter()
                        .flatten()
                        .find(|&&m| jn.movements[m].turn == crate::junction::Turn::Through)
                        .or_else(|| jn.movement_of[arm2].iter().flatten().next());
                    match through {
                        Some(&m) => {
                            legal_len += self.ribbons[jn.movements[m].first].length();
                            let to = jn.movements[m].to;
                            match jn.arms[to].outbound {
                                Some(out) => r = out,
                                None => break,
                            }
                        }
                        None => break,
                    }
                }
                if !found {
                    return Err(bad(
                        "its legal route reaches no median opening with a U-turn from its approach",
                    ));
                }
                self.junctions[j].wrong_way_detour[a] = (legal_len - wrong).max(0.0);
            }
        }
        Ok(())
    }

    /// Index of a ribbon by id.
    pub fn index(&self, id: &str) -> Result<usize, NetError> {
        self.by_id
            .get(id)
            .copied()
            .ok_or_else(|| NetError::UnknownRibbon(id.to_string()))
    }

    pub fn ribbon(&self, i: usize) -> &Ribbon {
        &self.ribbons[i]
    }

    pub fn len(&self) -> usize {
        self.ribbons.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ribbons.is_empty()
    }

    /// The ribbons that lead onto `r`, in index order.
    pub fn predecessors(&self, r: usize) -> impl Iterator<Item = usize> + '_ {
        (0..self.ribbons.len()).filter(move |&q| {
            self.ribbons[q].next == Some(r) || self.ribbons[q].branch.contains(&Some(r))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use marg_params::spec::{RibbonSpec, WidthPoint};

    fn r(id: &str, next: Option<&str>) -> RibbonSpec {
        RibbonSpec {
            id: id.into(),
            polyline: vec![[0.0, 0.0], [100.0, 0.0]],
            width: vec![WidthPoint {
                s: 0.0,
                y_min: -3.5,
                y_max: 3.5,
            }],
            next: next.map(Into::into),
            footpath: None,
            opposite: Vec::new(),
        }
    }

    #[test]
    fn builds_chains_and_rejects_errors() {
        let ok = Network::build(&NetworkSpec {
            ribbons: vec![r("a", Some("b")), r("b", None)],
            barriers: vec![],
            objects: vec![],
            parking: vec![],
            attractors: vec![],
            bus_stops: vec![],
            pothole_fields: vec![],
            humps: vec![],
            surfaces: vec![],
            waterlogging: vec![],
            pedestrian_crossings: vec![],
            junctions: vec![],
        })
        .unwrap();
        assert_eq!(ok.ribbon(0).next, Some(1));
        assert_eq!(ok.index("b").unwrap(), 1);
        let cyc = Network::build(&NetworkSpec {
            ribbons: vec![r("a", Some("b")), r("b", Some("a"))],
            barriers: vec![],
            objects: vec![],
            parking: vec![],
            attractors: vec![],
            bus_stops: vec![],
            pothole_fields: vec![],
            humps: vec![],
            surfaces: vec![],
            waterlogging: vec![],
            pedestrian_crossings: vec![],
            junctions: vec![],
        });
        assert!(matches!(cyc, Err(NetError::Cycle(_))));
        let unk = Network::build(&NetworkSpec {
            ribbons: vec![r("a", Some("z"))],
            barriers: vec![],
            objects: vec![],
            parking: vec![],
            attractors: vec![],
            bus_stops: vec![],
            pothole_fields: vec![],
            humps: vec![],
            surfaces: vec![],
            waterlogging: vec![],
            pedestrian_crossings: vec![],
            junctions: vec![],
        });
        assert!(matches!(unk, Err(NetError::UnknownNext { .. })));
        let mut bad = r("a", None);
        bad.width[0].y_max = -4.0;
        assert!(matches!(
            Network::build(&NetworkSpec {
                ribbons: vec![bad],
                barriers: vec![],
                objects: vec![],
                parking: vec![],
                attractors: vec![],
                bus_stops: vec![],
                pothole_fields: vec![],
                humps: vec![],
                surfaces: vec![],
                waterlogging: vec![],
                pedestrian_crossings: vec![],
                junctions: vec![],
            }),
            Err(NetError::BadWidth(_))
        ));
    }
}
