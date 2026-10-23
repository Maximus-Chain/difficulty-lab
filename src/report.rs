//! Runs every scenario and assembles the JSON consumed by the HTML report.

use crate::algo::{self, Asert, DifficultyAlgo, Dgw, Lwma, Params, Wtema};
use crate::chain::Header;
use crate::hashrate::{self, Curve};
use crate::anomalies::biggest_crash;
use crate::metrics::{self, Bins, Metrics, Recovery, RecoverySummary, Spread};
use crate::sim::{self, Miners, Rng, SimBlock};
use crate::target::difficulty;
use crate::par::parallel;
use serde::Serialize;
use serde_json::Value;
use std::sync::atomic::{AtomicUsize, Ordering};

pub struct Options {
    pub replay_runs: usize,
    pub pulse_runs: usize,
    pub hopper_runs: usize,
    /// Blocks per centered window when estimating real hashrate for the replay.
    pub hashrate_window: usize,
    /// First height whose block times are measured; skips the launch period.
    pub start_height: usize,
    pub pulse_factor: f64,
    pub hopper_extra: f64,
    pub hopper_join: f64,
    pub hopper_leave: f64,
    pub crash_runs: usize,
    /// Hashrate multiplier during the collapse (0.01 = 99% of miners leave).
    pub crash_factors: Vec<f64>,
    pub crash_hours: f64,
    /// Output of the `superblocks` subcommand, embedded as-is when available.
    pub superblocks: Option<Value>,
    /// Seconds of wasted hashing after each block (see `sim::run`).
    pub latency: f64,
}

#[derive(Serialize)]
pub struct Report {
    target_spacing: i64,
    tip_height: u32,
    tip_time: i64,
    start_height: usize,
    latency: f64,
    hashrate_window: usize,
    validation: Validation,
    real: Real,
    scenarios: Vec<Scenario>,
    superblocks: Option<Value>,
}

#[derive(Serialize)]
struct Validation {
    checked: usize,
    matches: usize,
}

#[derive(Serialize)]
struct Real {
    metrics: Metrics,
    series: Series,
    histogram: Histogram,
    zoom: Zoom,
    crash: Option<RealCrash>,
}

/// The sharpest real hashrate collapse, for comparison with the crash scenario.
#[derive(Serialize)]
struct RealCrash {
    height: usize,
    time: i64,
    /// Hours since the collapse at which each following block was found.
    hours: Vec<f64>,
}

#[derive(Serialize)]
struct Histogram {
    bin_width: f64,
    /// Percentage of blocks per bin; the last bin collects everything slower.
    real: Vec<f64>,
    /// What a pure exponential with the same mean would give.
    exponential: Vec<f64>,
}

#[derive(Serialize)]
struct Series {
    start: f64,
    width: f64,
    solvetime: Vec<Option<f64>>,
    difficulty: Vec<Option<f64>>,
    hashrate: Vec<Option<f64>>,
    /// Mean number of blocks per bin per run.
    blocks: Vec<f64>,
}

/// Block-by-block detail of a short window, from a single run.
#[derive(Serialize, Default)]
struct Zoom {
    time: Vec<f64>,
    solvetime: Vec<f64>,
    difficulty: Vec<f64>,
    hoppers: Vec<bool>,
}

#[derive(Serialize)]
struct Scenario {
    id: String,
    title: String,
    description: String,
    runs: usize,
    /// Collapse window (start, end) for the crash scenario.
    crash: Option<(f64, f64)>,
    /// Hashrate driving the scenario, sampled at each bin's midpoint.
    hashrate: Vec<f64>,
    algos: Vec<AlgoResult>,
}

#[derive(Serialize)]
struct AlgoResult {
    name: String,
    metrics: Spread,
    series: Series,
    zoom: Zoom,
    recovery: Option<RecoverySummary>,
}

struct Spec<'a> {
    latency: f64,
    id: String,
    title: String,
    description: String,
    seed: &'a [Header],
    end_time: f64,
    miners: Miners,
    runs: usize,
    bins: Bins,
    zoom: (f64, f64),
    crash: Option<(f64, f64)>,
}

fn algos() -> Vec<Box<dyn DifficultyAlgo + Send + Sync>> {
    vec![
        Box::new(Dgw::CURRENT),
        Box::new(Dgw { fix_timespan: true, fix_average: false }),
        Box::new(Dgw::CORRECTED),
        Box::new(Lwma { n: 24 }),
        Box::new(Lwma { n: 45 }),
        Box::new(Lwma { n: 60 }),
        Box::new(Lwma { n: 90 }),
        Box::new(Asert { tau: 24 }),
        Box::new(Asert { tau: 36 }),
        Box::new(Asert { tau: 72 }),
        Box::new(Wtema { m: 36 }),
    ]
}

fn run_scenario(spec: Spec, params: &Params) -> Scenario {
    let algos = algos();
    // The new rules activate right after the seed, as at a hard fork.
    let params = &Params { fork_height: spec.seed.last().expect("empty seed").height, ..params.clone() };
    let jobs = algos.len() * spec.runs;
    eprintln!("scenario {}: {} algorithms x {} runs", spec.id, algos.len(), spec.runs);
    let done = AtomicUsize::new(0);

    let outcomes = parallel(jobs, |job| {
        let (a, run) = (job / spec.runs, job % spec.runs);
        let mut rng = Rng::new(0x5eed_0000 + run as u64);
        let blocks = sim::run(algos[a].as_ref(), params, spec.seed, spec.end_time, &spec.miners, spec.latency, &mut rng);
        let summary = summarize(&blocks, &spec.bins, spec.zoom, run == 0, spec.crash, params);
        eprint!("\r  {}/{jobs}", done.fetch_add(1, Ordering::Relaxed) + 1);
        summary
    });
    eprintln!();

    let mut outcomes = outcomes.into_iter();
    let results = algos
        .iter()
        .map(|algo| {
            let runs: Vec<_> = outcomes.by_ref().take(spec.runs).collect();
            let mut bins = spec.bins.clone();
            for (_, b, _, _) in &runs {
                bins.merge(b);
            }
            let metrics: Vec<Metrics> = runs.iter().map(|(m, _, _, _)| *m).collect();
            let recoveries: Vec<Recovery> = runs.iter().filter_map(|(_, _, _, r)| *r).collect();
            AlgoResult {
                name: algo.name(),
                metrics: metrics::spread(&metrics),
                series: series(&bins, None, spec.runs),
                recovery: (!recoveries.is_empty()).then(|| metrics::summarize_recovery(&recoveries)),
                zoom: runs.into_iter().find_map(|(_, _, z, _)| z).unwrap_or_default(),
            }
        })
        .collect();

    let base = match &spec.miners {
        Miners::Fixed(curve) | Miners::Hoppers { base: curve, .. } => curve,
    };
    let hashrate = (0..spec.bins.count.len())
        .map(|i| base.at(spec.bins.start + (i as f64 + 0.5) * spec.bins.width))
        .collect();

    Scenario {
        id: spec.id,
        title: spec.title,
        description: spec.description,
        runs: spec.runs,
        crash: spec.crash,
        hashrate,
        algos: results,
    }
}

fn summarize(
    blocks: &[SimBlock],
    template: &Bins,
    zoom_window: (f64, f64),
    keep_zoom: bool,
    crash: Option<(f64, f64)>,
    params: &Params,
) -> (Metrics, Bins, Option<Zoom>, Option<Recovery>) {
    let times: Vec<f64> = blocks.iter().map(|b| b.time).collect();
    let diffs: Vec<f64> = blocks.iter().map(|b| b.difficulty).collect();
    let m = metrics::compute(&times, &diffs, params.target_spacing as f64);

    let mut bins = template.clone();
    let mut zoom = Zoom::default();
    for b in blocks {
        bins.add(b.time, b.solvetime, b.difficulty);
        if keep_zoom && b.time >= zoom_window.0 && b.time < zoom_window.1 {
            zoom.time.push(b.time);
            zoom.solvetime.push(b.solvetime);
            zoom.difficulty.push(b.difficulty);
            zoom.hoppers.push(b.hoppers_on);
        }
    }
    let recovery = crash.map(|(start, end)| {
        let solvetimes: Vec<f64> = blocks.iter().map(|b| b.solvetime).collect();
        metrics::recovery(&times, &solvetimes, start, end, params.target_spacing as f64)
    });
    (m, bins, keep_zoom.then_some(zoom), recovery)
}

fn series(bins: &Bins, hashrate: Option<Vec<Option<f64>>>, runs: usize) -> Series {
    Series {
        start: bins.start,
        width: bins.width,
        solvetime: bins.mean_solvetime(),
        difficulty: bins.mean_difficulty(),
        hashrate: hashrate.unwrap_or_default(),
        blocks: bins.count.iter().map(|c| c / runs as f64).collect(),
    }
}

/// Synthetic flat history at exactly T spacing, so pulse/hopper scenarios start in equilibrium.
fn flat_seed(bits: u32, start: i64, params: &Params) -> Vec<Header> {
    (0..200).map(|h| Header { height: h, time: start + h as i64 * params.target_spacing, bits, n_tx: 1 }).collect()
}

pub fn build(headers: &[Header], opts: &Options) -> Report {
    let params = Params::mainnet();
    let t = params.target_spacing as f64;
    let tip = headers.last().unwrap();
    let start = headers[opts.start_height].time as f64;
    let days = ((tip.time as f64 - start) / 86_400.0).ceil() as usize;
    let daily = Bins::new(start, 86_400.0, days);
    let zoom_window = (tip.time as f64 - 86_400.0, tip.time as f64);

    // Real chain, measured exactly like the simulations.
    let measured = &headers[opts.start_height..];
    let estimate = hashrate::estimate(headers, opts.hashrate_window);
    let real_metrics = metrics::compute(
        &measured.iter().map(|h| h.time as f64).collect::<Vec<_>>(),
        &measured.iter().map(|h| difficulty(h.bits)).collect::<Vec<_>>(),
        t,
    );
    let mut real_bins = daily.clone();
    let mut hashrate_bins = daily.clone();
    let mut zoom = Zoom::default();
    for (i, w) in headers.windows(2).enumerate().skip(opts.start_height) {
        let solvetime = (w[1].time - w[0].time) as f64;
        real_bins.add(w[1].time as f64, solvetime, difficulty(w[1].bits));
        // Bins only average two quantities; the difficulty slot carries the hashrate here.
        hashrate_bins.add(w[1].time as f64, 0.0, estimate[i + 1]);
        if (w[1].time as f64) >= zoom_window.0 {
            zoom.time.push(w[1].time as f64);
            zoom.solvetime.push(solvetime);
            zoom.difficulty.push(difficulty(w[1].bits));
            zoom.hoppers.push(false);
        }
    }

    let bin_width = 15.0;
    let n_bins = 40;
    let mut real_hist = vec![0.0; n_bins];
    for w in measured.windows(2) {
        let s = (w[1].time - w[0].time).max(0) as f64;
        real_hist[((s / bin_width) as usize).min(n_bins - 1)] += 1.0;
    }
    let total: f64 = real_hist.iter().sum();
    real_hist.iter_mut().for_each(|c| *c *= 100.0 / total);
    let mean = real_metrics.mean_solvetime;
    let survival = |x: f64| (-x / mean).exp();
    let exponential = (0..n_bins)
        .map(|i| {
            let (lo, hi) = (i as f64 * bin_width, (i + 1) as f64 * bin_width);
            100.0 * if i == n_bins - 1 { survival(lo) } else { survival(lo) - survival(hi) }
        })
        .collect();

    let real = Real {
        metrics: real_metrics,
        series: series(&real_bins, Some(hashrate_bins.mean_difficulty()), 1),
        histogram: Histogram { bin_width, real: real_hist, exponential },
        zoom,
        crash: biggest_crash(headers, opts.start_height).map(|height| {
            // The collapse starts when the last normal block was found.
            let time = headers[height - 1].time;
            let hours = headers[height..]
                .iter()
                .map(|h| (h.time - time) as f64 / 3600.0)
                .take_while(|&h| h <= opts.crash_hours + 6.0)
                .collect();
            RealCrash { height, time, hours }
        }),
    };

    // Equilibrium hashrate for the synthetic scenarios: whatever matches today's difficulty.
    let h0 = hashrate::work(tip.bits) / t;
    let seed = flat_seed(tip.bits, 0, &params);
    let t0 = seed.last().unwrap().time as f64;

    let replay = Spec {
        latency: opts.latency,
        id: "replay".into(),
        title: "Replay of the real hashrate".into(),
        description: format!(
            "Hashrate estimated from the real chain ({}-block centered window) drives each algorithm from height {} to the tip. \
             Assumes miners would have behaved the same, so it is a comparison, not a prediction.",
            opts.hashrate_window, opts.start_height
        ),
        seed: &headers[..=opts.start_height],
        end_time: tip.time as f64,
        miners: Miners::Fixed(Curve::from_chain(headers, opts.hashrate_window)),
        runs: opts.replay_runs,
        bins: daily.clone(),
        zoom: zoom_window,
        crash: None,
    };

    let pulse_start = t0 + 86_400.0;
    let pulse_end = pulse_start + 4.0 * 3_600.0;
    let pulse = Spec {
        latency: opts.latency,
        id: "pulse".into(),
        title: format!("Hashrate pulse: x{} for 4 hours", opts.pulse_factor),
        description: format!(
            "Constant hashrate matching today's difficulty, multiplied by {} for 4 hours and then back. \
             Shows how fast each algorithm catches up when big miners arrive and how long blocks stall when they leave.",
            opts.pulse_factor
        ),
        seed: &seed,
        end_time: t0 + 3.0 * 86_400.0,
        miners: Miners::Fixed(Curve::new(
            vec![f64::MIN, pulse_start, pulse_end],
            vec![h0, h0 * opts.pulse_factor, h0],
        )),
        runs: opts.pulse_runs,
        bins: Bins::new(t0, 600.0, 3 * 144),
        zoom: (pulse_start - 3_600.0, pulse_end + 3.0 * 3_600.0),
        crash: None,
    };

    let hoppers = Spec {
        latency: opts.latency,
        id: "hoppers".into(),
        title: "Profit-switching miners".into(),
        description: format!(
            "Constant base hashrate plus switching miners worth {}x the base, who join while difficulty is below {}x \
             the base-justified level and leave above {}x. This is the classic cause of slow stretches followed by bursts.",
            opts.hopper_extra, opts.hopper_join, opts.hopper_leave
        ),
        seed: &seed,
        end_time: t0 + 14.0 * 86_400.0,
        miners: Miners::Hoppers {
            base: Curve::constant(h0),
            extra: opts.hopper_extra * h0,
            join: opts.hopper_join,
            leave: opts.hopper_leave,
        },
        runs: opts.hopper_runs,
        bins: Bins::new(t0, 3_600.0, 14 * 24),
        zoom: (t0 + 7.0 * 86_400.0, t0 + 7.0 * 86_400.0 + 6.0 * 3_600.0),
        crash: None,
    };

    let crash_start = t0 + 86_400.0;
    let crash_end = crash_start + opts.crash_hours * 3_600.0;
    let crashes = opts.crash_factors.iter().map(|&factor| Spec {
        latency: opts.latency,
        id: format!("crash-{:.0}", (1.0 - factor) * 100.0),
        title: format!("Sudden collapse: hashrate x{factor} for {} hours", opts.crash_hours),
        description: format!(
            "Constant hashrate matching today's difficulty, then {:.0}% of it disappears at once for {} hours before coming back, \
             like the real collapse of October 2025. Measures how fast each rule lets the remaining miners find blocks again, \
             and how big the flood of fast blocks is when the hashrate returns.",
            (1.0 - factor) * 100.0,
            opts.crash_hours
        ),
        seed: &seed,
        end_time: crash_end + 12.0 * 3_600.0,
        miners: Miners::Fixed(Curve::new(vec![f64::MIN, crash_start, crash_end], vec![h0, h0 * factor, h0])),
        runs: opts.crash_runs,
        // 10-minute bins from 2 hours before the collapse to 12 hours after the hashrate returns.
        bins: Bins::new(crash_start - 7_200.0, 600.0, ((opts.crash_hours + 14.0) * 6.0) as usize),
        zoom: (crash_start - 3_600.0, crash_end + 2.0 * 3_600.0),
        crash: Some((crash_start, crash_end)),
    });

    let scenarios = [replay, pulse, hoppers].into_iter().chain(crashes).map(|s| run_scenario(s, &params)).collect();

    Report {
        target_spacing: params.target_spacing,
        tip_height: tip.height,
        tip_time: tip.time,
        start_height: opts.start_height,
        latency: opts.latency,
        hashrate_window: opts.hashrate_window,
        validation: Validation {
            checked: headers.len() - 1,
            matches: headers.len() - 1 - algo::mismatches(headers, &params).len(),
        },
        real,
        scenarios,
        superblocks: opts.superblocks.clone(),
    }
}
