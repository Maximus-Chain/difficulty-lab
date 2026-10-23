//! Minimal block headers (height, timestamp, nBits, transaction count) and their CSV format.

use anyhow::{Context, Result, bail};
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;

#[derive(Clone, Copy, Debug)]
pub struct Header {
    pub height: u32,
    pub time: i64,
    pub bits: u32,
    /// Transactions in the block, coinbase included. Simulated blocks use 1.
    pub n_tx: u32,
}

pub fn write_csv(path: &Path, headers: &[Header]) -> Result<()> {
    let mut out = BufWriter::new(File::create(path)?);
    writeln!(out, "height,time,bits,n_tx")?;
    for h in headers {
        writeln!(out, "{},{},{:08x},{}", h.height, h.time, h.bits, h.n_tx)?;
    }
    Ok(())
}

/// Reads the CSV and checks heights are contiguous from 0, so the vector can be indexed by height.
pub fn read_csv(path: &Path) -> Result<Vec<Header>> {
    let file = File::open(path).with_context(|| format!("opening {}", path.display()))?;
    let mut headers = Vec::new();
    for (i, line) in BufReader::new(file).lines().enumerate().skip(1) {
        let line = line?;
        let mut cols = line.split(',');
        let mut next = || cols.next().with_context(|| format!("line {} is incomplete", i + 1));
        let header = Header {
            height: next()?.parse()?,
            time: next()?.parse()?,
            bits: u32::from_str_radix(next()?, 16)?,
            n_tx: next()?.parse()?,
        };
        if header.height as usize != headers.len() {
            bail!("height {} out of order at line {}", header.height, i + 1);
        }
        headers.push(header);
    }
    Ok(headers)
}
