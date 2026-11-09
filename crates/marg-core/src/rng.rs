//! Counter-based random streams (ADR-0003).
//!
//! Every random number in the simulator comes from a [`StreamRng`] addressed by
//! `(master_seed, StreamKind, entity, counter)`. The generator is Philox4x32-10
//! (Salmon et al. 2011, "Parallel random numbers: as easy as 1, 2, 3"), a
//! counter-based generator: the output for a given address does not depend on
//! how many other numbers were drawn before it, on spawn order, or on the number
//! of threads. That gives
//!
//! - bitwise-reproducible runs on any machine (integer arithmetic only), and
//! - common random numbers across scenario arms: agent 7 gets the same draws in
//!   every arm of a twin experiment, whatever else differs.
//!
//! There is no global RNG anywhere in the crate.

/// What a stream is used for. Each kind gets its own Philox key, so streams of
/// different kinds never overlap even for the same entity id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum StreamKind {
    /// Arrival processes. Entity = entry × vehicle type.
    Demand = 1,
    /// Per-agent parameter draws at spawn. Entity = agent id.
    AgentDraws = 2,
    /// Per-agent stochastic decisions during the run. Entity = agent id.
    Decisions = 3,
    /// Environment processes (signal outages, weather). Entity = object id.
    Environment = 4,
    /// Road-object placement and object events. Entity = object id.
    Objects = 5,
    /// Pedestrian and animal agents. Entity = agent id.
    Pedestrians = 6,
}

const PHILOX_M0: u32 = 0xD251_1F53;
const PHILOX_M1: u32 = 0xCD9E_8D57;
const PHILOX_W0: u32 = 0x9E37_79B9;
const PHILOX_W1: u32 = 0xBB67_AE85;

#[inline(always)]
fn mulhilo(a: u32, b: u32) -> (u32, u32) {
    let p = u64::from(a) * u64::from(b);
    ((p >> 32) as u32, p as u32)
}

#[inline(always)]
fn philox_round(ctr: [u32; 4], key: [u32; 2]) -> [u32; 4] {
    let (hi0, lo0) = mulhilo(PHILOX_M0, ctr[0]);
    let (hi1, lo1) = mulhilo(PHILOX_M1, ctr[2]);
    [hi1 ^ ctr[1] ^ key[0], lo1, hi0 ^ ctr[3] ^ key[1], lo0]
}

/// Philox4x32 with 10 rounds: one 128-bit output block per 128-bit counter.
#[inline]
pub fn philox4x32_10(mut ctr: [u32; 4], mut key: [u32; 2]) -> [u32; 4] {
    ctr = philox_round(ctr, key);
    for _ in 1..10 {
        key[0] = key[0].wrapping_add(PHILOX_W0);
        key[1] = key[1].wrapping_add(PHILOX_W1);
        ctr = philox_round(ctr, key);
    }
    ctr
}

/// SplitMix64 finaliser, used only to turn `(seed, kind)` into a Philox key.
#[inline]
fn splitmix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// The Philox key for a stream kind under a master seed.
#[inline]
pub fn stream_key(master_seed: u64, kind: StreamKind) -> [u32; 2] {
    let k = splitmix64(master_seed ^ splitmix64(kind as u64));
    [k as u32, (k >> 32) as u32]
}

/// One random stream: a fixed key and entity, and a moving 64-bit counter.
///
/// Each counter value yields one 128-bit block (two `u64`). Use
/// [`StreamRng::at`] to jump to any counter position in O(1), e.g. to make a
/// decision at simulation step `k` reproducible regardless of what was drawn
/// before.
#[derive(Debug, Clone)]
pub struct StreamRng {
    key: [u32; 2],
    entity: u64,
    counter: u64,
    buf: [u32; 4],
    /// Index of the next unused `u64` in `buf` (0, 1, or 2 = exhausted).
    next: u8,
}

impl StreamRng {
    /// A stream for `entity` of `kind`, starting at counter 0.
    pub fn new(master_seed: u64, kind: StreamKind, entity: u64) -> Self {
        Self::at(master_seed, kind, entity, 0)
    }

    /// A stream for `entity` of `kind`, starting at block `counter`.
    pub fn at(master_seed: u64, kind: StreamKind, entity: u64, counter: u64) -> Self {
        Self {
            key: stream_key(master_seed, kind),
            entity,
            counter,
            buf: [0; 4],
            next: 2,
        }
    }

    /// The counter of the next block to be generated.
    pub fn counter(&self) -> u64 {
        self.counter
    }

    #[inline]
    fn refill(&mut self) {
        let ctr = [
            self.entity as u32,
            (self.entity >> 32) as u32,
            self.counter as u32,
            (self.counter >> 32) as u32,
        ];
        self.buf = philox4x32_10(ctr, self.key);
        self.counter = self.counter.wrapping_add(1);
        self.next = 0;
    }

    /// Next 64 random bits.
    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        if self.next >= 2 {
            self.refill();
        }
        let i = usize::from(self.next) * 2;
        self.next += 1;
        u64::from(self.buf[i]) | (u64::from(self.buf[i + 1]) << 32)
    }

    /// Uniform in `[0, 1)` with 53 bits of resolution.
    #[inline]
    pub fn uniform(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Uniform in the open interval `(0, 1)`; safe to take `ln` of.
    #[inline]
    pub fn uniform_open(&mut self) -> f64 {
        ((self.next_u64() >> 12) as f64 + 0.5) * (1.0 / (1u64 << 52) as f64)
    }

    /// Standard normal draw (Box–Muller with `libm`, so it is identical on
    /// every platform; one of the pair is discarded to keep draws addressable).
    #[inline]
    pub fn standard_normal(&mut self) -> f64 {
        let u1 = self.uniform_open();
        let u2 = self.uniform();
        libm::sqrt(-2.0 * libm::log(u1)) * libm::cos(std::f64::consts::TAU * u2)
    }
}

impl marg_params::dist::Rng64 for StreamRng {
    #[inline]
    fn uniform(&mut self) -> f64 {
        StreamRng::uniform(self)
    }
    #[inline]
    fn uniform_open(&mut self) -> f64 {
        StreamRng::uniform_open(self)
    }
    #[inline]
    fn standard_normal(&mut self) -> f64 {
        StreamRng::standard_normal(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Known-answer tests from Random123's `kat_vectors` for philox4x32 with 10 rounds.
    #[test]
    fn philox_known_answers() {
        assert_eq!(
            philox4x32_10([0, 0, 0, 0], [0, 0]),
            [0x6627_e8d5, 0xe169_c58d, 0xbc57_ac4c, 0x9b00_dbd8]
        );
        assert_eq!(
            philox4x32_10([u32::MAX; 4], [u32::MAX; 2]),
            [0x408f_276d, 0x41c8_3b0e, 0xa20b_c7c6, 0x6d54_51fd]
        );
        assert_eq!(
            philox4x32_10(
                [0x243f_6a88, 0x85a3_08d3, 0x1319_8a2e, 0x0370_7344],
                [0xa409_3822, 0x299f_31d0]
            ),
            [0xd16c_fe09, 0x94fd_cceb, 0x5001_e420, 0x2412_6ea1]
        );
    }

    #[test]
    fn same_address_same_numbers() {
        let mut a = StreamRng::new(42, StreamKind::AgentDraws, 7);
        let mut b = StreamRng::new(42, StreamKind::AgentDraws, 7);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn jump_matches_sequential() {
        let mut seq = StreamRng::new(1, StreamKind::Decisions, 3);
        for _ in 0..10 {
            seq.next_u64();
            seq.next_u64();
        }
        let mut jumped = StreamRng::at(1, StreamKind::Decisions, 3, 10);
        assert_eq!(seq.next_u64(), jumped.next_u64());
    }

    #[test]
    fn kinds_and_entities_are_independent() {
        let a = StreamRng::new(9, StreamKind::AgentDraws, 1).next_u64();
        let b = StreamRng::new(9, StreamKind::Decisions, 1).next_u64();
        let c = StreamRng::new(9, StreamKind::AgentDraws, 2).next_u64();
        let d = StreamRng::new(10, StreamKind::AgentDraws, 1).next_u64();
        assert!(a != b && a != c && a != d && b != c);
    }

    #[test]
    fn uniform_moments() {
        let mut r = StreamRng::new(5, StreamKind::Demand, 0);
        let n = 200_000;
        let (mut s, mut s2) = (0.0, 0.0);
        for _ in 0..n {
            let u = r.uniform();
            assert!((0.0..1.0).contains(&u));
            s += u;
            s2 += u * u;
        }
        let mean = s / n as f64;
        let var = s2 / n as f64 - mean * mean;
        assert!((mean - 0.5).abs() < 0.003, "mean {mean}");
        assert!((var - 1.0 / 12.0).abs() < 0.002, "var {var}");
    }

    #[test]
    fn normal_moments() {
        let mut r = StreamRng::new(5, StreamKind::AgentDraws, 11);
        let n = 200_000;
        let (mut s, mut s2) = (0.0, 0.0);
        for _ in 0..n {
            let z = r.standard_normal();
            s += z;
            s2 += z * z;
        }
        let mean = s / n as f64;
        let var = s2 / n as f64 - mean * mean;
        assert!(mean.abs() < 0.01, "mean {mean}");
        assert!((var - 1.0).abs() < 0.015, "var {var}");
    }

    proptest::proptest! {
        /// Draws for an entity do not depend on how many other entities exist
        /// or in which order their streams are consumed.
        #[test]
        fn entity_draws_independent_of_population(n_others in 0usize..200, seed in 0u64..1000) {
            let mut alone = StreamRng::new(seed, StreamKind::AgentDraws, 7);
            let expected: Vec<u64> = (0..8).map(|_| alone.next_u64()).collect();
            for other in 0..n_others as u64 {
                let mut o = StreamRng::new(seed, StreamKind::AgentDraws, 1000 + other);
                o.next_u64();
            }
            let mut again = StreamRng::new(seed, StreamKind::AgentDraws, 7);
            let got: Vec<u64> = (0..8).map(|_| again.next_u64()).collect();
            proptest::prop_assert_eq!(expected, got);
        }
    }
}
