//! Superblock audit: for every superblock height, how long it took to mine, whether the
//! governance budget was actually paid, and how many proposals had been submitted on-chain
//! (via their 1 MAXI OP_RETURN collateral) during the cycle leading to it.
//!
//! Triggers and votes travel over P2P and are pruned after a cycle, so they cannot be audited
//! after the fact; proposal collaterals and coinbase outputs are the on-chain evidence left.

use crate::chain::Header;
use crate::par::parallel_with;
use crate::rpc::Rpc;
use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{Value, json};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

const COIN: i64 = 100_000_000;
/// Mainnet consensus (src/chainparams.cpp).
const SUBSIDY_HALVING_INTERVAL: u32 = 36_000;
const SUPERBLOCK_START_BLOCK: u32 = 18_000;
const SUPERBLOCK_CYCLE: u32 = 36_000;
/// GOVERNANCE_PROPOSAL_FEE_TX (src/governance/object.h).
const PROPOSAL_FEE: i64 = COIN;
/// A superblock counts as paid when the coinbase exceeds the regular reward by at least this.
const PAID_THRESHOLD: i64 = COIN;

/// Port of GetBlockSubsidyHelper (src/validation.cpp): (regular block reward, superblock share)
/// in satoshis for the block at `prev_height + 1`.
pub fn subsidy(prev_height: u32) -> (i64, i64) {
    let mut subsidy: i64 = if prev_height == 0 { 6000 * COIN } else { COIN / 2 };
    let mut i = SUBSIDY_HALVING_INTERVAL;
    while i <= prev_height {
        subsidy -= subsidy / 125;
        i += SUBSIDY_HALVING_INTERVAL;
    }
    let superblock_part = if prev_height > SUPERBLOCK_START_BLOCK { subsidy / 20 * 3 } else { 0 };
    (subsidy - superblock_part, superblock_part)
}

#[derive(Serialize)]
pub struct Payee {
    pub address: String,
    pub amount: f64,
}

#[derive(Serialize)]
pub struct Superblock {
    pub height: u32,
    pub time: i64,
    /// Seconds since the previous block: the time it took to get this superblock accepted.
    pub solvetime: i64,
    pub coinbase_tag: String,
    pub coinbase_total: f64,
    pub regular_reward: f64,
    /// Coinbase minus the regular reward (transaction fees included, they are tiny here).
    pub budget_paid: f64,
    /// Maximum the superblock was allowed to pay (getsuperblockbudget).
    pub budget_available: f64,
    pub paid: bool,
    pub payees: Vec<Payee>,
    /// Proposal collaterals confirmed since the previous superblock.
    pub proposals_in_cycle: Vec<ProposalFee>,
}

#[derive(Serialize, Clone)]
pub struct ProposalFee {
    pub height: u32,
    pub time: i64,
    pub txid: String,
}

/// A governance object the node currently holds (proposal or payment trigger).
#[derive(Serialize)]
pub struct GovObject {
    pub kind: &'static str,
    pub name: String,
    /// Total the object asks for: the proposal amount, or the sum of a trigger's payments.
    pub amount: f64,
    pub yes: i64,
    pub no: i64,
    pub funded: bool,
}

/// The coming superblock as seen by the node right now; triggers and votes only exist here.
#[derive(Serialize)]
pub struct Next {
    pub height: u32,
    pub budget: f64,
    pub funding_threshold: i64,
    pub masternodes_enabled: i64,
    pub objects: Vec<GovObject>,
}

#[derive(Serialize)]
pub struct Audit {
    pub tip_height: u32,
    pub blocks_scanned: usize,
    pub superblocks: Vec<Superblock>,
    pub next: Option<Next>,
}

fn amount(v: &Value) -> i64 {
    (v.as_f64().unwrap_or(0.0) * COIN as f64).round() as i64
}

fn coinbase_tag(coinbase: &Value) -> String {
    let hex = coinbase["vin"][0]["coinbase"].as_str().unwrap_or("");
    let bytes: Vec<u8> = (0..hex.len() / 2).filter_map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok()).collect();
    // Skip the BIP34 height push; keep runs of printable ASCII (pool names, software tags).
    let text: String = bytes.iter().skip(4).map(|&b| if (32..127).contains(&b) { b as char } else { ' ' }).collect();
    text.split_whitespace().filter(|w| w.len() >= 3).collect::<Vec<_>>().join(" ")
}

fn address(output: &Value) -> String {
    let spk = &output["scriptPubKey"];
    spk["address"]
        .as_str()
        .or_else(|| spk["addresses"][0].as_str())
        .or_else(|| spk["type"].as_str())
        .unwrap_or("?")
        .to_string()
}

fn block(rpc: &Rpc, height: u32) -> Result<Value> {
    let hash = rpc.call("getblockhash", json!([height]))?;
    rpc.call("getblock", json!([hash, 2]))
}

/// Finds every proposal collateral (an OP_RETURN output worth exactly the proposal fee) in blocks
/// that carry transactions. The result is cached in `cache`, keyed by the tip it was scanned to.
fn proposal_fees(rpc: &Rpc, headers: &[Header], cache: &Path) -> Result<(Vec<ProposalFee>, usize)> {
    let tip = headers.last().context("empty chain")?.height;
    if let Ok(text) = fs::read_to_string(cache) {
        let mut lines = text.lines();
        if lines.next() == Some(&format!("# scanned to {tip}")) {
            let fees = lines
                .skip(1)
                .map(|l| {
                    let mut c = l.split(',');
                    let height: u32 = c.next().context("height")?.parse()?;
                    Ok(ProposalFee { height, time: headers[height as usize].time, txid: c.next().context("txid")?.to_string() })
                })
                .collect::<Result<Vec<_>>>()?;
            let scanned = headers.iter().filter(|h| h.n_tx > 1).count();
            return Ok((fees, scanned));
        }
    }

    let with_txs: Vec<u32> = headers.iter().filter(|h| h.n_tx > 1).map(|h| h.height).collect();
    let done = AtomicUsize::new(0);
    // A handful of threads keeps the node's RPC work queue from overflowing.
    let found = parallel_with(with_txs.len(), Some(6), |i| -> Result<Vec<ProposalFee>> {
        let height = with_txs[i];
        let b = block(rpc, height)?;
        let mut fees = Vec::new();
        for tx in b["tx"].as_array().context("tx")?.iter().skip(1) {
            let is_fee = tx["vout"].as_array().into_iter().flatten().any(|o| {
                o["scriptPubKey"]["type"].as_str() == Some("nulldata") && amount(&o["value"]) == PROPOSAL_FEE
            });
            if is_fee {
                fees.push(ProposalFee { height, time: headers[height as usize].time, txid: tx["txid"].as_str().unwrap_or("").into() });
            }
        }
        let n = done.fetch_add(1, Ordering::Relaxed) + 1;
        if n % 1000 == 0 {
            eprint!("\r  scanned {n} / {} blocks with transactions", with_txs.len());
        }
        Ok(fees)
    });
    eprintln!();
    let fees: Vec<ProposalFee> = found.into_iter().collect::<Result<Vec<_>>>()?.into_iter().flatten().collect();

    let mut out = fs::File::create(cache)?;
    writeln!(out, "# scanned to {tip}\nheight,txid")?;
    for f in &fees {
        writeln!(out, "{},{}", f.height, f.txid)?;
    }
    Ok((fees, with_txs.len()))
}

pub fn audit(rpc: &Rpc, headers: &[Header], cache: &Path) -> Result<Audit> {
    let tip = headers.last().context("empty chain")?.height;
    let (fees, blocks_scanned) = proposal_fees(rpc, headers, cache)?;

    let mut superblocks = Vec::new();
    let first = SUPERBLOCK_START_BLOCK.div_ceil(SUPERBLOCK_CYCLE) * SUPERBLOCK_CYCLE;
    for height in (first..=tip).step_by(SUPERBLOCK_CYCLE as usize) {
        let b = block(rpc, height)?;
        let coinbase = &b["tx"][0];
        let outputs = coinbase["vout"].as_array().context("vout")?;
        let total: i64 = outputs.iter().map(|o| amount(&o["value"])).sum();
        let (regular, _) = subsidy(height - 1);
        let available = amount(&rpc.call("getsuperblockbudget", json!([height]))?);
        let paid = total - regular;
        let previous = height.saturating_sub(SUPERBLOCK_CYCLE);

        superblocks.push(Superblock {
            height,
            time: headers[height as usize].time,
            solvetime: headers[height as usize].time - headers[height as usize - 1].time,
            coinbase_tag: coinbase_tag(coinbase),
            coinbase_total: total as f64 / COIN as f64,
            regular_reward: regular as f64 / COIN as f64,
            budget_paid: paid as f64 / COIN as f64,
            budget_available: available as f64 / COIN as f64,
            paid: paid >= PAID_THRESHOLD,
            payees: outputs
                .iter()
                .filter(|o| amount(&o["value"]) >= PAID_THRESHOLD)
                .map(|o| Payee { address: address(o), amount: o["value"].as_f64().unwrap_or(0.0) })
                .collect(),
            proposals_in_cycle: fees.iter().filter(|f| f.height > previous && f.height <= height).cloned().collect(),
        });
    }
    let next = next(rpc).map_err(|e| eprintln!("could not read current governance objects: {e}")).ok();
    Ok(Audit { tip_height: tip, blocks_scanned, superblocks, next })
}

fn next(rpc: &Rpc) -> Result<Next> {
    let info = rpc.call("getgovernanceinfo", json!([]))?;
    let height = info["nextsuperblock"].as_u64().context("nextsuperblock")? as u32;
    let mut objects = Vec::new();
    for obj in rpc.call("gobject", json!(["list", "all", "all"]))?.as_object().context("gobject list")?.values() {
        let data: Value = serde_json::from_str(obj["DataString"].as_str().unwrap_or("{}"))?;
        let (kind, name, amount) = match obj["ObjectType"].as_i64() {
            Some(1) => ("proposal", data["name"].as_str().unwrap_or("").to_string(), amount(&data["payment_amount"])),
            Some(2) => {
                // Trigger payments are a '|'-separated string of amounts.
                let total = data["payment_amounts"]
                    .as_str()
                    .unwrap_or("")
                    .split('|')
                    .filter_map(|a| a.trim().parse::<f64>().ok())
                    .map(|a| (a * COIN as f64).round() as i64)
                    .sum();
                ("trigger", format!("superblock {}", data["event_block_height"]), total)
            }
            _ => continue,
        };
        objects.push(GovObject {
            kind,
            name,
            amount: amount as f64 / COIN as f64,
            yes: obj["YesCount"].as_i64().unwrap_or(0),
            no: obj["NoCount"].as_i64().unwrap_or(0),
            funded: obj["fCachedFunding"].as_bool().unwrap_or(false),
        });
    }
    Ok(Next {
        height,
        budget: amount(&rpc.call("getsuperblockbudget", json!([height]))?) as f64 / COIN as f64,
        funding_threshold: info["fundingthreshold"].as_i64().unwrap_or(0),
        masternodes_enabled: rpc.call("masternode", json!(["count"]))?["enabled"].as_i64().unwrap_or(0),
        objects,
    })
}

pub fn print(audit: &Audit) {
    println!("scanned {} blocks with transactions for proposal collaterals\n", audit.blocks_scanned);
    println!("{:>7}  {:<16}  {:>9}  {:>9}  {:>9}  {:>9}  payees / tag", "height", "time (UTC)", "took", "paid", "budget", "proposals");
    for sb in &audit.superblocks {
        let took = if sb.solvetime >= 600 { format!("{:.1} h", sb.solvetime as f64 / 3600.0) } else { format!("{} s", sb.solvetime) };
        let payees: Vec<String> = sb.payees.iter().map(|p| format!("{} {:.2}", p.address, p.amount)).collect();
        println!(
            "{:>7}  {:<16}  {:>9}  {:>9.2}  {:>9.2}  {:>9}  {}{}",
            sb.height,
            crate::fmt_time(sb.time),
            took,
            sb.budget_paid.max(0.0),
            sb.budget_available,
            sb.proposals_in_cycle.len(),
            if payees.is_empty() { "NOT PAID".to_string() } else { payees.join(", ") },
            if sb.coinbase_tag.is_empty() { String::new() } else { format!("  [{}]", sb.coinbase_tag) },
        );
    }
    if let Some(next) = &audit.next {
        println!(
            "\nnext superblock {} (budget {:.8}, funding threshold {} yes votes, {} masternodes enabled)",
            next.height, next.budget, next.funding_threshold, next.masternodes_enabled
        );
        for o in &next.objects {
            // A superblock that pays more than its budget is invalid, so a trigger over budget stalls the chain.
            let fits = if o.amount <= next.budget { "within budget" } else { "OVER BUDGET" };
            println!("  {:<8} {:<32} {:>14.8}  yes {:>4}  no {:>3}  funded {:<5}  {fits}", o.kind, o.name, o.amount, o.yes, o.no, o.funded);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subsidy_matches_chain() {
        // Block 72000: one 0.8% reduction applied, 15% superblock share split off.
        let (regular, sb) = subsidy(71_999);
        assert_eq!(regular, 42_160_000);
        assert_eq!(sb * SUPERBLOCK_CYCLE as i64, 267_840_000_000); // getsuperblockbudget 72000 = 2678.4
    }
}
