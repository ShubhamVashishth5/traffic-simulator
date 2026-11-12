//! The road network: ribbons and their connections.

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
}

/// A built network.
#[derive(Debug, Clone, PartialEq)]
pub struct Network {
    pub ribbons: Vec<Ribbon>,
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
                    .all(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]) > 1e-6);
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
            if let Some(next) = &r.next {
                let j = *by_id.get(next).ok_or_else(|| NetError::UnknownNext {
                    from: r.id.clone(),
                    to: next.clone(),
                })?;
                ribbons[i].next = Some(j);
            }
        }
        // Chains only (one successor each): walking `next` must terminate.
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
        Ok(Self { ribbons, by_id })
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
        }
    }

    #[test]
    fn builds_chains_and_rejects_errors() {
        let ok = Network::build(&NetworkSpec {
            ribbons: vec![r("a", Some("b")), r("b", None)],
            barriers: vec![],
            objects: vec![],
        })
        .unwrap();
        assert_eq!(ok.ribbon(0).next, Some(1));
        assert_eq!(ok.index("b").unwrap(), 1);
        let cyc = Network::build(&NetworkSpec {
            ribbons: vec![r("a", Some("b")), r("b", Some("a"))],
            barriers: vec![],
            objects: vec![],
        });
        assert!(matches!(cyc, Err(NetError::Cycle(_))));
        let unk = Network::build(&NetworkSpec {
            ribbons: vec![r("a", Some("z"))],
            barriers: vec![],
            objects: vec![],
        });
        assert!(matches!(unk, Err(NetError::UnknownNext { .. })));
        let mut bad = r("a", None);
        bad.width[0].y_max = -4.0;
        assert!(matches!(
            Network::build(&NetworkSpec {
                ribbons: vec![bad],
                barriers: vec![],
                objects: vec![],
            }),
            Err(NetError::BadWidth(_))
        ));
    }
}
