//! Difficulty algorithms. Each one receives the chain up to and including pindexLast
//! (`chain[i].height == i`) and returns the nBits of the next block.
//! Validation and simulation use exactly these same functions.

use crate::chain::Header;
use crate::target::{from_compact, to_compact};
use primitive_types::U256;

/// Mainnet consensus parameters (src/chainparams.cpp).
#[derive(Clone)]
pub struct Params {
    pub pow_limit: U256,
    pub target_spacing: i64,
    pub target_timespan: i64,
    pub dgw_height: u32,
    /// Height of the last block mined under the current rule. Algorithms that need an anchor
    /// (ASERT) start from it, as they would at a hard fork; the simulator sets it to the last
    /// seeded block.
    pub fork_height: u32,
}

impl Params {
    pub fn mainnet() -> Self {
        Self {
            pow_limit: U256::MAX >> 20,
            target_spacing: 72,
            target_timespan: 24 * 60 * 60,
            dgw_height: 60,
            fork_height: u32::MAX,
        }
    }
}

pub trait DifficultyAlgo {
    fn name(&self) -> String;
    fn next_bits(&self, chain: &[Header], params: &Params) -> u32;
}

/// The mainnet rule exactly as in GetNextWorkRequired (src/pow.cpp):
/// Bitcoin's algorithm before nPowDGWHeight, DGW v3 afterwards.
pub struct Mainnet;

impl DifficultyAlgo for Mainnet {
    fn name(&self) -> String {
        "mainnet (BTC < 60, DGW v3)".into()
    }

    fn next_bits(&self, chain: &[Header], params: &Params) -> u32 {
        let last = chain.last().expect("empty chain");
        if last.height + 1 < params.dgw_height {
            btc_next_bits(chain, params)
        } else {
            Dgw::CURRENT.next_bits(chain, params)
        }
    }
}

/// GetNextWorkRequiredBTC without the min-difficulty rule (disabled on mainnet).
/// With a 1200-block interval there is never a retarget below height 60.
fn btc_next_bits(chain: &[Header], params: &Params) -> u32 {
    let last = chain.last().unwrap();
    let interval = params.target_timespan / params.target_spacing;
    assert!((last.height as i64 + 1) % interval != 0, "BTC retarget not supported");
    last.bits
}

/// Dark Gravity Wave v3, a literal port of src/pow.cpp including its biased "average".
/// With `fix_timespan`, the window's 23 measured intervals are compared against 23*T instead of
/// 24*T (the off-by-one flagged in pow.cpp), which is what makes mainnet blocks ~5% slow.
/// With `fix_average`, the 24 targets are averaged with equal weights; pow.cpp's running formula
/// counts the most recent target twice.
pub struct Dgw {
    pub fix_timespan: bool,
    pub fix_average: bool,
}

impl Dgw {
    pub const CURRENT: Dgw = Dgw { fix_timespan: false, fix_average: false };
    /// Both bugs fixed, clamps kept: the minimal change to the current rule.
    pub const CORRECTED: Dgw = Dgw { fix_timespan: true, fix_average: true };
}

const DGW_PAST_BLOCKS: usize = 24;

impl DifficultyAlgo for Dgw {
    fn name(&self) -> String {
        match (self.fix_timespan, self.fix_average) {
            (false, false) => "DGW v3".into(),
            (true, false) => "DGW v3 (fixed timespan)".into(),
            (false, true) => "DGW v3 (fixed average)".into(),
            (true, true) => "DGW v3 (corrected)".into(),
        }
    }

    fn next_bits(&self, chain: &[Header], params: &Params) -> u32 {
        let last = chain.last().expect("empty chain");
        if (last.height as usize) < DGW_PAST_BLOCKS {
            return to_compact(params.pow_limit);
        }

        let mut avg = U256::zero();
        for n in 1..=DGW_PAST_BLOCKS {
            let target = from_compact(chain[chain.len() - n].bits);
            avg = if self.fix_average {
                avg + target / U256::from(DGW_PAST_BLOCKS)
            } else if n == 1 {
                target
            } else {
                (avg * U256::from(n) + target) / U256::from(n + 1)
            };
        }
        let first = &chain[chain.len() - DGW_PAST_BLOCKS];

        let intervals = if self.fix_timespan { DGW_PAST_BLOCKS - 1 } else { DGW_PAST_BLOCKS };
        let target_timespan = intervals as i64 * params.target_spacing;
        let actual = (last.time - first.time).clamp(target_timespan / 3, target_timespan * 3);

        let new = avg * U256::from(actual) / U256::from(target_timespan);
        to_compact(new.min(params.pow_limit))
    }
}

/// LWMA-1 by zawy12 (Bitcoin Gold / Zcash-clone reference implementation):
/// linearly weighted solvetimes over the last `n` blocks, each capped at 6*T,
/// with out-of-order timestamps clamped the "safe" way (previous + 1).
pub struct Lwma {
    pub n: usize,
}

impl DifficultyAlgo for Lwma {
    fn name(&self) -> String {
        format!("LWMA-1 N={}", self.n)
    }

    fn next_bits(&self, chain: &[Header], params: &Params) -> u32 {
        let t = params.target_spacing;
        let n = self.n as i64;
        let k = n * (n + 1) * t / 2;
        let last = chain.last().expect("empty chain");
        if (last.height as i64) < n {
            return to_compact(params.pow_limit);
        }

        let window = &chain[chain.len() - self.n..];
        let mut previous = chain[chain.len() - self.n - 1].time;
        let mut weighted_solvetimes: i64 = 0;
        let mut avg_target = U256::zero();
        for (j, block) in window.iter().enumerate() {
            let this = if block.time > previous { block.time } else { previous + 1 };
            let solvetime = (this - previous).min(6 * t);
            previous = this;
            weighted_solvetimes += solvetime * (j as i64 + 1);
            // Dividing by k here prevents an overflow below.
            avg_target += from_compact(block.bits) / U256::from(n) / U256::from(k);
        }
        let next = avg_target * U256::from(weighted_solvetimes);
        to_compact(next.min(params.pow_limit))
    }
}

/// aserti3-2d, Bitcoin Cash's DAA since November 2020: a port of `GetNextASERTWorkRequired`
/// (Bitcoin ABC / BCHN `src/pow/aserti32d.cpp`) with the same integer arithmetic.
/// Difficulty follows an exponential of how far the chain is ahead of or behind an ideal
/// schedule anchored at the fork block, so it falls with wall-clock time, without a per-block cap.
pub struct Asert {
    /// Mean lifetime in blocks: the half-life is `tau * T * ln 2` seconds.
    pub tau: u32,
}

impl Asert {
    pub fn half_life(&self, params: &Params) -> i64 {
        (self.tau as f64 * params.target_spacing as f64 * std::f64::consts::LN_2).round() as i64
    }
}

impl DifficultyAlgo for Asert {
    fn name(&self) -> String {
        format!("ASERT tau={}", self.tau)
    }

    fn next_bits(&self, chain: &[Header], params: &Params) -> u32 {
        let last = chain.last().expect("empty chain");
        let fork = params.fork_height as usize;
        if last.height < params.fork_height || fork == 0 || fork >= chain.len() {
            return Dgw::CURRENT.next_bits(chain, params);
        }
        let anchor = &chain[fork];
        let anchor_parent_time = chain[fork - 1].time;
        let height_diff = (last.height - anchor.height) as i64;
        let time_diff = last.time - anchor_parent_time;
        let half_life = self.half_life(params);

        // 16.16 fixed-point exponent; Rust's `/` truncates towards zero like C++.
        let exponent = ((time_diff - params.target_spacing * (height_diff + 1)) * 65536) / half_life;
        let mut shifts = exponent >> 16;
        let frac = (exponent & 0xffff) as u128;
        // Cubic approximation of 2^frac on [0, 1), error under 0.013%.
        let factor = 65536 + ((195_766_423_245_049 * frac + 971_821_376 * frac * frac + 5127 * frac * frac * frac + (1u128 << 47)) >> 48);
        let mut next = from_compact(anchor.bits) * U256::from(factor as u64);
        shifts -= 16;
        if shifts <= 0 {
            next = next >> (-shifts).min(255) as usize;
        } else {
            let shifted = next << shifts as usize;
            next = if shifts >= 256 || shifted >> shifts as usize != next { params.pow_limit } else { shifted };
        }
        if next.is_zero() {
            next = U256::one();
        }
        to_compact(next.min(params.pow_limit))
    }
}

/// WTEMA by zawy12 / Jacob Eliosoff: a first-order exponential filter on the last solvetime,
/// `next = prev * (T*(M-1) + st) / (T*M)`. It is the relative form of ASERT and matches it except
/// for very long solvetimes, where it lowers difficulty linearly instead of exponentially.
/// Like ASERT it takes over at the fork block; before that it uses DGW v3.
pub struct Wtema {
    /// Mean lifetime in blocks.
    pub m: i64,
}

impl DifficultyAlgo for Wtema {
    fn name(&self) -> String {
        format!("WTEMA M={}", self.m)
    }

    fn next_bits(&self, chain: &[Header], params: &Params) -> u32 {
        let last = chain.last().expect("empty chain");
        if last.height <= params.fork_height || chain.len() < 2 {
            return Dgw::CURRENT.next_bits(chain, params);
        }
        let t = params.target_spacing;
        let solvetime = last.time - chain[chain.len() - 2].time;
        // A solvetime of -(M-1)*T or less would make the target zero or negative.
        let numerator = (t * (self.m - 1) + solvetime).max(1);
        let next = from_compact(last.bits) * U256::from(numerator) / U256::from(t * self.m);
        to_compact(next.max(U256::one()).min(params.pow_limit))
    }
}

/// Recomputes every block's nBits from its predecessors with the mainnet rule and returns the
/// heights where the result differs from the chain as (height, computed, actual).
pub fn mismatches(headers: &[Header], params: &Params) -> Vec<(usize, u32, u32)> {
    (1..headers.len())
        .filter_map(|h| {
            let computed = Mainnet.next_bits(&headers[..h], params);
            (computed != headers[h].bits).then_some((h, computed, headers[h].bits))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_chain(n: u32, spacing: i64, bits: u32) -> Vec<Header> {
        (0..n).map(|h| Header { height: h, time: 1_000_000 + h as i64 * spacing, bits, n_tx: 1 }).collect()
    }

    #[test]
    fn corrected_dgw_holds_difficulty_on_schedule() {
        let params = Params::mainnet();
        let bits = to_compact(params.pow_limit >> 12);
        let chain = flat_chain(100, params.target_spacing, bits);
        assert_eq!(Dgw::CORRECTED.next_bits(&chain, &params), bits);
        // The current rule reads 24 intervals' worth of target time from 23 intervals.
        assert_ne!(Dgw::CURRENT.next_bits(&chain, &params), bits);
    }

    #[test]
    fn wtema_keeps_difficulty_on_schedule() {
        let mut params = Params::mainnet();
        let bits = to_compact(params.pow_limit >> 12);
        let chain = flat_chain(200, params.target_spacing, bits);
        params.fork_height = 100;
        assert_eq!(Wtema { m: 36 }.next_bits(&chain, &params), bits);
    }

    #[test]
    fn asert_keeps_difficulty_on_schedule() {
        let mut params = Params::mainnet();
        let bits = to_compact(params.pow_limit >> 12);
        let chain = flat_chain(300, params.target_spacing, bits);
        params.fork_height = 100;
        assert_eq!(Asert { tau: 36 }.next_bits(&chain, &params), bits);
    }

    #[test]
    fn asert_halves_difficulty_per_half_life_behind() {
        let mut params = Params::mainnet();
        let bits = to_compact(params.pow_limit >> 12);
        let mut chain = flat_chain(200, params.target_spacing, bits);
        params.fork_height = 100;
        let algo = Asert { tau: 36 };
        // The tip is exactly one half-life late: the target doubles.
        chain.last_mut().unwrap().time += algo.half_life(&params);
        let next = from_compact(algo.next_bits(&chain, &params));
        let ratio = (next >> 192).low_u64() as f64 / (from_compact(bits) >> 192).low_u64() as f64;
        assert!((ratio - 2.0).abs() < 0.001, "ratio {ratio}");
    }
}
