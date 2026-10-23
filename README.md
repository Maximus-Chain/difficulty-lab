# difficulty-lab

Difficulty-algorithm simulator for Maximus mainnet: compares the current Dark Gravity Wave (DGW v3) with a corrected DGW, LWMA-1, ASERT (aserti3-2d) and WTEMA at several window sizes.

> **Research only.** No change to the difficulty algorithm or to emission has been decided. Any such change would be a hard fork, announced well in advance with an activation height and an upgrade window for pools, exchanges and node operators.

## Usage

```sh
# 1. Export height, timestamp and nBits of every block (needs a synced node with RPC on 127.0.0.1:9938)
cargo run --release -- extract --user <rpcuser> --password <rpcpassword>

# 2. Check that our DGW port reproduces every block's nBits on the real chain
cargo run --release -- validate

# 3. Optional: chain anomaly scan (long gaps, timestamps, clamp hits, hashrate collapses)
cargo run --release -- anomalies

# 4. Optional: superblock audit (budget paid, block time, proposal collaterals per cycle).
#    Scans every block with transactions once (~15 min) and caches it in data/proposal_fees.csv.
cargo run --release -- superblocks --user <rpcuser> --password <rpcpassword>

# 5. Optional: miners switching on purpose (profit switching, a pool that leaves and returns)
cargo run --release -- switching

# 6. Run all scenarios and write data/report.json + data/report.html
#    (embeds data/superblocks.json when present)
cargo run --release -- simulate
```

The published report lives in `docs/index.html` (served by GitHub Pages). Refresh it with `cargo run --release -- simulate --html docs/index.html`.

`simulate --help` lists the knobs: runs per scenario, hashrate window, propagation latency, pulse size, profit-switching thresholds and the size and length of the hashrate collapse.

## How it works

- `src/algo.rs`: the difficulty rules behind one trait. `Dgw` is a literal port of `DarkGravityWave()` from `src/pow.cpp` and matches all mainnet blocks bit for bit; its flags fix the off-by-one timespan and the biased target average (`Dgw::CORRECTED` fixes both). `Lwma` follows zawy12's LWMA-1 reference. `Asert` ports Bitcoin Cash's integer aserti3-2d, anchored at the last block before the simulated switch. `Wtema` is the relative, per-block form of the same exponential filter.
- `src/hashrate.rs`: estimates the network hashrate from the real chain (centered window of N blocks) and builds hashrate curves over time.
- `src/sim.rs`: Monte Carlo block production. Each block's work is exponentially distributed and is solved against the hashrate in force, with a fixed latency after each block for propagation and template switching.
- `src/report.rs`: four scenarios (replay of the real hashrate, a 4-hour hashrate pulse, profit-switching miners, sudden hashrate collapses of 50%, 80% and 99%) run for every algorithm in parallel. The 99% collapse is compared with the real one of October 2025. The report also embeds the `switching` results and a sweep of ASERT's τ from 20 to 40 through the same tests.
- `src/anomalies.rs`: anomaly scan of the real chain.
- `src/switching.rs`: miners who switch on purpose, next to steady miners, for every algorithm: difficulty-driven profit switching and a large pool that leaves and comes back. It measures reward per hash, the incentive to switch. Findings in `research/miner-switching.md` and in the report.
- `src/superblocks.rs`: superblock audit from coinbase outputs and on-chain proposal collaterals (triggers and votes are P2P-only and cannot be audited after the fact).

## Research

`research/alternative-difficulty-algorithms.md` surveys other difficulty algorithms (EMA/ASERT, Digishield, CryptoNote, KGW, real-time targeting, PID, emergency rules) and the literature, with formulas and the timestamp-rule changes any switch would need. Source notes are in `research/notes/`.

## Calibration

Defaults (latency 5 s, 240-block hashrate window) are calibrated so that the DGW replay matches the real chain's mean block time, tail, bursts and difficulty jitter.

The replay assumes miners would behave the same under another algorithm, so it compares rules like for like. It does not forecast the future.

## License

MIT, see [LICENSE](LICENSE).
