# Classic (non-EMA/ASERT) PoW difficulty algorithms: Digishield, CryptoNote, KGW/DGW, Grin DMA, Ethereum, Kaspa, merge-mined chains

Context for the reader: the target chain is a small AuxPoW X11 Dash fork, T = 72 s, running DGW v3 (N = 24, clamp 1/3..3x). It sees sudden 80-99% hashrate drops when a big merge-mining pool disconnects, plus some profit switching. Research date: 2026-10-23. A "(unverified)" tag marks items taken from background knowledge that I could not confirm in a fetched source this session.

---

## 1. Digishield v3 (DigiByte origin, Dogecoin and Zcash variants): formula, parameters, behaviour

### Takeaway
Digishield is an SMA of targets with a 4x "tempered" timespan, i.e. nextTarget = avgTarget * (T_w + (actual - T_w)/4) / T_w, with asymmetric clamps. Zcash's version uses N = 17 and median-time-past (MTP) endpoints. MTP makes it immune to the classic timewarp, but it also delays the response by about 6 blocks. Dogecoin's version uses N = 1 (every block, the previous block only) with a /8 damping and clamps of 0.75..1.5. Digishield is very stable, but its effective window is about 4N, so it reacts slowly to an 80-99% hashrate drop.

### Cited Findings
- **Zcash code (current master)**: the code sums the `nBits` targets of the last `nPowAveragingWindow` blocks and computes `bnAvg = bnTot / nPowAveragingWindow`. It then takes `nActualTimespan = MTP(pindexLast) - MTP(pindexFirst)` and `nActualTimespan = averagingWindowTimespan + (nActualTimespan - averagingWindowTimespan)/4`, clamps to `[MinActualTimespan, MaxActualTimespan]`, and sets `bnNew = bnAvg / averagingWindowTimespan * nActualTimespan`, capped at powLimit. A code comment says: "Use medians to prevent time-warp attacks". Testnet allows a min-difficulty block if the block time is more than 6x the spacing after the previous block. — [zcash/src/pow.cpp](https://raw.githubusercontent.com/zcash/zcash/master/src/pow.cpp)
- In zawy's words, Zcash Digishield is `nextTarget = avg(17 prior Targets) * (0.75*T + 0.25*avg(17 prior STs)) / T`. A 17-block window with dilution factor 4 gives an effective N of about 68. Solvetimes are shifted back about 6 blocks by MTP, and it takes about 500 blocks to reach the correct difficulty after genesis. — [zawy12 issue #46](https://github.com/zawy12/difficulty-algorithms/issues/46)
- Digishield's asymmetric limits are 16% (difficulty up) and 32% (difficulty down). In a 51% attack they let the attacker get about 50% more blocks than the normal 100%. Because of the timespan limits, startup takes about 500 blocks instead of about 80. Coins that use MTP as the newest timestamp (e.g. Digishield) are not vulnerable to the classic timespan-limit timewarp. — [zawy12 issue #30 "Timestamp Attacks"](https://github.com/zawy12/difficulty-algorithms/issues/30)
- Zcash timestamp policy (Feb 2020 patch): the FTL is 90 minutes relative to MTP, and the peer-time adjustment is limited to ±25 minutes (25 + 90 < 120). This followed a Dec 2019 issue (Davidson) where peer time plus a future timestamp caused a database freeze. — [zawy12 issue #30](https://github.com/zawy12/difficulty-algorithms/issues/30)
- **Dogecoin implementation**: when `fDigishieldDifficultyCalculation` is set, the actual timespan is damped toward the target by dividing the deviation by 8 (an "amplitude filter - thanks to daft27"). The result is clamped to [3/4, 3/2] of `nPowTargetTimespan`, and the new target is `prevTarget * modulatedTimespan / nPowTargetTimespan`. Pre-Digishield clamps were: 1/4..4x above height 10000, 1/8 min above 5000, 1/16 min below that. From height 157500, a block may use min difficulty if its time is more than 2 x nPowTargetSpacing after the previous block. That rule is only enabled where `fPowAllowMinDifficultyBlocks` is set (testnet). — [dogecoin/src/dogecoin.cpp](https://raw.githubusercontent.com/dogecoin/dogecoin/master/src/dogecoin.cpp)
- **Dogecoin mainnet heights**: at height 0, nPowTargetTimespan = 4 h, spacing 60 s, legacy BTC-style retarget. At **145000**, `digishieldConsensus` sets nPowTargetTimespan = 60 (so it retargets every block from the previous block only) and `fDigishieldDifficultyCalculation = true`. At **371337**, `auxpowConsensus` sets `fAllowLegacyBlocks = false` (AuxPoW era). The AuxPoW chain ID is 0x0062 and `fStrictChainId = true`. — [dogecoin/src/chainparams.cpp](https://raw.githubusercontent.com/dogecoin/dogecoin/master/src/chainparams.cpp)
- Credit for Digishield in the Dogecoin code: "DigiShield implementation - thanks to RealSolid & WDC for this code". — [dogecoin.cpp](https://raw.githubusercontent.com/dogecoin/dogecoin/master/src/dogecoin.cpp)
- zawy judged Digishield "definitely better" than ETH Homestead or a simple EMA of equivalent stability (N = 35), though the chart differences are small. — [zawy12 issue #46](https://github.com/zawy12/difficulty-algorithms/issues/46)

### Inferences
- Dogecoin's Digishield reduces to `newTarget = prevTarget * (1 + (st/T - 1)/8)`, clamped to [0.75, 1.5]. This is effectively a 1-block EMA with N ≈ 8, so it is far more responsive than Zcash's 17-block version. Dogecoin is the canonical merge-mined example: it has used this per-block filter since 2014, plus Litecoin AuxPoW.
- After a 90% hashrate drop, Dogecoin-style Digishield can lower difficulty by at most 1.5x per block, but only when the solvetime is at least about 5T. Each slow block lowers difficulty by about (st/T - 1)/8. Recovery is therefore exponential with time constant about 8 blocks, much faster than Zcash-style N = 17 with the 6-block MTP delay.
- Zcash's MTP timestamps are what remove the timewarp vulnerability, but they also add about 6 blocks of lag. The lag causes oscillation under on/off profit switching. For an AuxPoW chain hit by abrupt pool departures, that lag is the main cost.

### Gaps
- The exact Zcash `nPowMaxAdjustDown/Up` values (32/16) are cited only via zawy, not from chainparams. The DigiByte original Digishield v3 / MultiShield code was not fetched.

---

## 2. CryptoNote / Monero DAA (sort-cut-lag SMA)

### Takeaway
Monero takes a 720-block window lagged by 15 blocks, sorts the timestamps, cuts 60 from each end, and computes difficulty = Σwork(window) * T / timespan(cut window). The result is very stable but effectively lags by about 360+15 blocks. CryptoNote clones with low hashrate got stuck for days after hashrate left. That is the main reason zawy's LWMA spread through small CryptoNote coins in 2018.

### Cited Findings
- Constants: `DIFFICULTY_TARGET_V2 120`, `DIFFICULTY_TARGET_V1 60`, `DIFFICULTY_WINDOW 720`, `DIFFICULTY_LAG 15 // !!!`, `DIFFICULTY_CUT 60 // timestamps to cut after sorting`, `DIFFICULTY_BLOCKS_COUNT = WINDOW + LAG`, `CRYPTONOTE_BLOCK_FUTURE_TIME_LIMIT 60*60*2`, `BLOCKCHAIN_TIMESTAMP_CHECK_WINDOW 60`. — [monero/src/cryptonote_config.h](https://raw.githubusercontent.com/monero-project/monero/master/src/cryptonote_config.h)
- Algorithm (`next_difficulty`):
  1. Truncate the vectors to `DIFFICULTY_WINDOW`.
  2. Sort the timestamps only. Cumulative difficulties stay in chronological order.
  3. If length > WINDOW - 2*CUT, keep a centered sub-window of WINDOW - 2*CUT entries.
  4. Set time_span = last - first sorted timestamp, with 0 replaced by 1.
  5. Set total_work = the cumulative-difficulty difference across the cut window.
  6. next = ceil(total_work * target / time_span).

  The 64-bit variant returns 0 on overflow; the comment notes that this causes a "difficulty overhead" error. — [monero/src/cryptonote_basic/difficulty.cpp](https://raw.githubusercontent.com/monero-project/monero/master/src/cryptonote_basic/difficulty.cpp)
- CryptoNote uses only the node clock, with no peer median time (zawy cites this as a good design). It has MTP = 60 and usually no timespan limit. The public-mining version of the timestamp attack is "far easier with MTP=60 (Cryptonote defaults)" than with MTP = 11. — [zawy12 issue #30](https://github.com/zawy12/difficulty-algorithms/issues/30)
- LWMA coins attacked around April 2018: Niobio, Intense, Karbo, Sumo. — [zawy12 issue #30](https://github.com/zawy12/difficulty-algorithms/issues/30)
- Wownero (a Monero fork) replaced the CryptoNote DAA with LWMA in v8 (block 6969), LWMA v2 at 53666, and LWMA v4 at 63469. The LWMA header recommends reducing the FTL from 2 h to 500 s. — [Wownero commit (gitdab mirror)](https://gitdab.com/d0s3t3ch/wownero/commit/2c959a9b211486a549eb61a074c4c2edbf155b61)

### Inferences
- The centered cut window means the effective "age" of the difficulty estimate is about 15 + 360 blocks. With T = 72 s that would be about 7.5 hours of lag, which is unusable for a chain that loses 90% of its hashrate.
- Sorting the timestamps neutralizes out-of-order timestamps, but the large window plus a 2 h FTL lets a miner push difficulty down by a small fraction.

### Gaps
- I found no primary post-mortem quantifying a specific "stuck CryptoNote coin" incident: the search returned only Wownero commits. Monero's own stance (keeping the original DAA, then the RandomX era) was not researched.

---

## 3. Kimoto Gravity Well (KGW) and Dash's Dark Gravity Wave history and weaknesses

### Takeaway
KGW sizes its window dynamically and stops early at an "event horizon". It had a timewarp exploit, which Evan Duffield's DGW was created to fix (DGW v2 at block 45,000 and v3 on 2014-05-14, per a single wiki source). DGW v3 is a 24-block SMA of targets. Its averaging loop double-weights the newest target, it has an off-by-one timespan (24 intervals measured over 23), and its 1/3..3x clamps are exploitable with non-sequential timestamps. zawy estimates the combined effect makes solvetime about 6% longer than intended.

### Cited Findings
- **Dash `pow.cpp` dispatch**: height < `nPowKGWHeight` uses the BTC-style rule; height < `nPowDGWHeight` uses `KimotoGravityWell`; otherwise `DarkGravityWave`. With `fPowAllowMinDifficultyBlocks`, a block more than 2 h after the previous one returns powLimit, and one more than 4 spacings after gets previous nBits x 10. — [dash/src/pow.cpp](https://raw.githubusercontent.com/dashpay/dash/master/src/pow.cpp)
- **KGW** in Dash code: it averages the difficulty of a variable window of past blocks and scales by actual/target time. The window is bounded by `pastSecondsMin` = 2.5% of the target timespan and `pastSecondsMax` = 7x the target timespan. It iterates backward and stops early when the rate ratio falls outside an "event horizon" deviation. — [dash/src/pow.cpp](https://raw.githubusercontent.com/dashpay/dash/master/src/pow.cpp)
- **DGW v3** in Dash code ("DarkGravity v3, written by Evan Duffield"): `nPastBlocks = 24`, and it returns powLimit if the chain is shorter. It "averages" the 24 targets, with the in-code comment "NOTE: that's not an average really...". It clamps the actual timespan to [1/3, 3] x the target timespan and caps at powLimit. — [dash/src/pow.cpp](https://raw.githubusercontent.com/dashpay/dash/master/src/pow.cpp)
- **zawy12 issue #31 "DGW Problems"** (opened 2018-09-04):
  1. The averaging loop is "just an error". It is effectively an SMA that double-weights the most recent target.
  2. Off-by-one: DGW uses one fewer timestamp than it should, so with N = 24 the average solvetime is about 1/24 longer than intended.
  3. The small window adds about 1.5% more solvetime because of Erlang skew. Items 2 and 3 together make solvetime about 6% longer than intended.
  4. The 3x and 1/3 clamps let a selfish miner make unlimited blocks within about 2 windows when timestamps are not forced sequential.

  zawy's fix: `next_target = avg(N past targets) * (ts[1] - ts[N+1]) / (N*T)`. — [zawy12 issue #31](https://github.com/zawy12/difficulty-algorithms/issues/31)
- **Timespan-limit timewarp** (applies to DGW's first/last timestamp + symmetric clamps): it usually needs 51% with block withholding. Difficulty drops to about 1/100,000 in 1-2 hours, yielding about 5,000 blocks, and unlimited blocks are possible within 3·N·T. Limits cited: BTC 4x/¼, Dash 3x, BCH 2x. KGW has a one-sided limit that prevents rises but not falls, so it "may be more exposed". — [zawy12 issue #30](https://github.com/zawy12/difficulty-algorithms/issues/30)
- **Verge (XVG) April/May 2018**: a multi-algo chain (each algo with its own difficulty, using DGW-style per-algo retargets). Attackers forged timestamps and difficulty fell from about 117,320 to about 0.0015, letting them mine about 1 block/s. April took about 20M-55M XVG (estimates vary); in May the attacker alternated two algos. The fix limited block drift to 10 min from block 2,040,000. — [SecurityAffairs](https://securityaffairs.com/71203/hacking/verge-hacked.html), [CryptoSlate](https://cryptoslate.com/verge-victim-to-yet-another-51-attack-xvg-down-15-in-past-24-hours/)
- zawy attributes Verge to: a too-long FTL, 1/3 and 3x timespan limits, selecting the longest chain by block count instead of chain work, and the attacker holding a large share of one algo's hashrate. — [zawy12 issue #30](https://github.com/zawy12/difficulty-algorithms/issues/30)
- **Dash history**: DGW was created by Evan Duffield "to address a time-warp exploit in KGW" and uses "multiple exponential moving averages and a simple moving average". DGW 2.0 was introduced at block 45,000, and DGW 3.0 on 2014-05-14. This is a single wiki source mirrored in other languages. — [IQ.wiki Dash](https://iq.wiki/wiki/dash), [Mastering Blockchain (O'Reilly)](https://www.oreilly.com/library/view/mastering-blockchain/9781788839044/b595a6c8-e9e1-4aee-857d-2a5abbbd90b2.xhtml), [Bitcoinist 2014](https://bitcoinist.com/darkcoin-and-darksend-why-you-should-care/amp/)
- Veil (2020) found DGW unable to adjust ideally in a multi-algo setting when large gaps in per-algo block spacing occur. — [Veil devnet update](https://veil-project.com/blog/devnet-update)

### Inferences
- For the target chain (T = 72, N = 24), the DGW window spans about 29 minutes nominally. After a 90% drop, the next block takes about 10x72 s = 12 min. Each slow block only shifts the 24-block SMA by about 1/24 of its excess, and the 3x clamp caps the cut to 1/3 per evaluation. Full recovery therefore needs roughly 24 blocks at progressively shorter but still slow times, i.e. hours. This matches the reported "stalls".
- The "multiple EMAs" description of DGW in secondary sources does not match the v3 code. v3 is an SMA with a quirky weighting loop. Primary code takes precedence.
- Fixing the off-by-one and double-weighting is a cheap consensus change, but it does not address the response time to 80-99% drops. That needs a shorter effective window or an EMA/LWMA, which other researchers cover.

### Gaps
- No primary source (Dash release notes or forum) was found for the exact KGW→DGW activation height and date on Dash mainnet. I did not confirm the specific KGW timewarp exploit write-up (2014 BitcoinTalk) or a precise mechanism. Common accounts: KGW's dynamic window plus its one-sided limit let timestamps game the "event horizon". That is unverified here.

---

## 4. Grin damped moving average (DMA) and its replacement with wtEMA

### Takeaway
Grin launched with a DMA: a 60-block window (one hour at 60 s), damping factor 3 toward the goal timespan, clamp factor 2, and a minimum difficulty. At HF4 (header version 5) it replaced the DMA with wtEMA.

### Cited Findings
- `DMA_WINDOW = HOUR_HEIGHT` (60 blocks at 60 s), `BLOCK_TIME_WINDOW = DMA_WINDOW * BLOCK_TIME_SEC`, `CLAMP_FACTOR = 2`, `DMA_DAMP_FACTOR = 3`, `MIN_DMA_DIFFICULTY = DMA_DAMP_FACTOR` ("avoids getting stuck when trying to increase difficulty subject to dampening"). — [grin/core/src/consensus.rs](https://raw.githubusercontent.com/mimblewimble/grin/master/core/src/consensus.rs)
- `damp(actual, goal, f) = (actual + (f-1)*goal) / f` and `clamp(actual, goal, f) = max(goal/f, min(actual, goal*f))`. `next_difficulty` uses the DMA if header_version < 5, otherwise `next_wtema_difficulty` ("starting from HF4, the new wtema DAA"). — [grin consensus.rs](https://raw.githubusercontent.com/mimblewimble/grin/master/core/src/consensus.rs)

### Inferences
- The DMA is `diff = Σdiff(60) * T / clamp(damp(Δts, 60T, 3), 60T, 2)`, i.e. Digishield-like with damping 3 instead of 4 and clamp 0.5..2.
- Grin abandoning the DMA for an EMA is a precedent for "SMA + damping" being judged inferior.

### Gaps
- The exact `next_dma_difficulty` and `next_wtema_difficulty` bodies and the WTEMA half-life constant were not quoted in the fetch. HF4 date (believed Jan 2021) is unverified.

---

## 5. Ethereum (pre-merge) per-block adjustment and the difficulty bomb

### Takeaway
Ethereum used a per-block, discretized EMA-like step: diff += diff/2048 * max(k - Δt//9, -99), with k = 2 if the parent has uncles, else 1, plus an exponential "bomb". zawy shows Homestead is indistinguishable from a simple EMA with N ≈ 2048/~ln2 scaling. It has timestamp-attack weaknesses: a block-withholding miner can get up to about 175% of blocks.

### Cited Findings
- **Byzantium (EIP-100)**: `adj_factor = max((2 if len(parent.uncles) else 1) - ((timestamp - parent.timestamp) // 9), -99)`. This replaces the Homestead line (`//10`). The fork condition is `block.number >= BYZANTIUM_FORK_BLKNUM`, and the EIP was created 2016-04-28. — [EIP-100](https://eips.ethereum.org/EIPS/eip-100)
- zawy's form: Homestead `D = priorD * (1 + 1/N*(1 - int(ST/T/ln2)))`; with T = 10 that becomes `D = priorD*(1 + 0.0005*(1 - int(ST/10)))`, a 1/2048 step. It is a discretized simple EMA. Homestead allows timestamp attacks, and zawy estimates a selfish-mining attacker could get about 7x more profit in one scenario. — [zawy12 issue #46](https://github.com/zawy12/difficulty-algorithms/issues/46)
- "ETH Homestead: gives a >50% block-withholding miner more than scheduled blocks; some reports up to 175%." — [zawy12 issue #30](https://github.com/zawy12/difficulty-algorithms/issues/30)

### Inferences
- The -99 floor caps a single-block decrease at 99/2048 ≈ 4.8%. After a 90% drop, recovery takes about 50+ blocks of slow solves. Like Dogecoin's /8, this is fast compared to DGW only because the step applies every block.
- The uncle term is specific to GHOST/uncle chains (RSK inherits it). It is irrelevant for an X11 AuxPoW chain.

### Gaps
- `BLOCK_DIFF_FACTOR = 2048` is confirmed only via zawy, not on the EIP page. The bomb term `2**((fake_block_number // 100000) - 2)` and its delays (EIP-649 3M, EIP-1234 5M, EIP-2384 9M, EIP-3554, EIP-4345, EIP-5133) are from background knowledge (unverified this session). RSK's exact DAA was not fetched.

---

## 6. Kaspa DAA (window-based, sparse windows)

### Takeaway
Kaspa's DAA is a large-window average over blue blocks plus merged red blocks ("DAA score"). The legacy window is 2641 blocks. KIP-4 (active) makes the windows *sparse*: the difficulty window keeps 1000 sampled blocks over 30000 s, and the timestamp window keeps 121 blocks over 1210 s, so the 10-BPS upgrade stays computationally cheap.

### Cited Findings
- The difficulty window is 2641 blocks: the blue blocks plus the red blocks that fit, counting back from the virtual block. DAA score = blue score + the number of red blocks merged and rewarded so far, and it controls emission. — [Kaspa developer wiki](https://wiki.kaspa.org/developers-knowledge-base)
- KIP-3 (block sampling) was rejected. KIP-4 (Sparse Difficulty Windows; Wyborski, Sutton, Künzli) is active. — [kaspanet/kips](https://github.com/kaspanet/kips)
- KIP-4: the legacy windows are about 263 blocks (timestamp flexibility) and 2641 blocks (DAA). The proposal is a timestamp window of length 1210 s / size 121 and a difficulty window of length 30000 s (500 min) / size 1000. `sparsity = length*BPS/size`. A block enters the window when its DAA-score position is a multiple of the sparsity. Blocks older than length x BPS in blue score are non-DAA. — [KIP-4](https://raw.githubusercontent.com/kaspanet/kips/master/kip-0004.md)

### Inferences
- Kaspa's DAA is tuned for a high-BPS DAG with stable hashrate. Its 500-minute window is far too slow for a small AuxPoW chain with 90% drops. Its one transferable lesson is window *sampling*, which keeps a long window cheap.

### Gaps
- KIP-4 does not state the target formula or the timestamp aggregation. I did not fetch the rusty-kaspa `difficulty.rs`. Commonly described (unverified): average target x (max_ts - min_ts) / expected, with a minimum-timestamp approach. Nothing DAGKnight-specific about the DAA was found.

---

## 7. Merge-mined chains' DAAs and lessons from hashrate drops

### Takeaway
Dogecoin is the clearest precedent. Since height 145000 it has used a per-block Digishield (prev block, /8 damping, 0.75..1.5 clamp), well before AuxPoW at 371337, and it survived Litecoin merge-mining hashrate swings. Most other merge-mined chains (Namecoin, Syscoin) inherited Bitcoin's 2016-block retarget, which is untenable for a small chain. Multi-algo chains (Myriad, Verge, DigiByte) use per-algo per-block retargets, and Verge shows that clamped timespan DAAs get exploited.

### Cited Findings
- Dogecoin: Digishield at height 145000 with a 60 s timespan (per-block), and AuxPoW at 371337 with `fAllowLegacyBlocks = false` and chain ID 0x0062. — [dogecoin chainparams.cpp](https://raw.githubusercontent.com/dogecoin/dogecoin/master/src/chainparams.cpp), [dogecoin.cpp](https://raw.githubusercontent.com/dogecoin/dogecoin/master/src/dogecoin.cpp)
- Syscoin: the legacy 1.x scrypt chain used "fast-response KGW". A chart site claims the current chain retargets every 2016 blocks at a 60 s target; I could not confirm that against primary code. The AuxPoW target is separate from Bitcoin's. — [old Syscoin repo](https://github.com/spencerlievens/syscoin), [CoinWarz](https://www.coinwarz.com/mining/syscoin/difficulty-chart), [Syscoin merged-mining guide](https://syscoin.github.io/sys-docs/docs/guides/mining_setup)
- Myriad: 5 algos, each with independent difficulty and the same adjustment method. Difficulty adjusts every block, with a 5-min per-algo target (about 60 s overall). Source is an unofficial wiki. — [r/myriadcoin wiki mirror](https://r.datuan.dev/r/myriadcoin/wiki/index)
- Multi-algo DGW (Veil) cannot adjust well when large per-algo gaps occur. — [Veil devnet update](https://veil-project.com/blog/devnet-update)
- Komodo deployed RTT to handle 1,000,000x hashrate swings on sub-chains. — [zawy12 issue #30](https://github.com/zawy12/difficulty-algorithms/issues/30)

### Inferences
- The lesson for an AuxPoW chain that loses a big pool is that per-block response with small damping matters most. Dogecoin's /8 per-block filter or an EMA/LWMA beats a 24-block SMA with a 3x clamp. Real-time targeting (RTT, Komodo) is an option for extreme swings, but it adds timestamp-trust risks.
- Merge-mined pools tend to leave abruptly, as a step function. That favors DAAs with short effective memory on the "down" side; asymmetric designs are worth simulating.

### Gaps
- Unverified or not found: Namecoin's DAA (believed to be unchanged Bitcoin 2016-block retarget); Elastos' DAA (BTC-merged; believed Bitcoin-like); RSK's DAA (ETH-like with uncles, believed tuned in 2019-2020 per the Rootstock blog, [Rootstock blog](https://rootstock.io/blog/rootstock-blocks-getting-faster-redefining-difficulty-adjustment/), not read); Unobtanium's DAA; Myriad's exact MultiShield formula. I found no documented post-mortem of a merge-mined chain stalling after a pool left.
