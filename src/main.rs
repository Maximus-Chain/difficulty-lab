mod algo;
mod anomalies;
mod chain;
mod hashrate;
mod metrics;
mod par;
mod report;
mod rpc;
mod sim;
mod superblocks;
mod switching;
mod target;

use algo::{DifficultyAlgo, Mainnet, Params};
use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(about = "Difficulty algorithm simulator for Maximus")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Args)]
struct RpcArgs {
    #[arg(long, default_value = "http://127.0.0.1:9938")]
    url: String,
    #[arg(long, default_value = "diffsim")]
    user: String,
    #[arg(long, default_value = "diffsim")]
    password: String,
}

impl RpcArgs {
    fn client(&self) -> rpc::Rpc {
        rpc::Rpc::new(&self.url, &self.user, &self.password)
    }
}

#[derive(Subcommand)]
enum Cmd {
    /// Download height, timestamp, nBits and transaction count of every block via RPC.
    Extract {
        #[command(flatten)]
        rpc: RpcArgs,
        #[arg(long, default_value = "data/headers.csv")]
        out: PathBuf,
    },
    /// Scan the real chain for anomalies: long gaps, timestamps, clamps, bursts, collapses.
    Anomalies {
        #[arg(long, default_value = "data/headers.csv")]
        input: PathBuf,
        #[arg(long, default_value_t = 1200)]
        start_height: usize,
    },
    /// Miners switching on purpose, for every algorithm: profit switching and a pool that leaves and returns.
    Switching {
        #[arg(long, default_value = "data/headers.csv")]
        input: PathBuf,
        #[arg(long, default_value_t = 40)]
        runs: usize,
    },
    /// Audit every superblock: time to mine, budget paid, proposals submitted in its cycle.
    Superblocks {
        #[command(flatten)]
        rpc: RpcArgs,
        #[arg(long, default_value = "data/headers.csv")]
        input: PathBuf,
        /// Proposal-collateral scan cache (rebuilt when the chain tip changes).
        #[arg(long, default_value = "data/proposal_fees.csv")]
        cache: PathBuf,
        #[arg(long, default_value = "data/superblocks.json")]
        out: PathBuf,
    },
    /// Recompute every block's nBits with our DGW and compare it with the real chain.
    Validate {
        #[arg(long, default_value = "data/headers.csv")]
        input: PathBuf,
    },
    /// Run every scenario against every algorithm and write the JSON report.
    Simulate {
        #[arg(long, default_value = "data/headers.csv")]
        input: PathBuf,
        #[arg(long, default_value = "data/report.json")]
        out: PathBuf,
        /// Self-contained HTML report with the JSON embedded.
        #[arg(long, default_value = "data/report.html")]
        html: PathBuf,
        #[arg(long, default_value_t = 8)]
        replay_runs: usize,
        #[arg(long, default_value_t = 200)]
        pulse_runs: usize,
        #[arg(long, default_value_t = 20)]
        hopper_runs: usize,
        #[arg(long, default_value_t = 240)]
        hashrate_window: usize,
        #[arg(long, default_value_t = 1200)]
        start_height: usize,
        #[arg(long, default_value_t = 5.0)]
        pulse_factor: f64,
        #[arg(long, default_value_t = 3.0)]
        hopper_extra: f64,
        #[arg(long, default_value_t = 1.2)]
        hopper_join: f64,
        #[arg(long, default_value_t = 1.6)]
        hopper_leave: f64,
        #[arg(long, default_value_t = 200)]
        crash_runs: usize,
        /// Hashrate multipliers during the collapse, one scenario each (0.5 = half the miners leave).
        #[arg(long, value_delimiter = ',', default_value = "0.5,0.2,0.01")]
        crash_factors: Vec<f64>,
        #[arg(long, default_value_t = 24.0)]
        crash_hours: f64,
        /// Runs per cell for the switching-miner scenarios.
        #[arg(long, default_value_t = 40)]
        switching_runs: usize,
        /// Superblock audit to embed in the report (from the `superblocks` subcommand), if present.
        #[arg(long, default_value = "data/superblocks.json")]
        superblocks: PathBuf,
        /// Seconds miners waste on the old tip after each block.
        #[arg(long, default_value_t = 5.0)]
        latency: f64,
    },
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Extract { rpc, out } => {
            let rpc = rpc.client();
            let headers = rpc::extract_headers(&rpc)?;
            if let Some(dir) = out.parent() {
                std::fs::create_dir_all(dir)?;
            }
            chain::write_csv(&out, &headers)?;
            println!("saved {} headers to {}", headers.len(), out.display());
        }
        Cmd::Validate { input } => validate(&chain::read_csv(&input)?),
        Cmd::Switching { input, runs } => switching::print(&switching::build(&chain::read_csv(&input)?, runs, &report::algos())),
        Cmd::Anomalies { input, start_height } => anomalies::report(&chain::read_csv(&input)?, start_height),
        Cmd::Superblocks { rpc, input, cache, out } => {
            let audit = superblocks::audit(&rpc.client(), &chain::read_csv(&input)?, &cache)?;
            superblocks::print(&audit);
            std::fs::write(&out, serde_json::to_string(&audit)?)?;
            println!("\nwrote {}", out.display());
        }
        Cmd::Simulate {
            input,
            out,
            html,
            replay_runs,
            pulse_runs,
            hopper_runs,
            hashrate_window,
            start_height,
            pulse_factor,
            hopper_extra,
            hopper_join,
            hopper_leave,
            crash_runs,
            crash_factors,
            crash_hours,
            switching_runs,
            superblocks,
            latency,
        } => {
            let headers = chain::read_csv(&input)?;
            let opts = report::Options {
                replay_runs,
                pulse_runs,
                hopper_runs,
                hashrate_window,
                start_height,
                pulse_factor,
                hopper_extra,
                hopper_join,
                hopper_leave,
                crash_runs,
                crash_factors,
                crash_hours,
                switching_runs,
                superblocks: match std::fs::read_to_string(&superblocks) {
                    Ok(text) => Some(serde_json::from_str(&text)?),
                    Err(_) => {
                        eprintln!("no superblock audit at {} (run `superblocks` to include it)", superblocks.display());
                        None
                    }
                },
                latency,
            };
            let report = report::build(&headers, &opts);
            let json = serde_json::to_string(&report)?;
            std::fs::write(&out, &json)?;
            write_html(&html, &json)?;
            println!("wrote {} and {}", out.display(), html.display());
        }
    }
    Ok(())
}

fn write_html(path: &Path, json: &str) -> Result<()> {
    const TEMPLATE: &str = include_str!("../report/template.html");
    // The template is a page body (it is also published as a hosted artifact, which adds its
    // own document skeleton), so a standalone file needs the doctype and charset added here.
    let page = format!(
        "<!doctype html>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1, viewport-fit=cover\">\n{}",
        TEMPLATE.replace("__REPORT_DATA__", json)
    );
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(path, page)?;
    Ok(())
}

fn validate(headers: &[chain::Header]) {
    let algo = Mainnet;
    let mismatches = algo::mismatches(headers, &Params::mainnet());

    let checked = headers.len() - 1;
    println!("algorithm: {}", algo.name());
    println!("blocks checked: 1..={checked}");
    println!("matches: {} / {checked}", checked - mismatches.len());
    for (h, expected, actual) in mismatches.iter().take(20) {
        println!("  height {h}: computed {expected:08x}, chain {actual:08x}");
    }
    if mismatches.is_empty() {
        println!("OK: our DGW reproduces the chain bit for bit.");
    }
}

/// Formats a unix timestamp as "YYYY-MM-DD HH:MM" in UTC (Howard Hinnant's civil_from_days).
pub fn fmt_time(t: i64) -> String {
    let (days, secs) = (t.div_euclid(86_400), t.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02} {:02}:{:02}", secs / 3600, secs % 3600 / 60)
}

#[cfg(test)]
mod tests {
    #[test]
    fn fmt_time_utc() {
        assert_eq!(super::fmt_time(0), "1970-01-01 00:00");
        assert_eq!(super::fmt_time(1_738_355_558), "2025-01-31 20:32");
    }
}
