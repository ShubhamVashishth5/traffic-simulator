//! Tracks which part of a follower's lateral extent is already blocked by a
//! nearer leader, so leaders hidden behind it can be skipped ("shadowing").
//! Only the nearest leaders in each lateral band can constrain the IDM, so
//! once the follower's whole width is covered the leader scan can stop.

/// Maximum number of disjoint covered pieces tracked.
const MAX_PIECES: usize = 8;

/// Union of covered sub-intervals of `[lo, hi]`.
#[derive(Debug, Clone)]
pub struct Coverage {
    lo: f64,
    hi: f64,
    pieces: [(f64, f64); MAX_PIECES],
    n: usize,
}

impl Coverage {
    pub fn new(lo: f64, hi: f64) -> Self {
        Self {
            lo,
            hi,
            pieces: [(0.0, 0.0); MAX_PIECES],
            n: 0,
        }
    }

    /// Add `[a, b]` (clipped to the tracked extent). If there is no room for
    /// another disjoint piece it is ignored, which only makes the scan longer.
    pub fn add(&mut self, a: f64, b: f64) {
        let (mut a, mut b) = (a.max(self.lo), b.min(self.hi));
        if a >= b {
            return;
        }
        // Merge every overlapping or touching piece into [a, b].
        let mut i = 0;
        while i < self.n {
            let (pa, pb) = self.pieces[i];
            if pb >= a && pa <= b {
                a = a.min(pa);
                b = b.max(pb);
                self.n -= 1;
                self.pieces[i] = self.pieces[self.n];
            } else {
                i += 1;
            }
        }
        if self.n < MAX_PIECES {
            self.pieces[self.n] = (a, b);
            self.n += 1;
        }
    }

    /// Whether the whole extent is covered.
    pub fn full(&self) -> bool {
        self.pieces[..self.n]
            .iter()
            .any(|&(a, b)| a <= self.lo && b >= self.hi)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merges_pieces() {
        let mut c = Coverage::new(0.0, 1.0);
        c.add(-1.0, 0.3);
        assert!(!c.full());
        c.add(0.6, 2.0);
        assert!(!c.full());
        c.add(0.2, 0.7);
        assert!(c.full());
    }

    #[test]
    fn ignores_outside() {
        let mut c = Coverage::new(0.0, 1.0);
        c.add(2.0, 3.0);
        c.add(-3.0, -2.0);
        assert!(!c.full());
    }
}
