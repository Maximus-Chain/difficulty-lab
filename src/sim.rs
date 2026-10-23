//! Monte Carlo block production: given a difficulty algorithm and a hashrate model,
//! draw exponential solvetimes and build a simulated chain block by block.

use crate::algo::{DifficultyAlgo, Params};
use crate::chain::Header;
use crate::hashrate::{Curve, HASHES_PER_DIFFICULTY};
use crate::target::difficulty;

/// SplitMix64: tiny, fast and reproducible from a seed, which is all a simulation needs.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in (0, 1].
    pub fn uniform(&mut self) -> f64 {
        ((self.next_u64() >> 11) + 1) as f64 / (1u64 << 53) as f64
    }

    /// Exponential with mean 1: the number of "expected blocks of work" until a block is found.
    pub fn exp(&mut self) -> f64 {
        -self.uniform().ln()
    }
}

/// Who is mining, and how much, at the moment a new block template is handed out.
pub enum Miners {
    /// Hashrate follows a fixed curve over time, ignoring difficulty.
    Fixed(Curve),
    /// A base curve plus profit-switching miners that add `extra` hashrate while difficulty
    /// is below `join * reference` and leave once it rises above `leave * reference`,
    /// where `reference` is the difficulty the base hashrate alone would justify.
    Hoppers { base: Curve, extra: f64, join: f64, leave: f64 },
}

pub struct SimBlock {
    pub time: f64,
    pub solvetime: f64,
    pub difficulty: f64,
    pub hoppers_on: bool,
}

/// Extends `seed` (real or synthetic history the algorithm needs as its window) with simulated
/// blocks until `end_time`. Timestamps are honest; the header stores them rounded down to
/// whole seconds like a real node would.
///
/// `latency` is how long miners keep hashing on the old tip after a block is found (propagation
/// plus pool template switch). That work is wasted, which is why real chains almost never show
/// 0-2 second blocks even though an exponential distribution says they should be common.
pub fn run(
    algo: &dyn DifficultyAlgo,
    params: &Params,
    seed: &[Header],
    end_time: f64,
    miners: &Miners,
    latency: f64,
    rng: &mut Rng,
) -> Vec<SimBlock> {
    let mut chain = seed.to_vec();
    let mut time = seed.last().expect("empty seed").time as f64;
    let mut hoppers_on = false;
    let mut blocks = Vec::new();

    while time < end_time {
        let bits = algo.next_bits(&chain, params);
        let diff = difficulty(bits);
        let work = rng.exp() * diff * HASHES_PER_DIFFICULTY;

        let solvetime = match miners {
            Miners::Fixed(curve) => latency + curve.solve_time(time + latency, work, 0.0),
            Miners::Hoppers { base, extra, join, leave } => {
                let reference = base.at(time) * params.target_spacing as f64 / HASHES_PER_DIFFICULTY;
                if diff < join * reference {
                    hoppers_on = true;
                } else if diff > leave * reference {
                    hoppers_on = false;
                }
                latency + base.solve_time(time + latency, work, if hoppers_on { *extra } else { 0.0 })
            }
        };

        time += solvetime;
        chain.push(Header { height: chain.len() as u32, time: time.floor() as i64, bits, n_tx: 1 });
        blocks.push(SimBlock { time, solvetime, difficulty: diff, hoppers_on });
    }
    blocks
}
