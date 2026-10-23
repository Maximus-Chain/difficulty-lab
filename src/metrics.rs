//! Block-time statistics shared by the real chain and the simulations.

use serde::Serialize;

#[derive(Clone, Copy, Serialize, Default)]
pub struct Metrics {
    pub blocks: f64,
    pub mean_solvetime: f64,
    pub blocks_per_day: f64,
    /// Share of blocks slower than 5x the target spacing (360s for T=72).
    pub pct_over_5t: f64,
    /// Gaps longer than 7 minutes per day.
    pub gaps_over_7min_per_day: f64,
    /// Three consecutive blocks found in under 60 seconds, per day (sliding).
    pub bursts_per_day: f64,
    pub p99_solvetime: f64,
    pub max_solvetime: f64,
    /// Standard deviation of the block-to-block difficulty change, in percent.
    pub difficulty_jitter_pct: f64,
}

/// `times` are block timestamps (the solvetime of block i is times[i] - times[i - 1]).
pub fn compute(times: &[f64], difficulties: &[f64], target_spacing: f64) -> Metrics {
    let solvetimes: Vec<f64> = times.windows(2).map(|w| w[1] - w[0]).collect();
    let n = solvetimes.len() as f64;
    let days = (times[times.len() - 1] - times[0]) / 86_400.0;

    let mut sorted = solvetimes.clone();
    sorted.sort_by(f64::total_cmp);

    let jumps: Vec<f64> = difficulties.windows(2).map(|w| (w[1] / w[0]).ln()).collect();
    let jump_mean = jumps.iter().sum::<f64>() / jumps.len() as f64;
    let jump_var = jumps.iter().map(|j| (j - jump_mean).powi(2)).sum::<f64>() / jumps.len() as f64;

    Metrics {
        blocks: n,
        mean_solvetime: solvetimes.iter().sum::<f64>() / n,
        blocks_per_day: n / days,
        pct_over_5t: 100.0 * solvetimes.iter().filter(|&&s| s > 5.0 * target_spacing).count() as f64 / n,
        gaps_over_7min_per_day: solvetimes.iter().filter(|&&s| s > 420.0).count() as f64 / days,
        bursts_per_day: times.windows(4).filter(|w| w[3] - w[0] < 60.0).count() as f64 / days,
        p99_solvetime: sorted[((n * 0.99) as usize).min(sorted.len() - 1)],
        max_solvetime: sorted[sorted.len() - 1],
        difficulty_jitter_pct: 100.0 * jump_var.sqrt(),
    }
}

/// Mean, min and max of each metric across Monte Carlo runs.
#[derive(Serialize)]
pub struct Spread {
    pub mean: Metrics,
    pub min: Metrics,
    pub max: Metrics,
}

pub fn spread(runs: &[Metrics]) -> Spread {
    let arrays: Vec<[f64; 9]> = runs.iter().map(Metrics::to_array).collect();
    let reduce = |f: fn(&mut dyn Iterator<Item = f64>) -> f64| {
        Metrics::from_array(std::array::from_fn(|i| f(&mut arrays.iter().map(|a| a[i]))))
    };
    let n = runs.len() as f64;
    Spread {
        mean: Metrics::from_array(std::array::from_fn(|i| arrays.iter().map(|a| a[i]).sum::<f64>() / n)),
        min: reduce(|it| it.fold(f64::INFINITY, f64::min)),
        max: reduce(|it| it.fold(f64::NEG_INFINITY, f64::max)),
    }
}

impl Metrics {
    fn to_array(&self) -> [f64; 9] {
        [
            self.blocks,
            self.mean_solvetime,
            self.blocks_per_day,
            self.pct_over_5t,
            self.gaps_over_7min_per_day,
            self.bursts_per_day,
            self.p99_solvetime,
            self.max_solvetime,
            self.difficulty_jitter_pct,
        ]
    }

    fn from_array(a: [f64; 9]) -> Self {
        Self {
            blocks: a[0],
            mean_solvetime: a[1],
            blocks_per_day: a[2],
            pct_over_5t: a[3],
            gaps_over_7min_per_day: a[4],
            bursts_per_day: a[5],
            p99_solvetime: a[6],
            max_solvetime: a[7],
            difficulty_jitter_pct: a[8],
        }
    }
}

/// Fixed-width time bins accumulating solvetime and difficulty so several runs can be averaged.
#[derive(Clone)]
pub struct Bins {
    pub start: f64,
    pub width: f64,
    pub solvetime_sum: Vec<f64>,
    pub difficulty_sum: Vec<f64>,
    pub count: Vec<f64>,
}

impl Bins {
    pub fn new(start: f64, width: f64, n: usize) -> Self {
        Self { start, width, solvetime_sum: vec![0.0; n], difficulty_sum: vec![0.0; n], count: vec![0.0; n] }
    }

    pub fn add(&mut self, time: f64, solvetime: f64, difficulty: f64) {
        let idx = ((time - self.start) / self.width).floor();
        if idx >= 0.0 && (idx as usize) < self.count.len() {
            let i = idx as usize;
            self.solvetime_sum[i] += solvetime;
            self.difficulty_sum[i] += difficulty;
            self.count[i] += 1.0;
        }
    }

    pub fn merge(&mut self, other: &Bins) {
        for i in 0..self.count.len() {
            self.solvetime_sum[i] += other.solvetime_sum[i];
            self.difficulty_sum[i] += other.difficulty_sum[i];
            self.count[i] += other.count[i];
        }
    }

    pub fn mean_solvetime(&self) -> Vec<Option<f64>> {
        self.ratio(&self.solvetime_sum)
    }

    pub fn mean_difficulty(&self) -> Vec<Option<f64>> {
        self.ratio(&self.difficulty_sum)
    }

    fn ratio(&self, sums: &[f64]) -> Vec<Option<f64>> {
        sums.iter().zip(&self.count).map(|(s, c)| (*c > 0.0).then(|| s / c)).collect()
    }
}
