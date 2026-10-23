//! Hashrate estimation from the real chain and hashrate curves used to drive simulations.

use crate::chain::Header;
use crate::target::difficulty;

/// Expected hashes to find a block at difficulty 1.
pub const HASHES_PER_DIFFICULTY: f64 = 4_294_967_296.0;

pub fn work(bits: u32) -> f64 {
    difficulty(bits) * HASHES_PER_DIFFICULTY
}

/// Centered estimate for every height: work of the `window` blocks around it divided by
/// the time they took. A single block is far too noisy (exponential solvetimes), so this
/// trades time resolution for accuracy. Edges reuse the nearest full window.
pub fn estimate(headers: &[Header], window: usize) -> Vec<f64> {
    let half = window / 2;
    let mut cumulative_work = Vec::with_capacity(headers.len() + 1);
    cumulative_work.push(0.0);
    for h in headers {
        cumulative_work.push(cumulative_work.last().unwrap() + work(h.bits));
    }

    (0..headers.len())
        .map(|i| {
            let lo = i.saturating_sub(half).min(headers.len() - 1 - window);
            let hi = lo + window;
            let elapsed = (headers[hi].time - headers[lo].time).max(1) as f64;
            (cumulative_work[hi + 1] - cumulative_work[lo + 1]) / elapsed
        })
        .collect()
}

/// Piecewise-constant hashrate over time: `values[i]` applies from `times[i]` to `times[i + 1]`,
/// and the last value applies forever.
pub struct Curve {
    times: Vec<f64>,
    values: Vec<f64>,
}

impl Curve {
    pub fn new(times: Vec<f64>, values: Vec<f64>) -> Self {
        assert_eq!(times.len(), values.len());
        assert!(times.windows(2).all(|w| w[0] <= w[1]), "curve times must be sorted");
        Self { times, values }
    }

    pub fn constant(value: f64) -> Self {
        Self::new(vec![f64::MIN], vec![value])
    }

    /// Real hashrate replay: the estimate at each block holds until the next block's timestamp.
    pub fn from_chain(headers: &[Header], window: usize) -> Self {
        let values = estimate(headers, window);
        let mut times: Vec<f64> = headers.iter().map(|h| h.time as f64).collect();
        // Real timestamps are not monotonic; a running max keeps the curve well defined.
        for i in 1..times.len() {
            times[i] = times[i].max(times[i - 1]);
        }
        Self::new(times, values)
    }

    /// Pointwise sum of two curves.
    pub fn add(&self, other: &Curve) -> Curve {
        let mut times: Vec<f64> = self.times.iter().chain(&other.times).copied().collect();
        times.sort_by(f64::total_cmp);
        times.dedup();
        let values = times.iter().map(|&t| self.at(t) + other.at(t)).collect();
        Curve::new(times, values)
    }

    /// Hashes performed between `from` and `to`.
    pub fn integral(&self, from: f64, to: f64) -> f64 {
        let mut total = 0.0;
        for (i, &start) in self.times.iter().enumerate() {
            let end = self.times.get(i + 1).copied().unwrap_or(f64::INFINITY);
            let (lo, hi) = (start.max(from), end.min(to));
            if hi > lo {
                total += self.values[i] * (hi - lo);
            }
        }
        total
    }

    pub fn at(&self, time: f64) -> f64 {
        let idx = self.times.partition_point(|&t| t <= time).saturating_sub(1);
        self.values[idx]
    }

    /// Time needed, starting at `start`, to perform `work` hashes with this curve plus a
    /// constant `extra` hashrate. Hashrate changes during the block are integrated exactly.
    pub fn solve_time(&self, start: f64, mut work: f64, extra: f64) -> f64 {
        let mut idx = self.times.partition_point(|&t| t <= start).saturating_sub(1);
        let mut now = start;
        loop {
            let rate = self.values[idx] + extra;
            let segment_end = self.times.get(idx + 1).copied().unwrap_or(f64::INFINITY);
            let available = rate * (segment_end - now);
            if work <= available {
                return now + work / rate - start;
            }
            work -= available;
            now = segment_end;
            idx += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Curve;

    #[test]
    fn curve_sum_and_integral() {
        let a = Curve::new(vec![f64::MIN, 10.0], vec![1.0, 3.0]);
        let b = Curve::new(vec![f64::MIN, 5.0], vec![2.0, 0.0]);
        let sum = a.add(&b);
        assert_eq!((sum.at(0.0), sum.at(7.0), sum.at(20.0)), (3.0, 1.0, 3.0));
        assert_eq!(sum.integral(0.0, 20.0), 3.0 * 5.0 + 1.0 * 5.0 + 3.0 * 10.0);
    }
}
