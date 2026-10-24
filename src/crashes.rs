//! Real hashrate collapses: how deep and how long every drop of the chain's history was.
//!
//! Hashrate is measured on an hourly grid. Each block's work is spread evenly over the interval
//! it took to find (previous timestamp to its own), so an hour with no block still gets the
//! share of a slow block that spans it: during a deep collapse the estimate stays meaningful
//! instead of dropping to zero between blocks.
//!
//! An episode is a run of hours below half the baseline, the baseline being the median hourly
//! hashrate over the 24 h before the episode starts. Runs separated by a single normal hour are
//! merged. Depth is the drop of the episode's median hour against that baseline (a mean would be
//! dragged by the burst of fast blocks when miners return); the peak is the drop of its worst hour.

use crate::chain::Header;
use crate::fmt_time;
use crate::hashrate::work;
use serde::Serialize;

const HOUR: i64 = 3_600;
const BASELINE_HOURS: usize = 24;
/// An hour counts as collapsed below this fraction of the baseline.
pub const THRESHOLD: f64 = 0.5;

#[derive(Serialize, Clone)]
pub struct Episode {
    pub start: i64,
    pub hours: f64,
    /// Median hourly hashrate over the 24 h before, hashes per second.
    pub baseline: f64,
    /// Median hourly hashrate during the episode.
    pub during: f64,
    /// Average hashrate during the episode (total work / duration).
    pub mean: f64,
    pub depth: f64,
    pub peak_depth: f64,
    pub first_height: u32,
    pub blocks: usize,
    /// Longest time between two blocks inside the episode, seconds.
    pub longest_gap: i64,
}

#[derive(Serialize)]
pub struct Bucket {
    pub label: &'static str,
    pub min_depth: f64,
    pub count: usize,
    pub median_hours: f64,
    pub max_hours: f64,
    pub total_hours: f64,
}

#[derive(Serialize)]
pub struct HourPoint {
    pub time: i64,
    pub hashrate: f64,
    pub blocks: usize,
}

#[derive(Serialize)]
pub struct Crashes {
    pub from: i64,
    pub to: i64,
    pub episodes: Vec<Episode>,
    pub buckets: Vec<Bucket>,
    /// The deepest episode, and the hourly hashrate from two days before to two days after it.
    pub deepest: Option<Episode>,
    pub deepest_hours: Vec<HourPoint>,
    /// Daily average hashrate from a week before the deepest episode to a week after it.
    pub deepest_days: Vec<HourPoint>,
}

/// Hourly hashrate from `from` (aligned to the hour) and the number of blocks stamped in each hour.
fn hourly(headers: &[Header], start_height: usize) -> (i64, Vec<f64>, Vec<usize>) {
    let from = headers[start_height - 1].time.div_euclid(HOUR) * HOUR;
    // Real timestamps are not monotonic; a running max gives every block a valid interval.
    let mut times = Vec::with_capacity(headers.len());
    let mut last = i64::MIN;
    for h in headers {
        last = last.max(h.time);
        times.push(last);
    }
    let n = ((times[headers.len() - 1] - from) / HOUR + 1) as usize;
    let (mut hashes, mut blocks) = (vec![0.0; n], vec![0usize; n]);
    let bin = |t: i64| ((t - from) / HOUR) as usize;
    for i in start_height..headers.len() {
        let (a, b, w) = (times[i - 1], times[i], work(headers[i].bits));
        blocks[bin(b)] += 1;
        if a == b {
            hashes[bin(b)] += w;
            continue;
        }
        let rate = w / (b - a) as f64;
        let mut t = a;
        while t < b {
            let end = (from + (bin(t) as i64 + 1) * HOUR).min(b);
            hashes[bin(t)] += rate * (end - t) as f64;
            t = end;
        }
    }
    let rates = hashes.into_iter().map(|h| h / HOUR as f64).collect();
    (from, rates, blocks)
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

pub fn build(headers: &[Header], start_height: usize) -> Crashes {
    let (from, rates, blocks) = hourly(headers, start_height);
    // The last hour is partial.
    let n = rates.len() - 1;
    let mut episodes = Vec::new();
    let mut h = BASELINE_HOURS;
    while h < n {
        let baseline = median(rates[h - BASELINE_HOURS..h].to_vec());
        if rates[h] >= THRESHOLD * baseline {
            h += 1;
            continue;
        }
        let start = h;
        let mut end = h + 1;
        while end < n && (rates[end] < THRESHOLD * baseline || (end + 1 < n && rates[end + 1] < THRESHOLD * baseline)) {
            end += 1;
        }
        let mean = rates[start..end].iter().sum::<f64>() / (end - start) as f64;
        let during = median(rates[start..end].to_vec());
        let worst = rates[start..end].iter().copied().fold(f64::INFINITY, f64::min);
        let (t0, t1) = (from + start as i64 * HOUR, from + end as i64 * HOUR);
        let inside: Vec<usize> = (start_height..headers.len()).filter(|&i| headers[i].time >= t0 && headers[i].time < t1).collect();
        let longest_gap = (start_height..headers.len())
            .filter(|&i| headers[i].time > t0 && headers[i - 1].time < t1)
            .map(|i| headers[i].time - headers[i - 1].time)
            .max()
            .unwrap_or(0);
        episodes.push(Episode {
            start: t0,
            hours: (end - start) as f64,
            baseline,
            during,
            mean,
            depth: 1.0 - during / baseline,
            peak_depth: 1.0 - worst / baseline,
            first_height: inside.first().map_or(0, |&i| i as u32),
            blocks: blocks[start..end].iter().sum(),
            longest_gap,
        });
        h = end;
    }

    let mut buckets: Vec<Bucket> = [("50-80%", 0.5, 0.8), ("80-95%", 0.8, 0.95), ("95-99%", 0.95, 0.99), ("99%+", 0.99, 1.01)]
        .into_iter()
        .map(|(label, lo, hi)| {
            let hours: Vec<f64> = episodes.iter().filter(|e| e.depth >= lo && e.depth < hi).map(|e| e.hours).collect();
            Bucket {
                label,
                min_depth: lo,
                count: hours.len(),
                median_hours: if hours.is_empty() { 0.0 } else { median(hours.clone()) },
                max_hours: hours.iter().copied().fold(0.0, f64::max),
                total_hours: hours.iter().fold(0.0, |a, h| a + h),
            }
        })
        .collect();
    // Episodes whose worst hours crossed the threshold but whose median hour did not are listed apart.
    let shallow = episodes.iter().filter(|e| e.depth < 0.5).count();
    if shallow > 0 {
        let hours: Vec<f64> = episodes.iter().filter(|e| e.depth < 0.5).map(|e| e.hours).collect();
        buckets.insert(
            0,
            Bucket {
                label: "<50% median",
                min_depth: 0.0,
                count: shallow,
                median_hours: median(hours.clone()),
                max_hours: hours.iter().copied().fold(0.0, f64::max),
                total_hours: hours.iter().sum::<f64>(),
            },
        );
    }

    let deepest = episodes.iter().max_by(|a, b| (a.depth * a.hours.sqrt()).total_cmp(&(b.depth * b.hours.sqrt()))).cloned();
    let deepest_hours = deepest.as_ref().map_or(Vec::new(), |e| {
        let first = ((e.start - from) / HOUR) as usize;
        (first.saturating_sub(48)..(first + e.hours as usize + 48).min(n))
            .map(|i| HourPoint { time: from + i as i64 * HOUR, hashrate: rates[i], blocks: blocks[i] })
            .collect()
    });
    let deepest_days = deepest.as_ref().map_or(Vec::new(), |e| {
        let first = ((e.start - from) / HOUR) as usize;
        let day = |d: usize| {
            let range = d..(d + 24).min(n);
            HourPoint {
                time: from + d as i64 * HOUR,
                hashrate: rates[range.clone()].iter().sum::<f64>() / range.len() as f64,
                blocks: blocks[range].iter().sum(),
            }
        };
        (first.saturating_sub(7 * 24)..(first + e.hours as usize + 7 * 24).min(n)).step_by(24).map(day).collect()
    });
    Crashes { from, to: from + n as i64 * HOUR, episodes, buckets, deepest, deepest_hours, deepest_days }
}

pub fn print(c: &Crashes) {
    let th = |v: f64| v / 1e12;
    println!(
        "Hourly hashrate {} to {} ({} days). Episode = hours below {:.0}% of the median of the previous 24 h.",
        fmt_time(c.from),
        fmt_time(c.to),
        (c.to - c.from) / 86_400,
        THRESHOLD * 100.0
    );

    println!("\n== Episodes by depth of the median hour");
    println!("  {:<10} {:>6} {:>14} {:>10} {:>12}", "depth", "count", "median length", "longest", "total hours");
    for b in &c.buckets {
        println!("  {:<10} {:>6} {:>12.0} h {:>8.0} h {:>12.0}", b.label, b.count, b.median_hours, b.max_hours, b.total_hours);
    }

    println!("\n== Every episode of 2 h or more, or deeper than 80%");
    println!(
        "  {:<16}  {:>7}  {:>6}  {:>10}  {:>10}  {:>6}  {:>6}  {:>6}  {:>8}",
        "start (UTC)", "height", "hours", "base TH/s", "during", "depth", "peak", "blocks", "max gap"
    );
    for e in c.episodes.iter().filter(|e| e.hours >= 2.0 || e.depth >= 0.8) {
        println!(
            "  {}  {:>7}  {:>6.0}  {:>10.2}  {:>10.3}  {:>5.1}%  {:>5.1}%  {:>6}  {:>6.0} m",
            fmt_time(e.start),
            e.first_height,
            e.hours,
            th(e.baseline),
            th(e.during),
            100.0 * e.depth,
            100.0 * e.peak_depth,
            e.blocks,
            e.longest_gap as f64 / 60.0
        );
    }

    if let Some(e) = &c.deepest {
        println!("\n== Deepest episode, day by day (24 h from the episode's start hour; TH/s, blocks)");
        for p in &c.deepest_days {
            let mark = if p.time >= e.start && p.time < e.start + e.hours as i64 * HOUR { '*' } else { ' ' };
            println!("  {mark} {}  {:>9.3}  {:>5}", fmt_time(p.time), th(p.hashrate), p.blocks);
        }
        println!("\n== Deepest episode, hour by hour around its start");
        for p in c.deepest_hours.iter().filter(|p| p.time >= e.start - 12 * HOUR && p.time < e.start + 24 * HOUR) {
            let mark = if p.time >= e.start && p.time < e.start + e.hours as i64 * HOUR { '*' } else { ' ' };
            println!("  {mark} {}  {:>9.3}  {:>3}", fmt_time(p.time), th(p.hashrate), p.blocks);
        }
    }
}
