//! Minimal JSON-RPC client and header extraction via getblockheaders (batches of 2000).

use crate::chain::Header;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

pub struct Rpc {
    url: String,
    auth: String,
}

impl Rpc {
    pub fn new(url: &str, user: &str, password: &str) -> Self {
        use base64::Engine;
        let token = base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}"));
        Self { url: url.to_string(), auth: format!("Basic {token}") }
    }

    pub fn call(&self, method: &str, params: Value) -> Result<Value> {
        let body = json!({"jsonrpc": "1.0", "id": "diffsim", "method": method, "params": params});
        let resp: Value = match ureq::post(&self.url).set("Authorization", &self.auth).send_json(body) {
            Ok(r) => r.into_json()?,
            // The node answers HTTP 500 with the error in the JSON body.
            Err(ureq::Error::Status(_, r)) => r.into_json()?,
            Err(e) => return Err(e.into()),
        };
        if !resp["error"].is_null() {
            bail!("{method}: {}", resp["error"]);
        }
        Ok(resp["result"].clone())
    }
}

pub fn extract_headers(rpc: &Rpc) -> Result<Vec<Header>> {
    let tip = rpc.call("getblockcount", json!([]))?.as_u64().context("getblockcount")? as u32;
    let mut hash = rpc.call("getblockhash", json!([0]))?.as_str().context("getblockhash")?.to_string();
    let mut headers = Vec::with_capacity(tip as usize + 1);

    loop {
        let batch = rpc.call("getblockheaders", json!([hash, 2000, true]))?;
        let batch = batch.as_array().context("getblockheaders did not return an array")?;
        let mut next = None;
        for h in batch {
            let height = h["height"].as_u64().context("height")? as u32;
            if height > tip {
                break;
            }
            if height as usize != headers.len() {
                bail!("height gap: expected {}, got {height}", headers.len());
            }
            headers.push(Header {
                height,
                time: h["time"].as_i64().context("time")?,
                bits: u32::from_str_radix(h["bits"].as_str().context("bits")?, 16)?,
            });
            next = h["nextblockhash"].as_str().map(str::to_string);
        }
        eprint!("\r{} / {tip}", headers.len() - 1);
        match next {
            Some(n) if (headers.len() as u32) <= tip => hash = n,
            _ => break,
        }
    }
    eprintln!();
    Ok(headers)
}
