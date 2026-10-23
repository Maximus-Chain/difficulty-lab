//! Miners joining and leaving on purpose: what a miner gains by switching hashrate on and off,
//! for every algorithm. Each scenario puts the switching miner next to steady miners on a flat
//! seed in equilibrium, like the synthetic scenarios in `report`.
//!
//! The headline number is the switching miner's *edge*: its blocks per hash divided by the steady
//! miners' blocks per hash. 1.00 means no advantage; 1.20 means 20% more reward per unit of work.

use crate::algo::{DifficultyAlgo, Params};
use crate::chain::Header;
use crate::hashrate::{Curve, HASHES_PER_DIFFICULTY};
use crate::par::parallel;
use crate::report::{Algos, flat_seed};
use crate::sim::Rng;
use crate::target::difficulty;
use serde::Serialize;

const DAY: f64 = 86_400.0;
const HOUR: f64 = 3_600.0;

enum Switcher {
    /// Hashrate on a fixed schedule.
    Schedule(Curve),
    /// Adds `extra` while difficulty is below `join` times what the steady hashrate alone
    /// justifies, and leaves once it is above `leave` times that.
    Hopper { extra: f64, join: f64, leave: f64 },
}

struct Setup<'a> {
    seed: &'a [Header],
    steady: Curve,
    switcher: Switcher,
    /// Blocks and work are counted between these two times.
    measure_from: f64,
    end: f64,
}

#[derive(Default, Clone, Copy)]
struct Outcome {
    blocks: f64,
    switcher_blocks: f64,
    steady_work: f64,
    switcher_work: f64,
}

impl Outcome {
    fn sum(outcomes: &[Outcome]) -> Outcome {
        outcomes.iter().fold(Outcome::default(), |a, o| Outcome {
            blocks: a.blocks + o.blocks,
            switcher_blocks: a.switcher_blocks + o.switcher_blocks,
            steady_work: a.steady_work + o.steady_work,
            switcher_work: a.switcher_work + o.switcher_work,
        })
    }

    /// Blocks per hash for the switcher and for the steady miners, each relative to mining
    /// steadily at the seed's difficulty (`block_work` hashes per block).
    fn vs_steady(&self, block_work: f64) -> (f64, f64) {
        (self.switcher_blocks * block_work / self.switcher_work, (self.blocks - self.switcher_blocks) * block_work / self.steady_work)
    }

    fn edge(&self) -> f64 {
        (self.switcher_blocks / self.switcher_work) / ((self.blocks - self.switcher_blocks) / self.steady_work)
    }
}

fn run(algo: &dyn DifficultyAlgo, params: &Params, setup: &Setup, rng: &mut Rng) -> Outcome {
    let t = params.target_spacing as f64;
    let mut chain = setup.seed.to_vec();
    let mut time = chain.last().expect("empty seed").time as f64;
    let total = match &setup.switcher {
        Switcher::Schedule(curve) => Some(setup.steady.add(curve)),
        Switcher::Hopper { .. } => None,
    };
    let (mut on, mut hopper_seconds) = (false, 0.0);
    let mut out = Outcome::default();

    while time < setup.end {
        let bits = algo.next_bits(&chain, params);
        let diff = difficulty(bits);
        let work = rng.exp() * diff * HASHES_PER_DIFFICULTY;

        let (end, switcher_rate, total_rate) = match &setup.switcher {
            Switcher::Schedule(curve) => {
                let total = total.as_ref().unwrap();
                let end = time + total.solve_time(time, work, 0.0);
                (end, curve.at(end), total.at(end))
            }
            Switcher::Hopper { extra, join, leave } => {
                let reference = setup.steady.at(time) * t / HASHES_PER_DIFFICULTY;
                if diff < join * reference {
                    on = true;
                } else if diff > leave * reference {
                    on = false;
                }
                let extra = if on { *extra } else { 0.0 };
                let end = time + setup.steady.solve_time(time, work, extra);
                if on {
                    hopper_seconds += (end.min(setup.end) - time.max(setup.measure_from)).max(0.0);
                }
                (end, extra, setup.steady.at(end) + extra)
            }
        };
        // Hashrate is piecewise constant, so the finder is drawn from the rates at the moment
        // the block is found.
        let by_switcher = rng.uniform() * total_rate < switcher_rate;
        chain.push(Header { height: chain.len() as u32, time: end.floor() as i64, bits, n_tx: 1 });

        if end >= setup.measure_from && end < setup.end {
            out.blocks += 1.0;
            if by_switcher {
                out.switcher_blocks += 1.0;
            }
        }
        time = end;
    }

    out.steady_work = setup.steady.integral(setup.measure_from, setup.end);
    out.switcher_work = match &setup.switcher {
        Switcher::Schedule(curve) => curve.integral(setup.measure_from, setup.end),
        Switcher::Hopper { extra, .. } => extra * hopper_seconds,
    };
    out
}

/// Runs `make_setup` for every algorithm, `runs` times each, and sums the outcomes per algorithm.
fn evaluate<'a>(algos: &Algos, params: &Params, seed: &'a [Header], runs: usize, make_setup: &(dyn Fn() -> Setup<'a> + Sync)) -> Vec<Outcome> {
    let params = &Params { fork_height: seed.last().unwrap().height, ..params.clone() };
    let results = parallel(algos.len() * runs, |job| {
        let (a, r) = (job / runs, job % runs);
        let mut rng = Rng::new(0x5eed_a77a_c0de ^ (r as u64) << 8);
        run(algos[a].as_ref(), params, &make_setup(), &mut rng)
    });
    results.chunks(runs).map(Outcome::sum).collect()
}

#[derive(Serialize)]
pub struct Switching {
    pub runs: usize,
    pub hoppers: Vec<HopperCase>,
    pub exits: Vec<ExitCase>,
}

#[derive(Serialize)]
pub struct HopperCase {
    /// Hopper hashrate as a multiple of the steady hashrate.
    pub extra: f64,
    pub join: f64,
    pub leave: f64,
    pub algos: Vec<HopperResult>,
}

#[derive(Serialize)]
pub struct HopperResult {
    pub name: String,
    pub edge: f64,
    /// Share of all blocks found by the hoppers.
    pub share: f64,
}

#[derive(Serialize)]
pub struct ExitCase {
    /// Share of the hashrate that leaves.
    pub share: f64,
    pub hours: f64,
    pub algos: Vec<ExitResult>,
}

#[derive(Serialize)]
pub struct ExitResult {
    pub name: String,
    /// Pool's blocks per hash relative to mining steadily, from the exit to 24 h after the return.
    pub pool: f64,
    /// Same for the miners who stayed.
    pub stayed: f64,
}

pub fn build(headers: &[Header], runs: usize, algos: &Algos) -> Switching {
    let params = Params::mainnet();
    let t = params.target_spacing as f64;
    let names: Vec<String> = algos.iter().map(|a| a.name()).collect();
    let seed = flat_seed(headers.last().expect("empty chain").bits, 0, &params);
    let t0 = seed.last().unwrap().time as f64;
    // Hashrate that mines the seed's difficulty at exactly T.
    let h0 = difficulty(seed.last().unwrap().bits) * HASHES_PER_DIFFICULTY / t;

    let hoppers = [(3.0, 1.2, 1.6), (3.0, 1.0, 1.2), (10.0, 1.0, 1.2)]
        .into_iter()
        .map(|(extra, join, leave)| {
            eprintln!("switching: hoppers {extra}x, join {join}, leave {leave}");
            let outcomes = evaluate(algos, &params, &seed, runs, &|| Setup {
                seed: &seed,
                steady: Curve::constant(h0),
                switcher: Switcher::Hopper { extra: extra * h0, join, leave },
                measure_from: t0,
                end: t0 + 3.0 * DAY,
            });
            let algos = outcomes
                .iter()
                .zip(&names)
                .map(|(o, name)| HopperResult { name: name.clone(), edge: o.edge(), share: o.switcher_blocks / o.blocks })
                .collect();
            HopperCase { extra, join, leave, algos }
        })
        .collect();

    let exits = [(0.8, 2.0), (0.8, 6.0), (0.99, 2.0), (0.99, 6.0), (0.99, 24.0)]
        .into_iter()
        .map(|(share, hours)| {
            eprintln!("switching: {:.0}% leaves for {hours} h", share * 100.0);
            let leave = t0 + 6.0 * HOUR;
            let back = leave + hours * HOUR;
            let outcomes = evaluate(algos, &params, &seed, runs, &|| Setup {
                seed: &seed,
                steady: Curve::constant(h0 * (1.0 - share)),
                switcher: Switcher::Schedule(Curve::new(vec![f64::MIN, leave, back], vec![h0 * share, 0.0, h0 * share])),
                measure_from: leave,
                end: back + DAY,
            });
            let algos = outcomes
                .iter()
                .zip(&names)
                .map(|(o, name)| {
                    let (pool, stayed) = o.vs_steady(h0 * t);
                    ExitResult { name: name.clone(), pool, stayed }
                })
                .collect();
            ExitCase { share, hours, algos }
        })
        .collect();

    Switching { runs, hoppers, exits }
}

pub fn print(s: &Switching) {
    println!("Edge = switching miners' blocks per hash / steady miners' blocks per hash (1.00 = fair). {} runs per cell.", s.runs);
    let names: Vec<&str> = s.hoppers[0].algos.iter().map(|a| a.name.as_str()).collect();

    println!("\n== Profit switching (hopper hashrate x steady, join/leave thresholds): edge / share of blocks");
    print!("  {:<24}", "");
    for c in &s.hoppers {
        print!("  {:>16}", format!("{}x {}/{}", c.extra, c.join, c.leave));
    }
    println!();
    for (i, name) in names.iter().enumerate() {
        print!("  {name:<24}");
        for c in &s.hoppers {
            print!("  {:>16}", format!("{:.2} / {:.0}%", c.algos[i].edge, 100.0 * c.algos[i].share));
        }
        println!();
    }

    println!("\n== A pool leaves and returns, exit to 24 h after the return: blocks per hash vs steady mining, pool / miners who stayed");
    print!("  {:<24}", "");
    for c in &s.exits {
        print!("  {:>16}", format!("{:.0}% off {}h", c.share * 100.0, c.hours));
    }
    println!();
    for (i, name) in names.iter().enumerate() {
        print!("  {name:<24}");
        for c in &s.exits {
            print!("  {:>16}", format!("{:.2} / {:.2}", c.algos[i].pool, c.algos[i].stayed));
        }
        println!();
    }
}
