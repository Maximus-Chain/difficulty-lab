mod algo;
mod chain;
mod hashrate;
mod metrics;
mod report;
mod rpc;
mod sim;
mod target;

use algo::{DifficultyAlgo, Mainnet, Params};
use std::path::Path;
use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "Difficulty algorithm simulator for Maximus")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Download height, timestamp and nBits of every block via RPC.
    Extract {
        #[arg(long, default_value = "http://127.0.0.1:9938")]
        url: String,
        #[arg(long, default_value = "diffsim")]
        user: String,
        #[arg(long, default_value = "diffsim")]
        password: String,
        #[arg(long, default_value = "data/headers.csv")]
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
        /// Seconds miners waste on the old tip after each block.
        #[arg(long, default_value_t = 5.0)]
        latency: f64,
    },
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Extract { url, user, password, out } => {
            let rpc = rpc::Rpc::new(&url, &user, &password);
            let headers = rpc::extract_headers(&rpc)?;
            if let Some(dir) = out.parent() {
                std::fs::create_dir_all(dir)?;
            }
            chain::write_csv(&out, &headers)?;
            println!("saved {} headers to {}", headers.len(), out.display());
        }
        Cmd::Validate { input } => validate(&chain::read_csv(&input)?),
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
