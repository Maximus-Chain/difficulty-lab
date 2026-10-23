//! Runs every scenario and assembles the JSON consumed by the HTML report.

use crate::algo::{self, DifficultyAlgo, Dgw, Lwma, Params};
use crate::chain::Header;
use crate::hashrate::{self, Curve};
use crate::metrics::{self, Bins, Metrics, Spread};
use crate::sim::{self, Miners, Rng, SimBlock};
use crate::target::difficulty;
use serde::Serialize;
use std::sync::Mutex;
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
    id: &'static str,
    title: String,
    description: String,
    runs: usize,
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
}

struct Spec<'a> {
    latency: f64,
    id: &'static str,
    title: String,
    description: String,
    seed: &'a [Header],
    end_time: f64,
    miners: Miners,
    runs: usize,
    bins: Bins,
    zoom: (f64, f64),
}

fn algos() -> Vec<Box<dyn DifficultyAlgo + Send + Sync>> {
    vec![
        Box::new(Dgw::CURRENT),
        Box::new(Dgw { fix_timespan: true }),
        Box::new(Lwma { n: 24 }),
        Box::new(Lwma { n: 45 }),
        Box::new(Lwma { n: 60 }),
        Box::new(Lwma { n: 90 }),
    ]
}

/// Runs `f(0..jobs)` on all cores and returns the results in job order.
fn parallel<R: Send>(jobs: usize, f: impl Fn(usize) -> R + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let results = Mutex::new((0..jobs).map(|_| None).collect::<Vec<Option<R>>>());
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= jobs {
                        break;
                    }
                    let r = f(i);
                    results.lock().unwrap()[i] = Some(r);
                }
            });
        }
    });
    results.into_inner().unwrap().into_iter().map(Option::unwrap).collect()
}

fn run_scenario(spec: Spec, params: &Params) -> Scenario {
    let algos = algos();
    let jobs = algos.len() * spec.runs;
    eprintln!("scenario {}: {} algorithms x {} runs", spec.id, algos.len(), spec.runs);
    let done = AtomicUsize::new(0);

    let outcomes = parallel(jobs, |job| {
        let (a, run) = (job / spec.runs, job % spec.runs);
        let mut rng = Rng::new(0x5eed_0000 + run as u64);
        let blocks = sim::run(algos[a].as_ref(), params, spec.seed, spec.end_time, &spec.miners, spec.latency, &mut rng);
        let summary = summarize(&blocks, &spec.bins, spec.zoom, run == 0, params);
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
            for (_, b, _) in &runs {
                bins.merge(b);
            }
            let metrics: Vec<Metrics> = runs.iter().map(|(m, _, _)| *m).collect();
            AlgoResult {
                name: algo.name(),
                metrics: metrics::spread(&metrics),
                series: series(&bins, None),
                zoom: runs.into_iter().find_map(|(_, _, z)| z).unwrap_or_default(),
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
        hashrate,
        algos: results,
    }
}

fn summarize(
    blocks: &[SimBlock],
    template: &Bins,
    zoom_window: (f64, f64),
    keep_zoom: bool,
    params: &Params,
) -> (Metrics, Bins, Option<Zoom>) {
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
    (m, bins, keep_zoom.then_some(zoom))
}

fn series(bins: &Bins, hashrate: Option<Vec<Option<f64>>>) -> Series {
    Series {
        start: bins.start,
        width: bins.width,
        solvetime: bins.mean_solvetime(),
        difficulty: bins.mean_difficulty(),
        hashrate: hashrate.unwrap_or_default(),
    }
}

/// Synthetic flat history at exactly T spacing, so pulse/hopper scenarios start in equilibrium.
fn flat_seed(bits: u32, start: i64, params: &Params) -> Vec<Header> {
    (0..200).map(|h| Header { height: h, time: start + h as i64 * params.target_spacing, bits }).collect()
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
        series: series(&real_bins, Some(hashrate_bins.mean_difficulty())),
        histogram: Histogram { bin_width, real: real_hist, exponential },
        zoom,
    };

    // Equilibrium hashrate for the synthetic scenarios: whatever matches today's difficulty.
    let h0 = hashrate::work(tip.bits) / t;
    let seed = flat_seed(tip.bits, 0, &params);
    let t0 = seed.last().unwrap().time as f64;

    let replay = Spec {
        latency: opts.latency,
        id: "replay",
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
    };

    let pulse_start = t0 + 86_400.0;
    let pulse_end = pulse_start + 4.0 * 3_600.0;
    let pulse = Spec {
        latency: opts.latency,
        id: "pulse",
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
    };

    let hoppers = Spec {
        latency: opts.latency,
        id: "hoppers",
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
    };

    let scenarios = [replay, pulse, hoppers].into_iter().map(|s| run_scenario(s, &params)).collect();

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
    }
}
