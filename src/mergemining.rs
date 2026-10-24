//! Merge-mining check: whether the pools that left Maximus during its collapses kept mining the
//! parent chain (Dash) and the sibling aux chain (Osmium, chain ID 0x62, merge-mined on the same
//! Dash work).
//!
//! An AuxPoW parent block is usually only a share: it meets the aux chain's target, not Dash's,
//! so its own hash is not on Dash. What must be on Dash is the block it builds on, at the height
//! in its coinbase minus one. That is what the parent check looks up in a public Insight API.
//! Dash's own pool shares come from a sample of Dash blocks read by height.

use crate::crashes::Crashes;
use crate::fmt_time;
use crate::miners::{Block, Window, hex_bytes, pool_name, tag, window};
use crate::rpc::Rpc;
use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};

const HOUR: i64 = 3_600;
/// Dash blocks sampled on each side of a collapse.
const DASH_SAMPLE: usize = 30;
/// Pause between two requests to the explorer.
const PAUSE: Duration = Duration::from_millis(350);

/// Read-only Insight API client. Every answer (a 404 included) is cached on disk, so a rerun
/// sends nothing for what it already asked.
pub struct Insight {
    base: String,
    cache: PathBuf,
    agent: ureq::Agent,
    last: Instant,
    pub requests: usize,
}

impl Insight {
    pub fn new(base: &str, cache: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&cache)?;
        Ok(Self { base: base.trim_end_matches('/').to_string(), cache, agent: ureq::agent(), last: Instant::now() - PAUSE, requests: 0 })
    }

    /// `None` when the explorer does not know the object.
    fn get(&mut self, path: &str) -> Result<Option<Value>> {
        let file = self.cache.join(format!("{}.json", path.replace('/', "_")));
        if let Ok(text) = std::fs::read_to_string(&file) {
            let v: Value = serde_json::from_str(&text)?;
            return Ok((!v.is_null()).then_some(v));
        }
        std::thread::sleep(PAUSE.saturating_sub(self.last.elapsed()));
        self.last = Instant::now();
        self.requests += 1;
        let v: Value = match self.agent.get(&format!("{}/{path}", self.base)).call() {
            Ok(r) => r.into_json()?,
            Err(ureq::Error::Status(404, _)) => Value::Null,
            Err(e) => return Err(e).with_context(|| format!("GET {path}")),
        };
        std::fs::write(&file, v.to_string())?;
        Ok((!v.is_null()).then_some(v))
    }

    fn block_at(&mut self, height: u64) -> Result<DashBlock> {
        let index = self.get(&format!("block-index/{height}"))?.context("block-index")?;
        let hash = index["blockHash"].as_str().context("blockHash")?.to_string();
        let block = self.get(&format!("block/{hash}"))?.context("block")?;
        let txid = block["tx"][0].as_str().context("coinbase txid")?.to_string();
        let tx = self.get(&format!("tx/{txid}"))?.context("coinbase tx")?;
        let script = hex_bytes(tx["vin"][0]["coinbase"].as_str().unwrap_or_default());
        Ok(DashBlock { time: block["time"].as_i64().unwrap_or(0), pool: pool_name(&tag(&script)) })
    }
}

struct DashBlock {
    time: i64,
    pool: String,
}

#[derive(Serialize)]
pub struct ParentCheck {
    pub label: String,
    pub sampled: usize,
    /// The parent's previous block is on Dash's main chain, at the parent's height minus one.
    pub on_dash: usize,
    /// Found on Dash but off the main chain or at another height.
    pub mismatched: usize,
    pub not_found: usize,
}

#[derive(Serialize)]
pub struct Side {
    pub maximus: Window,
    pub osmium: Window,
    pub maximus_per_hour: f64,
    pub osmium_per_hour: f64,
    pub dash_sampled: usize,
    /// Share of the sampled Dash blocks per pool, largest first.
    pub dash: Vec<(String, f64)>,
}

#[derive(Serialize)]
pub struct Collapse {
    pub start: i64,
    pub hours: f64,
    pub depth: f64,
    pub before: Side,
    pub during: Side,
    /// Osmium parents found during the collapse, checked against Dash.
    pub osmium_parents: ParentCheck,
}

#[derive(Serialize)]
pub struct MergeMining {
    pub osmium_tip: u32,
    /// Maximus AuxPoW blocks of the last 90 days whose parent hash is also an Osmium parent.
    pub shared_parents_90d: f64,
    pub checks: Vec<ParentCheck>,
    pub collapses: Vec<Collapse>,
}

/// The parent's previous block hash and its BIP34 height, read again from the node (the
/// cache keeps only the parent's own hash).
fn parent_link(rpc: &Rpc, height: u32) -> Result<(String, u64)> {
    let hash = rpc.call("getblockhash", json!([height]))?;
    let b = rpc.call("getblock", json!([hash, 2]))?;
    let aux = &b["auxpow"];
    let prev = aux["parentblock"]["previousblockhash"].as_str().context("parent previousblockhash")?.to_string();
    let script = hex_bytes(aux["tx"]["vin"][0]["coinbase"].as_str().unwrap_or_default());
    let height = match script.first() {
        Some(&n @ 1..=8) => script[1..=n as usize].iter().rev().fold(0u64, |a, &b| a << 8 | b as u64),
        _ => 0,
    };
    Ok((prev, height))
}

fn check_parents(label: &str, rpc: &Rpc, dash: &mut Insight, heights: &[u32]) -> Result<ParentCheck> {
    let mut c = ParentCheck { label: label.to_string(), sampled: heights.len(), on_dash: 0, mismatched: 0, not_found: 0 };
    for &h in heights {
        let (prev, parent_height) = parent_link(rpc, h)?;
        match dash.get(&format!("block/{prev}"))? {
            None => c.not_found += 1,
            Some(b) if b["isMainChain"].as_bool() == Some(true) && b["height"].as_u64() == Some(parent_height.wrapping_sub(1)) => {
                c.on_dash += 1
            }
            Some(_) => c.mismatched += 1,
        }
    }
    Ok(c)
}

/// Up to `n` items spread evenly over `items`.
fn spread<T: Copy>(items: &[T], n: usize) -> Vec<T> {
    if items.len() <= n {
        return items.to_vec();
    }
    (0..n).map(|i| items[i * (items.len() - 1) / (n - 1).max(1)]).collect()
}

/// Dash height at a time, interpolated from the Dash-range parents both aux chains recorded.
struct DashClock(Vec<(i64, u64)>);

impl DashClock {
    fn new(blocks: &[&[Block]]) -> Self {
        let mut points: Vec<(i64, u64)> = blocks
            .iter()
            .flat_map(|bs| bs.iter())
            .filter(|b| b.parent_height >= 2_000_000)
            .map(|b| (b.parent_time, b.parent_height))
            .collect();
        points.sort();
        // Shares are stamped by many machines; a running max keeps heights monotonic in time.
        for i in 1..points.len() {
            points[i].1 = points[i].1.max(points[i - 1].1);
        }
        Self(points)
    }

    fn height_at(&self, time: i64) -> u64 {
        let i = self.0.partition_point(|p| p.0 < time).min(self.0.len() - 1);
        self.0[i].1
    }
}

fn dash_shares(dash: &mut Insight, clock: &DashClock, from: i64, to: i64) -> Result<(usize, Vec<(String, f64)>)> {
    let heights: Vec<u64> = (clock.height_at(from)..clock.height_at(to)).collect();
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut n = 0;
    for h in spread(&heights, DASH_SAMPLE) {
        let b = dash.block_at(h)?;
        if b.time >= from - HOUR && b.time < to + HOUR {
            *counts.entry(b.pool).or_default() += 1;
            n += 1;
        }
    }
    let mut shares: Vec<(String, f64)> = counts.into_iter().map(|(p, c)| (p, c as f64 / n.max(1) as f64)).collect();
    shares.sort_by(|a, b| b.1.total_cmp(&a.1));
    Ok((n, shares))
}

fn side(label: &str, maximus: &[Block], osmium: &[Block], dash: &mut Insight, clock: &DashClock, from: i64, to: i64) -> Result<Side> {
    let pick = |bs: &[Block]| -> Vec<Block> { bs.iter().filter(|b| b.time >= from && b.time < to).cloned().collect() };
    let (m, o) = (pick(maximus), pick(osmium));
    let hours = (to - from) as f64 / HOUR as f64;
    let (dash_sampled, dash) = dash_shares(dash, clock, from, to)?;
    Ok(Side {
        maximus_per_hour: m.len() as f64 / hours,
        osmium_per_hour: o.len() as f64 / hours,
        maximus: window(label.into(), &m.iter().collect::<Vec<_>>()),
        osmium: window(label.into(), &o.iter().collect::<Vec<_>>()),
        dash_sampled,
        dash,
    })
}

pub fn build(
    rpc: &Rpc,
    osmium_rpc: &Rpc,
    dash: &mut Insight,
    maximus: &[Block],
    osmium: &[Block],
    crashes: &Crashes,
) -> Result<MergeMining> {
    let now = maximus.last().unwrap().time;
    let osmium_parents: HashSet<&str> = osmium.iter().filter(|b| b.auxpow()).map(|b| b.parent_hash.as_str()).collect();
    let recent: Vec<&Block> = maximus.iter().filter(|b| b.auxpow() && b.time > now - 90 * 24 * HOUR).collect();
    let shared_parents_90d = recent.iter().filter(|b| osmium_parents.contains(b.parent_hash.as_str())).count() as f64 / recent.len() as f64;

    let heights = |bs: &[Block], dash_range: bool| -> Vec<u32> {
        bs.iter().filter(|b| b.auxpow() && (b.parent_height >= 2_000_000) == dash_range).map(|b| b.height).collect()
    };
    let checks = vec![
        check_parents("Maximus parents, Dash height range", rpc, dash, &spread(&heights(maximus, true), 40))?,
        check_parents("Maximus parents, height < 1M", rpc, dash, &spread(&heights(maximus, false), 20))?,
        check_parents("Osmium parents, Dash height range", osmium_rpc, dash, &spread(&heights(osmium, true), 20))?,
    ];

    let clock = DashClock::new(&[maximus, osmium]);
    let mut collapses = Vec::new();
    for e in crashes.episodes.iter().filter(|e| e.depth >= 0.9) {
        let end = e.start + e.hours as i64 * HOUR;
        let before = side("week before", maximus, osmium, dash, &clock, e.start - 7 * 24 * HOUR, e.start)?;
        let during = side("during", maximus, osmium, dash, &clock, e.start, end)?;
        let inside: Vec<Block> = osmium.iter().filter(|b| b.time >= e.start && b.time < end).cloned().collect();
        let osmium_parents = check_parents("Osmium parents during the collapse", osmium_rpc, dash, &spread(&heights(&inside, true), 8))?;
        eprintln!("collapse {} done ({} explorer requests so far)", fmt_time(e.start), dash.requests);
        collapses.push(Collapse { start: e.start, hours: e.hours, depth: e.depth, before, during, osmium_parents });
    }
    Ok(MergeMining { osmium_tip: osmium.last().map_or(0, |b| b.height), shared_parents_90d, checks, collapses })
}

fn top(shares: impl Iterator<Item = (String, f64)>, n: usize) -> String {
    shares.take(n).map(|(p, s)| format!("{p} {:.0}%", 100.0 * s)).collect::<Vec<_>>().join(", ")
}

fn print_side(s: &Side) {
    let pools = |w: &Window| top(w.pools.iter().map(|p| (p.pool.clone(), p.share)), 3);
    println!("    Maximus {:>6.1} blocks/h  {}", s.maximus_per_hour, pools(&s.maximus));
    println!("    Osmium  {:>6.1} blocks/h  {}", s.osmium_per_hour, pools(&s.osmium));
    println!("    Dash    {:>6} sampled   {}", s.dash_sampled, top(s.dash.iter().cloned(), 4));
}

pub fn print(m: &MergeMining) {
    println!(
        "Osmium tip {}. Maximus AuxPoW blocks of the last 90 days sharing their parent with an Osmium block: {:.1}%",
        m.osmium_tip,
        100.0 * m.shared_parents_90d
    );
    println!("\n== Parent check: is the block each parent builds on a Dash main-chain block, one below the parent's height?");
    for c in &m.checks {
        println!(
            "  {:<40} {:>3} sampled: {:>3} on Dash, {:>3} mismatched, {:>3} not on Dash",
            c.label, c.sampled, c.on_dash, c.mismatched, c.not_found
        );
    }
    for c in &m.collapses {
        println!("\n== Maximus collapse of {} ({:.0} h, {:.1}% down)", fmt_time(c.start), c.hours, 100.0 * c.depth);
        println!("  week before:");
        print_side(&c.before);
        println!("  during:");
        print_side(&c.during);
        let p = &c.osmium_parents;
        println!(
            "  Osmium parents during it: {} sampled, {} on Dash, {} mismatched, {} not on Dash",
            p.sampled, p.on_dash, p.mismatched, p.not_found
        );
    }
}
