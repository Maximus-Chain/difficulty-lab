//! Who mines Maximus: pool tags from the coinbase scriptSig, the payout address, and the AuxPoW
//! data (parent block and its coinbase) of every block, read via RPC and cached in a CSV.
//!
//! Since AuxPoW activated, Maximus coinbases carry only the height; the pool's tag is in the
//! parent chain's coinbase, next to the merged-mining marker (fabe6d6d, the aux chains' merkle
//! root, and the number of slots in that tree).

use crate::chain::Header;
use crate::crashes::Crashes;
use crate::fmt_time;
use crate::par::parallel_with;
use crate::rpc::Rpc;
use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

const DAY: i64 = 86_400;
pub const VERSION_AUXPOW: u32 = 1 << 8;
pub const CHAIN_ID: u32 = 0x45;

#[derive(Clone, Debug)]
pub struct Block {
    pub height: u32,
    pub time: i64,
    pub version: u32,
    /// Empty when the block has no AuxPoW.
    pub parent_hash: String,
    pub parent_time: i64,
    pub parent_bits: u32,
    /// BIP34 height in the parent coinbase.
    pub parent_height: u64,
    /// Slots in the merged-mining tree (how many aux chains the pool can merge), 0 if absent.
    pub mm_size: u32,
    /// Printable text of the coinbase that carries the pool tag (the parent's for AuxPoW blocks).
    pub tag: String,
    /// Maximus address of the largest coinbase output (the miner's reward).
    pub payout: String,
    /// Script of the parent coinbase's largest output.
    pub parent_payout: String,
}

impl Block {
    pub fn auxpow(&self) -> bool {
        self.version & VERSION_AUXPOW != 0
    }

    pub fn chain_id(&self) -> u32 {
        self.version >> 16
    }

    pub fn pool(&self) -> String {
        pool_name(&self.tag)
    }

    /// Parent chain, told apart by height: Dash was past 2.1 million blocks when Maximus launched,
    /// while the other X11 chains seen as parents are all below 1 million.
    pub fn parent_chain(&self) -> &'static str {
        match (self.auxpow(), self.parent_height) {
            (false, _) => "none (mined directly)",
            (true, h) if h >= 2_000_000 => "Dash height range",
            (true, _) => "other chain (height < 1M)",
        }
    }
}

/// Pool name: the longest `/name/` in the tag, else a known pool software or pool name, else
/// "untagged". Pools put random extranonce bytes around their tag, and those bytes sometimes
/// form a short `/xx/` of their own, so the first match is not enough.
pub fn pool_name(tag: &str) -> String {
    let name_char = |c: char| c.is_ascii_alphanumeric() || ".-_".contains(c);
    let named = tag
        .match_indices('/')
        .filter_map(|(start, _)| {
            let rest = &tag[start + 1..];
            let len = rest.find('/')?;
            ((3..=32).contains(&len) && rest[..len].chars().all(name_char)).then(|| &rest[..len])
        })
        .max_by_key(|name| name.len());
    if let Some(name) = named {
        return name.to_string();
    }
    const KNOWN: [&str; 4] = ["Miningcore", "zergpool.com", "yiimp", "Getablocks"];
    if let Some(name) = KNOWN.iter().find(|k| tag.contains(*k)) {
        return name.to_string();
    }
    if tag.contains("OSMA") { "OSMA (no pool name)".into() } else { "untagged".into() }
}

pub fn hex_bytes(hex: &str) -> Vec<u8> {
    (0..hex.len() / 2).filter_map(|i| u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).ok()).collect()
}

/// BIP34 height: the first push of the scriptSig, little endian.
fn bip34_height(script: &[u8]) -> u64 {
    match script.first() {
        Some(&n @ 1..=8) if script.len() > n as usize => script[1..=n as usize].iter().rev().fold(0, |a, &b| a << 8 | b as u64),
        Some(&op @ 0x51..=0x60) => (op - 0x50) as u64,
        _ => 0,
    }
}

/// Runs of 4+ printable ASCII characters with at least two letters, after the height push.
pub fn tag(script: &[u8]) -> String {
    let skip = script.first().map_or(0, |&n| if (1..=8).contains(&n) { n as usize + 1 } else { 1 });
    let mut runs = Vec::new();
    for run in script.get(skip..).unwrap_or_default().split(|b| !(0x20..0x7f).contains(b)) {
        let text = String::from_utf8_lossy(run).trim().replace(',', ";");
        if text.len() >= 4 && text.chars().filter(char::is_ascii_alphabetic).count() >= 2 {
            runs.push(text);
        }
    }
    runs.join(" ")
}

fn mm_size(script: &[u8]) -> u32 {
    const MARKER: [u8; 4] = [0xfa, 0xbe, 0x6d, 0x6d];
    script
        .windows(4)
        .position(|w| w == MARKER)
        .and_then(|p| script.get(p + 36..p + 40))
        .map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap()))
}

fn largest_output(tx: &Value) -> &Value {
    tx["vout"].as_array().and_then(|v| v.iter().max_by_key(|o| o["valueSat"].as_u64().unwrap_or(0))).unwrap_or(&Value::Null)
}

fn fetch(rpc: &Rpc, height: u32) -> Result<Block> {
    let hash = rpc.call("getblockhash", json!([height]))?;
    let b = rpc.call("getblock", json!([hash, 2]))?;
    let coinbase = &b["tx"][0];
    let own_script = hex_bytes(coinbase["vin"][0]["coinbase"].as_str().unwrap_or_default());
    let out = largest_output(coinbase);
    let payout = out["scriptPubKey"]["addresses"][0].as_str().unwrap_or_default().to_string();
    let mut block = Block {
        height,
        time: b["time"].as_i64().context("time")?,
        version: b["version"].as_i64().context("version")? as u32,
        parent_hash: String::new(),
        parent_time: 0,
        parent_bits: 0,
        parent_height: 0,
        mm_size: mm_size(&own_script),
        tag: tag(&own_script),
        payout,
        parent_payout: String::new(),
    };
    let aux = &b["auxpow"];
    if !aux.is_null() {
        let parent = &aux["parentblock"];
        let script = hex_bytes(aux["tx"]["vin"][0]["coinbase"].as_str().unwrap_or_default());
        block.parent_hash = parent["hash"].as_str().unwrap_or_default().to_string();
        block.parent_time = parent["time"].as_i64().unwrap_or(0);
        block.parent_bits = u32::from_str_radix(parent["bits"].as_str().unwrap_or("0"), 16)?;
        block.parent_height = bip34_height(&script);
        block.mm_size = mm_size(&script);
        block.tag = tag(&script);
        block.parent_payout = largest_output(&aux["tx"])["scriptPubKey"]["hex"].as_str().unwrap_or_default().to_string();
    }
    Ok(block)
}

const CSV_HEADER: &str = "height,time,version,parent_hash,parent_time,parent_bits,parent_height,mm_size,tag,payout,parent_payout";

fn read_cache(path: &Path) -> Result<BTreeMap<u32, Block>> {
    let mut blocks = BTreeMap::new();
    let Ok(file) = std::fs::File::open(path) else {
        return Ok(blocks);
    };
    for line in BufReader::new(file).lines().skip(1) {
        let line = line?;
        let c: Vec<&str> = line.split(',').collect();
        anyhow::ensure!(c.len() == 11, "bad line in {}: {line}", path.display());
        let b = Block {
            height: c[0].parse()?,
            time: c[1].parse()?,
            version: c[2].parse()?,
            parent_hash: c[3].to_string(),
            parent_time: c[4].parse()?,
            parent_bits: u32::from_str_radix(c[5], 16)?,
            parent_height: c[6].parse()?,
            mm_size: c[7].parse()?,
            tag: c[8].to_string(),
            payout: c[9].to_string(),
            parent_payout: c[10].to_string(),
        };
        blocks.insert(b.height, b);
    }
    Ok(blocks)
}

/// Every block from `from` to the tip, fetching only those missing from the cache.
pub fn load(rpc: &Rpc, cache: &Path, from: u32, tip: u32) -> Result<Vec<Block>> {
    let mut blocks = read_cache(cache)?;
    let missing: Vec<u32> = (from..=tip).filter(|h| !blocks.contains_key(h)).collect();
    if !cache.exists() {
        std::fs::write(cache, format!("{CSV_HEADER}\n"))?;
    }
    let mut file = OpenOptions::new().append(true).open(cache)?;
    for chunk in missing.chunks(20_000) {
        let fetched = parallel_with(chunk.len(), Some(16), |i| fetch(rpc, chunk[i]));
        for b in fetched {
            let b = b?;
            writeln!(
                file,
                "{},{},{},{},{},{:08x},{},{},{},{},{}",
                b.height,
                b.time,
                b.version,
                b.parent_hash,
                b.parent_time,
                b.parent_bits,
                b.parent_height,
                b.mm_size,
                b.tag,
                b.payout,
                b.parent_payout
            )?;
            blocks.insert(b.height, b);
        }
        eprint!("\rfetched up to height {}", chunk.last().unwrap());
    }
    if !missing.is_empty() {
        eprintln!();
    }
    Ok(blocks.into_values().filter(|b| b.height >= from && b.height <= tip).collect())
}

#[derive(Serialize)]
pub struct PoolShare {
    pub pool: String,
    pub blocks: usize,
    pub share: f64,
    pub addresses: usize,
    pub mm_size: u32,
    pub parent_heights: (u64, u64),
}

#[derive(Serialize)]
pub struct Window {
    pub label: String,
    pub from: i64,
    pub to: i64,
    pub blocks: usize,
    pub auxpow: f64,
    pub chain_id: f64,
    /// Share of blocks per parent chain, largest first.
    pub parents: Vec<(String, f64)>,
    pub pools: Vec<PoolShare>,
    /// Herfindahl-Hirschman index of block shares (1 = a single pool, 1/n = n equal pools).
    pub hhi: f64,
    /// Fewest pools that together found more than half the blocks.
    pub nakamoto: usize,
}

#[derive(Serialize)]
pub struct Period {
    pub start: i64,
    pub blocks: usize,
    pub auxpow: f64,
    /// Share of each pool in `Miners::tracked`, then everyone else.
    pub shares: Vec<f64>,
}

#[derive(Serialize)]
pub struct Miners {
    pub tip: u32,
    pub windows: Vec<Window>,
    /// Pools followed over time: the largest in the last 90 days and over the whole history.
    pub tracked: Vec<String>,
    pub weekly: Vec<Period>,
    pub monthly: Vec<Period>,
    /// Pool shares the week before and the first week of every collapse deeper than 90%.
    pub collapses: Vec<(i64, Window, Window)>,
}

pub fn window(label: String, blocks: &[&Block]) -> Window {
    let mut by_pool: HashMap<String, Vec<&Block>> = HashMap::new();
    for b in blocks {
        by_pool.entry(b.pool()).or_default().push(b);
    }
    let n = blocks.len().max(1) as f64;
    let mut pools: Vec<PoolShare> = by_pool
        .into_iter()
        .map(|(pool, bs)| {
            let mut addresses: Vec<&str> = bs.iter().map(|b| b.payout.as_str()).collect();
            addresses.sort();
            addresses.dedup();
            let mut sizes: HashMap<u32, usize> = HashMap::new();
            for b in &bs {
                *sizes.entry(b.mm_size).or_default() += 1;
            }
            let heights = bs.iter().filter(|b| b.parent_height > 0).map(|b| b.parent_height);
            PoolShare {
                blocks: bs.len(),
                share: bs.len() as f64 / n,
                addresses: addresses.len(),
                mm_size: sizes.into_iter().max_by_key(|&(_, c)| c).map_or(0, |(s, _)| s),
                parent_heights: (heights.clone().min().unwrap_or(0), heights.max().unwrap_or(0)),
                pool,
            }
        })
        .collect();
    pools.sort_by(|a, b| b.blocks.cmp(&a.blocks).then(a.pool.cmp(&b.pool)));
    let nakamoto = pools
        .iter()
        .scan(0.0, |cumulative, p| {
            let before = *cumulative;
            *cumulative += p.share;
            (before <= 0.5).then_some(())
        })
        .count();
    Window {
        label,
        from: blocks.first().map_or(0, |b| b.time),
        to: blocks.last().map_or(0, |b| b.time),
        blocks: blocks.len(),
        auxpow: blocks.iter().filter(|b| b.auxpow()).count() as f64 / n,
        chain_id: blocks.iter().filter(|b| b.chain_id() == CHAIN_ID).count() as f64 / n,
        parents: {
            let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
            for b in blocks {
                *counts.entry(b.parent_chain()).or_default() += 1;
            }
            let mut v: Vec<(String, f64)> = counts.into_iter().map(|(k, c)| (k.to_string(), c as f64 / n)).collect();
            v.sort_by(|a, b| b.1.total_cmp(&a.1));
            v
        },
        hhi: pools.iter().map(|p| p.share * p.share).sum(),
        nakamoto,
        pools,
    }
}

fn periods(blocks: &[Block], length: i64, tracked: &[String]) -> Vec<Period> {
    let mut out: Vec<Period> = Vec::new();
    let start = blocks[0].time.div_euclid(length) * length;
    for chunk in blocks.chunk_by(|a, b| (a.time - start).div_euclid(length) == (b.time - start).div_euclid(length)) {
        let n = chunk.len() as f64;
        let mut shares: Vec<f64> = tracked.iter().map(|p| chunk.iter().filter(|b| &b.pool() == p).count() as f64 / n).collect();
        shares.push(1.0 - shares.iter().sum::<f64>());
        out.push(Period {
            start: start + (chunk[0].time - start).div_euclid(length) * length,
            blocks: chunk.len(),
            auxpow: chunk.iter().filter(|b| b.auxpow()).count() as f64 / n,
            shares,
        });
    }
    out
}

pub fn build(blocks: &[Block], crashes: &Crashes) -> Miners {
    let now = blocks.last().unwrap().time;
    let since = |days: i64| -> Vec<&Block> { blocks.iter().filter(|b| b.time > now - days * DAY).collect() };
    let windows = vec![
        window("last 30 days".into(), &since(30)),
        window("last 90 days".into(), &since(90)),
        window("whole history".into(), &blocks.iter().collect::<Vec<_>>()),
    ];
    let mut tracked: Vec<String> = windows[1].pools.iter().take(6).map(|p| p.pool.clone()).collect();
    for p in windows[2].pools.iter().take(6) {
        if !tracked.contains(&p.pool) {
            tracked.push(p.pool.clone());
        }
    }
    let recent: Vec<Block> = since(13 * 7).into_iter().cloned().collect();
    let collapses = crashes
        .episodes
        .iter()
        .filter(|e| e.depth >= 0.9)
        .map(|e| {
            let before: Vec<&Block> = blocks.iter().filter(|b| b.time >= e.start - 7 * DAY && b.time < e.start).collect();
            let end = e.start + (e.hours as i64 * 3_600).min(7 * DAY);
            let during: Vec<&Block> = blocks.iter().filter(|b| b.time >= e.start && b.time < end).collect();
            (e.start, window("week before".into(), &before), window("during (up to 7 days)".into(), &during))
        })
        .collect();
    Miners {
        tip: blocks.last().unwrap().height,
        weekly: periods(&recent, 7 * DAY, &tracked),
        monthly: periods(blocks, 30 * DAY, &tracked),
        windows,
        tracked,
        collapses,
    }
}

fn print_window(w: &Window, top: usize) {
    println!(
        "  {} blocks, {} to {}. AuxPoW {:.1}%, chain ID 0x45 {:.1}%. HHI {:.3} (~{:.1} equal pools), {} pool(s) above 50%.",
        w.blocks,
        fmt_time(w.from),
        fmt_time(w.to),
        100.0 * w.auxpow,
        100.0 * w.chain_id,
        w.hhi,
        1.0 / w.hhi,
        w.nakamoto
    );
    let parents: Vec<String> = w.parents.iter().map(|(c, f)| format!("{c} {:.1}%", 100.0 * f)).collect();
    println!("  parent chain: {}", parents.join(", "));
    println!("  {:<32} {:>7} {:>7} {:>6} {:>5}  {}", "pool", "blocks", "share", "addrs", "mm", "parent heights");
    for p in w.pools.iter().take(top) {
        println!(
            "  {:<32} {:>7} {:>6.1}% {:>6} {:>5}  {}..{}",
            p.pool.chars().take(32).collect::<String>(),
            p.blocks,
            100.0 * p.share,
            p.addresses,
            p.mm_size,
            p.parent_heights.0,
            p.parent_heights.1
        );
    }
    let rest: usize = w.pools.iter().skip(top).map(|p| p.blocks).sum();
    if rest > 0 {
        println!("  {:<32} {:>7} {:>6.1}%  ({} more pools)", "others", rest, 100.0 * rest as f64 / w.blocks as f64, w.pools.len() - top);
    }
}

fn print_periods(m: &Miners, periods: &[Period]) {
    print!("  {:<10} {:>6} {:>6}", "from", "blocks", "auxpow");
    for p in &m.tracked {
        print!(" {:>12}", p.chars().take(12).collect::<String>());
    }
    println!(" {:>12}", "others");
    for p in periods {
        print!("  {:<10} {:>6} {:>5.0}%", &fmt_time(p.start)[..10], p.blocks, 100.0 * p.auxpow);
        for s in &p.shares {
            print!(" {:>11.1}%", 100.0 * s);
        }
        println!();
    }
}

pub fn print(m: &Miners) {
    for w in &m.windows {
        println!("\n== {}", w.label);
        print_window(w, 12);
    }
    println!("\n== Weekly shares, last 13 weeks");
    print_periods(m, &m.weekly);
    println!("\n== Shares every 30 days, whole history");
    print_periods(m, &m.monthly);
    for (start, before, during) in &m.collapses {
        println!("\n== Collapse of {}: who was mining", fmt_time(*start));
        println!(" {}:", before.label);
        print_window(before, 6);
        println!(" {}:", during.label);
        print_window(during, 6);
    }
}

/// Heights the headers CSV already covers; blocks past it are left for the next `extract`.
pub fn tip_of(headers: &[Header]) -> u32 {
    headers.last().map_or(0, |h| h.height)
}
