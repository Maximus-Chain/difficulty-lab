//! The ASERT decision page: ASERT tau=24 against DGW v3, LWMA-1 N=45 and longer ASERT half-lives,
//! in hashrate collapses of every size, and replayed over the real catalogue of collapses so the
//! comparison is weighted by what actually happens on Maximus.
//!
//! The JSON also carries the collapse catalogue (`crashes`), who mines (`miners`) and the
//! merge-mining check (`mergemining`), so one page tells the whole story.

use crate::algo::{Asert, Dgw, DifficultyAlgo, Lwma, Params};
use crate::chain::Header;
use crate::crashes::Crashes;
use crate::hashrate::{self, Curve};
use crate::metrics::{self, Metrics, Stat};
use crate::par::parallel;
use crate::report::{Algos, flat_seed};
use crate::sim::{self, Miners, Rng};
use crate::switching::Switching;
use serde::Serialize;
use serde_json::Value;

const HOUR: f64 = 3_600.0;
pub const DEPTHS: [f64; 4] = [0.5, 0.8, 0.95, 0.99];
pub const DURATIONS: [f64; 3] = [2.0, 6.0, 24.0];

pub fn algos() -> Algos {
    vec![
        Box::new(Dgw::CURRENT),
        Box::new(Lwma { n: 45 }),
        Box::new(Asert { tau: 24 }),
        Box::new(Asert { tau: 36 }),
        Box::new(Asert { tau: 48 }),
    ]
}

pub struct Options {
    pub grid_runs: usize,
    pub catalogue_runs: usize,
    pub replay_runs: usize,
    pub switching_runs: usize,
    pub start_height: usize,
    pub hashrate_window: usize,
    pub latency: f64,
}

/// One collapse, as one run of one algorithm lived it.
#[derive(Clone, Copy, Default)]
struct Run {
    hours_to_normal: f64,
    longest_gap_min: f64,
    /// Blocks short of a T-paced chain during the collapse, in hours of blocks (50 blocks = 1 h).
    lost_hours: f64,
    /// Blocks found in the first hour after the hashrate returns, beyond the normal 50.
    burst: f64,
}

#[derive(Serialize)]
pub struct Outcome {
    pub hours_to_normal: Stat,
    pub longest_gap_min: Stat,
    pub lost_hours: Stat,
    pub burst: Stat,
}

impl Outcome {
    fn of(runs: &[Run]) -> Self {
        Outcome {
            hours_to_normal: Stat::of(runs.iter().map(|r| r.hours_to_normal)),
            longest_gap_min: Stat::of(runs.iter().map(|r| r.longest_gap_min)),
            lost_hours: Stat::of(runs.iter().map(|r| r.lost_hours)),
            burst: Stat::of(runs.iter().map(|r| r.burst)),
        }
    }
}

#[derive(Serialize)]
pub struct Cell {
    pub depth: f64,
    pub hours: f64,
    /// One per algorithm, in `algos` order.
    pub algos: Vec<Outcome>,
}

/// Totals over every real collapse, per year of chain history.
#[derive(Serialize)]
pub struct Weighted {
    pub name: String,
    pub stalled_hours: f64,
    pub lost_hours: f64,
    pub burst_blocks: f64,
    /// Longest gap of the worst collapse, mean across runs.
    pub worst_gap_min: f64,
}

#[derive(Serialize)]
pub struct Steady {
    pub name: String,
    pub metrics: Metrics,
}

#[derive(Serialize)]
pub struct Decision {
    pub target_spacing: i64,
    pub tip_height: u32,
    pub tip_time: i64,
    pub algos: Vec<String>,
    pub grid_runs: usize,
    pub catalogue_runs: usize,
    pub replay_runs: usize,
    pub grid: Vec<Cell>,
    /// Real collapses replayed (depth, hours), each with one outcome per algorithm.
    pub catalogue: Vec<(f64, f64, Vec<Outcome>)>,
    pub catalogue_years: f64,
    pub weighted: Vec<Weighted>,
    pub steady: Vec<Steady>,
    pub switching: Switching,
    pub crashes: Crashes,
    pub miners: Option<Value>,
    pub mergemining: Option<Value>,
}

/// Monte Carlo of a collapse to `depth` lasting `hours`, from a flat chain in equilibrium.
fn collapse(algo: &dyn DifficultyAlgo, depth: f64, hours: f64, runs: usize, latency: f64, tip_bits: u32) -> Vec<Run> {
    let base = Params::mainnet();
    let t = base.target_spacing as f64;
    let seed = flat_seed(tip_bits, 0, &base);
    let params = Params { fork_height: seed.last().unwrap().height, ..base };
    let h0 = hashrate::work(tip_bits) / t;
    let start = seed.last().unwrap().time as f64 + 6.0 * HOUR;
    let end = start + hours * HOUR;
    let miners = Miners::Fixed(Curve::new(vec![f64::MIN, start, end], vec![h0, h0 * (1.0 - depth), h0]));
    (0..runs)
        .map(|run| {
            let mut rng = Rng::new(0xdec1_0000 + run as u64);
            let blocks = sim::run(algo, &params, &seed, end + 2.0 * HOUR, &miners, latency, &mut rng);
            let times: Vec<f64> = blocks.iter().map(|b| b.time).collect();
            let solvetimes: Vec<f64> = blocks.iter().map(|b| b.solvetime).collect();
            let r = metrics::recovery(&times, &solvetimes, start, end, t);
            let during = times.iter().filter(|&&x| x >= start && x < end).count() as f64;
            Run {
                hours_to_normal: r.hours_to_normal,
                longest_gap_min: r.longest_gap_min,
                lost_hours: (hours * HOUR / t - during) * t / HOUR,
                burst: r.blocks_first_hour_after - HOUR / t,
            }
        })
        .collect()
}

fn replay(algo: &dyn DifficultyAlgo, headers: &[Header], opts: &Options) -> Metrics {
    let params = Params { fork_height: opts.start_height as u32, ..Params::mainnet() };
    let miners = Miners::Fixed(Curve::from_chain(headers, opts.hashrate_window));
    let runs: Vec<Metrics> = (0..opts.replay_runs)
        .map(|run| {
            let mut rng = Rng::new(0x5eed_0000 + run as u64);
            let end = headers.last().unwrap().time as f64;
            let blocks = sim::run(algo, &params, &headers[..=opts.start_height], end, &miners, opts.latency, &mut rng);
            let times: Vec<f64> = blocks.iter().map(|b| b.time).collect();
            let diffs: Vec<f64> = blocks.iter().map(|b| b.difficulty).collect();
            metrics::compute(&times, &diffs, params.target_spacing as f64)
        })
        .collect();
    metrics::spread(&runs).mean
}

pub fn build(headers: &[Header], crashes: Crashes, miners: Option<Value>, mergemining: Option<Value>, opts: &Options) -> Decision {
    let algos = algos();
    let tip = headers.last().unwrap();

    let cells: Vec<(f64, f64)> = DEPTHS.iter().flat_map(|&d| DURATIONS.iter().map(move |&h| (d, h))).collect();
    eprintln!("grid: {} cells x {} algorithms x {} runs", cells.len(), algos.len(), opts.grid_runs);
    let grid_runs = parallel(cells.len() * algos.len(), |j| {
        collapse(
            algos[j % algos.len()].as_ref(),
            cells[j / algos.len()].0,
            cells[j / algos.len()].1,
            opts.grid_runs,
            opts.latency,
            tip.bits,
        )
    });
    let grid = cells
        .iter()
        .enumerate()
        .map(|(c, &(depth, hours))| Cell {
            depth,
            hours,
            algos: (0..algos.len()).map(|a| Outcome::of(&grid_runs[c * algos.len() + a])).collect(),
        })
        .collect();

    let episodes: Vec<(f64, f64)> = crashes.episodes.iter().map(|e| (e.depth, e.hours)).collect();
    eprintln!("catalogue: {} real collapses x {} algorithms x {} runs", episodes.len(), algos.len(), opts.catalogue_runs);
    let catalogue_runs = parallel(episodes.len() * algos.len(), |j| {
        let (depth, hours) = episodes[j / algos.len()];
        collapse(algos[j % algos.len()].as_ref(), depth, hours, opts.catalogue_runs, opts.latency, tip.bits)
    });
    let catalogue_years = (crashes.to - crashes.from) as f64 / (365.25 * 86_400.0);
    let weighted = (0..algos.len())
        .map(|a| {
            let per_episode: Vec<&Vec<Run>> = (0..episodes.len()).map(|e| &catalogue_runs[e * algos.len() + a]).collect();
            let total = |f: fn(&Run) -> f64| {
                per_episode.iter().map(|runs| runs.iter().map(f).sum::<f64>() / runs.len() as f64).sum::<f64>() / catalogue_years
            };
            Weighted {
                name: algos[a].name(),
                stalled_hours: total(|r| r.hours_to_normal),
                lost_hours: total(|r| r.lost_hours),
                burst_blocks: total(|r| r.burst.max(0.0)),
                worst_gap_min: per_episode
                    .iter()
                    .map(|runs| runs.iter().map(|r| r.longest_gap_min).sum::<f64>() / runs.len() as f64)
                    .fold(0.0, f64::max),
            }
        })
        .collect();
    let catalogue = episodes
        .iter()
        .enumerate()
        .map(|(e, &(d, h))| (d, h, (0..algos.len()).map(|a| Outcome::of(&catalogue_runs[e * algos.len() + a])).collect()))
        .collect();

    eprintln!("replay: {} algorithms x {} runs", algos.len(), opts.replay_runs);
    let steady = parallel(algos.len(), |a| Steady { name: algos[a].name(), metrics: replay(algos[a].as_ref(), headers, opts) });
    let switching = crate::switching::build(headers, opts.switching_runs, &algos);

    Decision {
        target_spacing: Params::mainnet().target_spacing,
        tip_height: tip.height,
        tip_time: tip.time,
        algos: algos.iter().map(|a| a.name()).collect(),
        grid_runs: opts.grid_runs,
        catalogue_runs: opts.catalogue_runs,
        replay_runs: opts.replay_runs,
        grid,
        catalogue,
        catalogue_years,
        weighted,
        steady,
        switching,
        crashes,
        miners,
        mergemining,
    }
}

pub fn print(d: &Decision) {
    println!("== Side by side: hours to a normal pace / longest gap (min) / blocks lost (h) / burst after return");
    print!("  {:<14}", "collapse");
    for a in &d.algos {
        print!(" {:>24}", a);
    }
    println!();
    for c in &d.grid {
        print!("  {:<14}", format!("-{:.0}% {} h", 100.0 * c.depth, c.hours));
        for o in &c.algos {
            print!(
                " {:>24}",
                format!("{:.1} / {:.0} / {:.2} / {:+.0}", o.hours_to_normal.mean, o.longest_gap_min.mean, o.lost_hours.mean, o.burst.mean)
            );
        }
        println!();
    }
    println!("\n== Weighted by the {} real collapses, per year ({:.2} years of history)", d.catalogue.len(), d.catalogue_years);
    println!("  {:<16} {:>14} {:>16} {:>14} {:>16}", "algorithm", "stalled h/yr", "blocks lost h/yr", "burst blk/yr", "worst gap (min)");
    for w in &d.weighted {
        println!("  {:<16} {:>14.1} {:>16.2} {:>14.0} {:>16.0}", w.name, w.stalled_hours, w.lost_hours, w.burst_blocks, w.worst_gap_min);
    }
    println!("\n== Steady state (real hashrate replay)");
    println!("  {:<16} {:>10} {:>12} {:>10} {:>10}", "algorithm", "mean (s)", "gaps>7m/day", "p99 (s)", "jitter");
    for s in &d.steady {
        let m = &s.metrics;
        println!(
            "  {:<16} {:>10.2} {:>12.2} {:>10.0} {:>9.1}%",
            s.name, m.mean_solvetime, m.gaps_over_7min_per_day, m.p99_solvetime, m.difficulty_jitter_pct
        );
    }
    println!("\n== Leaving and returning (switching module): edge of profit switchers / pool back after leaving");
    for (i, a) in d.algos.iter().enumerate() {
        let exits: Vec<String> =
            d.switching.exits.iter().map(|c| format!("{:.0}% {}h {:.2}", 100.0 * c.share, c.hours, c.algos[i].pool)).collect();
        println!("  {:<16} hopper edge {:.2}; {}", a, d.switching.hoppers[0].algos[i].edge, exits.join(", "));
    }
}
