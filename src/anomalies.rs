//! Chain anomaly scan: long gaps, timestamp oddities, DGW clamp hits, whether long gaps are
//! followed by bursts, solvetime autocorrelation, time-of-day patterns and hashrate collapses.

use crate::chain::Header;
use crate::fmt_time;
use crate::hashrate::work;
use crate::target::difficulty;

const T: i64 = 72;
const DGW_WINDOW: usize = 24;

fn mean(values: impl IntoIterator<Item = f64>) -> f64 {
    let (sum, n) = values.into_iter().fold((0.0, 0usize), |(s, n), v| (s + v, n + 1));
    sum / n.max(1) as f64
}

/// Hashrate over a whole UTC day: total work of its blocks divided by 86,400 s.
struct Day {
    start: i64,
    blocks: usize,
    mean_solvetime: f64,
    hashrate: f64,
}

fn days(headers: &[Header], from: usize) -> Vec<Day> {
    let mut out: Vec<Day> = Vec::new();
    let mut acc = (0i64, 0usize, 0i64, 0.0f64);
    let flush = |out: &mut Vec<Day>, (day, blocks, solvetimes, w): (i64, usize, i64, f64)| {
        if blocks > 50 {
            out.push(Day { start: day * 86_400, blocks, mean_solvetime: solvetimes as f64 / blocks as f64, hashrate: w / 86_400.0 });
        }
    };
    for i in from..headers.len() {
        let day = headers[i].time.div_euclid(86_400);
        if day != acc.0 && acc.1 > 0 {
            flush(&mut out, acc);
            acc = (day, 0, 0, 0.0);
        }
        acc.0 = day;
        acc.1 += 1;
        acc.2 += headers[i].time - headers[i - 1].time;
        acc.3 += work(headers[i].bits);
    }
    // The last day is partial; leave it out.
    out
}

pub fn report(headers: &[Header], start_height: usize) {
    let st = |i: usize| headers[i].time - headers[i - 1].time;
    let range = start_height..headers.len();

    println!("== Longest gaps");
    let mut by_gap: Vec<usize> = range.clone().collect();
    by_gap.sort_by_key(|&i| -st(i));
    for &i in by_gap.iter().take(12) {
        println!(
            "  height {i:>7}  {}  {:>5.0} min  difficulty {:>12.0} -> next block {:>12.0}",
            fmt_time(headers[i - 1].time),
            st(i) as f64 / 60.0,
            difficulty(headers[i].bits),
            headers.get(i + 1).map_or(f64::NAN, |h| difficulty(h.bits)),
        );
    }

    println!("\n== Timestamps");
    let zero = range.clone().filter(|&i| st(i) == 0).count();
    let negative = range.clone().filter(|&i| st(i) < 0).count();
    let (mut ahead_count, mut worst) = (0, (0i64, 0usize));
    for i in start_height..headers.len().saturating_sub(10) {
        let ahead = headers[i].time - headers[i + 1..=i + 10].iter().map(|h| h.time).min().unwrap();
        if ahead > 0 {
            ahead_count += 1;
            if ahead > worst.0 {
                worst = (ahead, i);
            }
        }
    }
    println!("  same second as previous block: {zero}, earlier than previous block: {negative}");
    println!("  later than one of the next 10 blocks: {ahead_count} (worst {} s at height {})", worst.0, worst.1);

    println!("\n== DGW clamp hits (24-block timespan outside [T*24/3, T*24*3])");
    let (mut fast, mut slow) = (0, 0);
    for i in start_height.max(DGW_WINDOW + 1)..headers.len() {
        let span = headers[i - 1].time - headers[i - DGW_WINDOW].time;
        if span < DGW_WINDOW as i64 * T / 3 {
            fast += 1;
        }
        if span > DGW_WINDOW as i64 * T * 3 {
            slow += 1;
        }
    }
    println!("  difficulty capped at x3 up: {fast} blocks, capped at /3 down: {slow} blocks");

    println!("\n== Do long gaps (> 7 min) come with bursts right after?");
    let end = headers.len() - 3;
    let next3 = |i: usize| (headers[i + 3].time - headers[i].time) as f64;
    let after_gap: Vec<f64> = (start_height..end).filter(|&i| st(i) > 420).map(next3).collect();
    let all: Vec<f64> = (start_height..end).map(next3).collect();
    let share_fast = |v: &[f64]| 100.0 * v.iter().filter(|&&x| x < 60.0).count() as f64 / v.len() as f64;
    println!(
        "  {} gaps; next 3 blocks within 60 s: {:.1}% (any block: {:.1}%); mean time for the next 3: {:.0} s (any block: {:.0} s)",
        after_gap.len(),
        share_fast(&after_gap),
        share_fast(&all),
        mean(after_gap.iter().copied()),
        mean(all.iter().copied())
    );

    println!("\n== Solvetime autocorrelation (0 = pure luck, > 0 = slow blocks cluster)");
    let x: Vec<f64> = range.clone().map(|i| st(i) as f64).collect();
    let mu = mean(x.iter().copied());
    let var = mean(x.iter().map(|v| (v - mu).powi(2)));
    for lag in 1..=5 {
        let c = mean((0..x.len() - lag).map(|k| (x[k] - mu) * (x[k + lag] - mu))) / var;
        println!("  lag {lag}: {c:+.4}");
    }

    let year_ago = headers.last().unwrap().time - 365 * 86_400;
    let recent: Vec<usize> = range.clone().filter(|&i| headers[i].time >= year_ago).collect();
    println!("\n== Mean block time by UTC hour, last 365 days");
    let line: Vec<String> = (0..24)
        .map(|h| format!("{h:02}:{:.0}", mean(recent.iter().filter(|&&i| headers[i].time.rem_euclid(86_400) / 3600 == h).map(|&i| st(i) as f64))))
        .collect();
    println!("  {}", line.join(" "));
    println!("== By weekday, last 365 days");
    // 1970-01-01 was a Thursday.
    let names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let line: Vec<String> = (0..7)
        .map(|d| {
            let m = mean(recent.iter().filter(|&&i| (headers[i].time.div_euclid(86_400) + 3).rem_euclid(7) == d).map(|&i| st(i) as f64));
            format!("{}:{m:.1}", names[d as usize])
        })
        .collect();
    println!("  {}", line.join(" "));

    println!("\n== Slowest days");
    let days = days(headers, start_height);
    let mut slowest: Vec<&Day> = days.iter().collect();
    slowest.sort_by(|a, b| b.mean_solvetime.total_cmp(&a.mean_solvetime));
    for d in slowest.iter().take(8) {
        println!("  {}  {:>4} blocks  mean {:>4.0} s  hashrate {:>8.1} TH/s", &fmt_time(d.start)[..10], d.blocks, d.mean_solvetime, d.hashrate / 1e12);
    }
    let mut swings: Vec<(f64, i64)> = days.windows(2).map(|w| (w[1].hashrate / w[0].hashrate, w[1].start)).collect();
    swings.sort_by(|a, b| a.0.total_cmp(&b.0));
    let fmt_swing = |s: &[(f64, i64)]| s.iter().map(|(r, t)| format!("{} x{r:.2}", &fmt_time(*t)[..10])).collect::<Vec<_>>().join(", ");
    println!("  biggest day-to-day hashrate drops: {}", fmt_swing(&swings[..5]));
    println!("  biggest day-to-day hashrate rises: {}", fmt_swing(&swings[swings.len() - 5..]));

    if let Some(crash) = biggest_crash(headers, start_height) {
        println!("\n== Biggest hashrate collapse: block by block from height {}", crash);
        println!("  {:>7}  {:<16}  {:>8}  {:>12}", "height", "time (UTC)", "took", "difficulty");
        for i in crash - 3..(crash + 18).min(headers.len()) {
            println!("  {i:>7}  {}  {:>7}s  {:>12.0}", fmt_time(headers[i].time), st(i), difficulty(headers[i].bits));
        }
        if let Some(back) = (crash..headers.len() - DGW_WINDOW).find(|&i| headers[i + DGW_WINDOW].time - headers[i].time < 2 * T * DGW_WINDOW as i64) {
            println!(
                "  back to a normal pace (24 blocks averaging < 2*T) at height {back}, {:.1} h after the collapse",
                (headers[back].time - headers[crash - 1].time) as f64 / 3600.0
            );
        }
    }
}

/// The block that starts the sharpest collapse: the largest ratio between the work rate of the
/// 100 blocks before it and the time the following 10 blocks actually took.
pub fn biggest_crash(headers: &[Header], start_height: usize) -> Option<usize> {
    const BEFORE: usize = 100;
    const AFTER: usize = 10;
    (start_height.max(BEFORE)..headers.len() - AFTER)
        .map(|i| {
            let before = (headers[i - 1].time - headers[i - BEFORE].time).max(1) as f64 / (BEFORE - 1) as f64;
            let after = (headers[i + AFTER - 1].time - headers[i - 1].time).max(1) as f64 / AFTER as f64;
            (after / before, i)
        })
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, i)| i)
}
