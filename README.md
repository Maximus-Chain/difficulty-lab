# difficulty-lab

Difficulty-algorithm simulator for Maximus mainnet: compares the current Dark Gravity Wave (DGW v3) with LWMA-1 at several window sizes.

> **Research only.** No change to the difficulty algorithm or to emission has been decided. Any such change would be a hard fork, announced well in advance with an activation height and an upgrade window for pools, exchanges and node operators.

## Usage

```sh
# 1. Export height, timestamp and nBits of every block (needs a synced node with RPC on 127.0.0.1:9938)
cargo run --release -- extract --user <rpcuser> --password <rpcpassword>

# 2. Check that our DGW port reproduces every block's nBits on the real chain
cargo run --release -- validate

# 3. Run all scenarios and write data/report.json + data/report.html
cargo run --release -- simulate
```

`simulate --help` lists the knobs: runs per scenario, hashrate window, propagation latency, pulse size and profit-switching thresholds.

## How it works

- `src/algo.rs`: the difficulty rules behind one trait. `Dgw` is a literal port of `DarkGravityWave()` from `src/pow.cpp` and matches all mainnet blocks bit for bit. `Lwma` follows zawy12's LWMA-1 reference.
- `src/hashrate.rs`: estimates the network hashrate from the real chain (centered window of N blocks) and builds hashrate curves over time.
- `src/sim.rs`: Monte Carlo block production. Each block's work is exponentially distributed and is solved against the hashrate in force, with a fixed latency after each block for propagation and template switching.
- `src/report.rs`: three scenarios (replay of the real hashrate, a 4-hour hashrate pulse, profit-switching miners) run for every algorithm in parallel.

## Calibration

Defaults (latency 5 s, 240-block hashrate window) are calibrated so that the DGW replay matches the real chain's mean block time, tail, bursts and difficulty jitter.

The replay assumes miners would behave the same under another algorithm, so it compares rules like for like. It does not forecast the future.

## License

MIT, see [LICENSE](LICENSE).
