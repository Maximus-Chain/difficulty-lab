# Miners leaving and coming back on purpose

Can a miner earn more by choosing when to mine Maximus? This is measured with `cargo run --release -- switching` (40 runs per cell, flat hashrate in equilibrium) for the current DGW v3 and every candidate. The same tables are in the published report.

The number that matters is **reward per hash**: blocks found per unit of work. 1.00 means no advantage, and 1.30 means 30% more reward than a miner who puts the same work in steadily. It measures the incentive to switch, which is what decides whether miners will do it.

## Summary

| | DGW v3 (today) | LWMA-1 N=60 | ASERT τ=36 | ASERT τ=72 |
|---|---|---|---|---|
| Profit switchers' edge over steady miners | **1.28–1.30** | 1.05–1.10 | 1.06–1.08 | 1.01–1.05 |
| 99% pool away 6 h, its reward per hash on return | 0.99 | 1.01 | **1.14** | **1.23** |

- **Today's weakness is profit switching.** Under DGW, miners who join when difficulty is low and leave when it rises earn ~30% more per hash than steady miners. The steady miners pay for it with the long gaps. Fixing DGW's bias changes nothing. Every candidate cuts the edge to a fifth or less.
- **ASERT's weakness is the catch-up burst.** A large pool that leaves for hours and returns collects it. That only pays a pool that mines something else while away. A merge-mining pool loses more than it gains.

## 1. Profit switching driven by difficulty

Switching miners add a multiple of the steady hashrate while difficulty is below a threshold of what the steady hashrate justifies, and leave above another threshold. The cells show the switching miners' reward per hash relative to the steady miners, and their share of blocks, over 3 days.

| | 3x, join < 1.2, leave > 1.6 | 3x, join < 1.0, leave > 1.2 | 10x, join < 1.0, leave > 1.2 |
|---|---|---|---|
| DGW v3 | **1.29** / 34% | **1.28** / 19% | **1.30** / 21% |
| DGW corrected | 1.28 / 36% | 1.27 / 22% | 1.29 / 23% |
| LWMA-1 N=24 | 1.15 / 32% | 1.21 / 19% | 1.22 / 20% |
| LWMA-1 N=60 | 1.05 / 29% | 1.09 / 14% | 1.10 / 15% |
| LWMA-1 N=90 | 1.04 / 28% | 1.08 / 13% | 1.08 / 13% |
| ASERT τ=24 | 1.08 / 31% | 1.12 / 16% | 1.13 / 17% |
| ASERT τ=36 | 1.06 / 30% | 1.08 / 14% | 1.08 / 15% |
| ASERT τ=72 | 1.01 / 29% | 1.04 / 12% | 1.05 / 12% |
| WTEMA M=36 | 1.06 / 29% | 1.08 / 13% | 1.08 / 13% |

DGW's 24-block window, with its double weight on the latest target, reacts only after the switching miners have taken the cheap blocks. Rules with more memory react to the switch instead of to its last few blocks, so the switchers' gain shrinks. A slower rule cuts it further. That is the stability versus response trade-off seen from the miner's side: τ=72 nearly removes the edge but recovers from a collapse about twice as slowly as τ=36.

## 2. A large pool leaves and comes back

A pool with 80% or 99% of the hashrate stops mining Maximus for 2, 6 or 24 hours and then returns. The cells show reward per hash relative to mining steadily, measured from the exit to 24 h after the return, as pool / miners who stayed:

| | 80% off 2 h | 80% off 6 h | 99% off 2 h | 99% off 6 h | 99% off 24 h |
|---|---|---|---|---|---|
| DGW v3 | 0.96 / 1.17 | 0.96 / 1.65 | 0.98 / 1.10 | 0.99 / 1.81 | 0.99 / 31.5 |
| DGW corrected | 1.00 / 1.22 | 1.01 / 1.71 | 1.02 / 1.17 | 1.03 / 1.65 | 1.04 / 31.7 |
| LWMA-1 N=60 | 1.02 / 1.10 | 1.03 / 1.58 | 1.00 / 1.07 | 1.01 / 1.12 | 1.06 / 5.7 |
| ASERT τ=24 | 1.03 / 1.20 | 1.03 / 1.70 | 1.08 / 1.33 | 1.10 / 13.1 | 1.09 / 46.0 |
| ASERT τ=36 | 1.04 / 1.15 | 1.05 / 1.64 | 1.08 / 1.21 | **1.14** / 9.3 | 1.14 / 43.7 |
| ASERT τ=72 | 1.05 / 1.10 | 1.09 / 1.50 | 1.08 / 1.17 | **1.23** / 2.9 | 1.28 / 36.8 |
| WTEMA M=36 | 1.03 / 1.13 | 1.03 / 1.61 | 1.03 / 1.10 | 1.11 / 4.4 | 1.12 / 38.6 |

DGW and LWMA forget the lost time, so a returning pool gets no bonus. ASERT keeps the chain on its schedule. After a long outage it has to win back the missing blocks, about τ·ln(1/r) of them for a remaining fraction r, and the returning pool collects most of that burst. A larger τ makes the burst, and the bonus, larger. The miners who stayed are the big winners under every rule, because difficulty falls while they are alone. Under ASERT it falls with elapsed time, so they get far more blocks. Their share is small in absolute terms.

Whether leaving pays depends on what the pool does while it is away:

- **A profit switcher** that mines another coin meanwhile gets the full bonus. ASERT gives it a reason to leave for hours and come back, and a larger τ makes that reason stronger.
- **A merge-mining pool** gets nothing elsewhere for the time it is away, since its main chain pays the same either way. Over the 30-hour window, a pool away for 6 h with τ=36 earns about 1.14 × 24/30 ≈ 0.91 of what staying would have paid. Leaving costs it about 9%.

The burst lands on everyone as roughly 200 blocks in the first hour after a 99% outage, against 50 at target pace, which means more orphans. The options are:

- Prefer τ=24 or τ=36 over τ=72, which gives a smaller burst and a faster return.
- Bound how far behind schedule ASERT can fall, at some cost in emission accuracy.
- Accept the burst, since total emission still follows the schedule.

## Takeaways

1. The incentive that exists on mainnet today is profit switching, at ~1.3x reward per hash. Fixing DGW's bias does not touch it; any of the candidates cuts it to ~1.05–1.10.
2. ASERT τ=36 keeps the best balance: a low switching edge (~1.08), the fastest recovery from a collapse, and a burst bonus that only pays a pool with somewhere else to mine.
3. τ=72 minimizes the switching edge but doubles both the recovery time and the burst bonus. It is the wrong trade for a chain whose real risk is a merge-mining pool disconnecting.
