//! Difficulty algorithms. Each one receives the chain up to and including pindexLast
//! (`chain[i].height == i`) and returns the nBits of the next block.
//! Validation and simulation use exactly these same functions.

use crate::chain::Header;
use crate::target::{from_compact, to_compact};
use primitive_types::U256;

/// Mainnet consensus parameters (src/chainparams.cpp).
pub struct Params {
    pub pow_limit: U256,
    pub target_spacing: i64,
    pub target_timespan: i64,
    pub dgw_height: u32,
}

impl Params {
    pub fn mainnet() -> Self {
        Self {
            pow_limit: U256::MAX >> 20,
            target_spacing: 72,
            target_timespan: 24 * 60 * 60,
            dgw_height: 60,
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
pub struct Dgw {
    pub fix_timespan: bool,
}

impl Dgw {
    pub const CURRENT: Dgw = Dgw { fix_timespan: false };
}

const DGW_PAST_BLOCKS: usize = 24;

impl DifficultyAlgo for Dgw {
    fn name(&self) -> String {
        if self.fix_timespan { "DGW v3 (fixed timespan)".into() } else { "DGW v3".into() }
    }

    fn next_bits(&self, chain: &[Header], params: &Params) -> u32 {
        let last = chain.last().expect("empty chain");
        if (last.height as usize) < DGW_PAST_BLOCKS {
            return to_compact(params.pow_limit);
        }

        let mut avg = U256::zero();
        for n in 1..=DGW_PAST_BLOCKS {
            let target = from_compact(chain[chain.len() - n].bits);
            avg = if n == 1 { target } else { (avg * U256::from(n) + target) / U256::from(n + 1) };
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
