//! Minimal JSON-RPC client and header extraction via getblockheaders (batches of 2000).

use crate::chain::Header;
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};

pub struct Rpc {
    url: String,
    auth: String,
    /// Shared so connections are kept alive: a scan makes hundreds of thousands of calls, and a
    /// new TCP connection per call exhausts ports and gets reset by the node.
    agent: ureq::Agent,
}

impl Rpc {
    pub fn new(url: &str, user: &str, password: &str) -> Self {
        use base64::Engine;
        let token = base64::engine::general_purpose::STANDARD.encode(format!("{user}:{password}"));
        Self { url: url.to_string(), auth: format!("Basic {token}"), agent: ureq::AgentBuilder::new().max_idle_connections_per_host(16).build() }
    }

    pub fn call(&self, method: &str, params: Value) -> Result<Value> {
        const ATTEMPTS: u32 = 8;
        let body = json!({"jsonrpc": "1.0", "id": "difficulty-lab", "method": method, "params": params});
        let mut attempt = 1;
        let resp: Value = loop {
            // A retry uses a fresh agent in case the pool handed out a connection the node closed.
            let agent = if attempt == 1 { self.agent.clone() } else { ureq::agent() };
            match agent.post(&self.url).set("Authorization", &self.auth).send_json(body.clone()) {
                Ok(r) => break r.into_json()?,
                // The node answers HTTP 500 with the error in the JSON body.
                Err(ureq::Error::Status(_, r)) => break r.into_json()?,
                Err(ureq::Error::Transport(_)) if attempt < ATTEMPTS => {
                    std::thread::sleep(std::time::Duration::from_millis(250 << attempt));
                    attempt += 1;
                }
                Err(e) => return Err(e.into()),
            }
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
                n_tx: h["nTx"].as_u64().context("nTx")? as u32,
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
