# ASERT with a long-delay brake beats DGW

The best alternative to DGW v3 and LWMA-1 for Maximus is **ASERT (Bitcoin Cash's aserti3-2d) with a short half-life of about 30 minutes (mean lifetime M ≈ 36 blocks)**. If simulations justify it, a TSA-style "long-delay" layer can be added that eases only the block being mined once the wait exceeds ~12 minutes. All serious families (EMA/WTEMA/ASERT, LWMA, WT) perform almost the same when compared at equal stability. The clear losers are SMAs with clamps (DGW, cw-144, MTP-based Digishield, CryptoNote) and threshold emergency rules (BCH EDA, the testnet 20-minute rule). The academic literature, small and focused on BTC/BCH, agrees: SMA windows create positive feedback and resonance, and exponential filters remove it. ASERT fits Maximus's specific problem, sudden disconnections of a merge-mining pool, better than LWMA. **ASERT lowers difficulty with wall-clock time and has no per-block cap**, while LWMA "rises ~50% faster and falls ~50% slower" at equal noise. That explains why LWMA-60 takes 20.6 h to recover from a −99% drop in our simulator. No algorithm that only looks at past blocks avoids the first wait of ~100·T (2 h) after losing 99% of the hashrate, so a real-time layer is the only lever against that first gap. None of this is safe while **MAX_FUTURE_BLOCK_TIME stays at 7200 s (100·T)**: the FTL, peer time adjustment and timestamp monotonicity must change together with the DAA, or before it.

## The alternatives fall into four families with very different profiles

The catalogue is large, but it reduces to four behaviours.

**SMAs with clamps or delay: DGW, KGW, cw-144, Digishield, CryptoNote, Kaspa.** Difficulty depends on a window of past timestamps. A slow block weighs the same when it enters the window as when it leaves, which produces the positive feedback Ilie et al. documented on BCH ([Unstable Throughput](https://ar5iv.arxiv.org/html/2006.03044)). Symmetric clamps (3x and 1/3 in DGW) also allow the unlimited-blocks timewarp when timestamps are not forced to be sequential ([zawy12 #31](https://github.com/zawy12/difficulty-algorithms/issues/31); [zawy12 #30](https://github.com/zawy12/difficulty-algorithms/issues/30)).

**Per-block exponential filters: EMA, WTEMA, ASERT, Ethereum, Dogecoin-Digishield.** They only use the last solvetime or the distance to an absolute schedule. A fake timestamp affects a single block.

**Real time: Harding's RTT, zawy's TSA, Komodo's AdaptivePoW.** The difficulty of the block being mined depends on its own timestamp. This is the only family that acts *during* the wait.

**Threshold emergency rules: BCH EDA and testnet min-difficulty rules.** Both failed in production.

| Algorithm | Effective memory | Sudden 80–99% drop | Stability and timestamp attacks | Verdict for Maximus |
|---|---|---|---|---|
| DGW v3 (N=24, clamps 1/3–3x) | 24-block SMA, double weight on the last target | 8.3 h at −99% (our sim); at most ÷3 per calculation | Off-by-one and Erlang bias: ~6% slow; exploitable clamps | Baseline to replace |
| KGW | Variable window with an "event horizon" | Fast, but the curve has no justification | One-sided limit; the timewarp exploit that led to DGW | Rejected |
| BCH cw-144 | Chainwork SMA-144, median of 3, clamps 0.5–2x | Slow | ~1-day oscillation; steady miners lose ~6–13% | Rejected |
| Digishield v3 (Zcash N=17, MTP) | ~4N ≈ 68 blocks + ~6 blocks of MTP delay | Slow; 16%/32% limits | Immune to the classic timewarp thanks to MTP, but oscillates | Rejected |
| Dogecoin Digishield (/8, clamp 0.75–1.5) | Per-block EMA with N≈8 | Up to ÷1.5 per block | High noise (~25%); AuxPoW precedent | Reference, not a candidate |
| CryptoNote/Monero | 720 sorted blocks, 60+60 cut, lag 15 | ~375 blocks of delay (≈7.5 h at T=72) | Very stable; small coins stuck for days | Rejected |
| Grin DMA → wtEMA | 60-block DMA, damp 3, clamp 2 | Medium | Grin dropped it for wtEMA | Precedent in favour of EMA |
| Ethereum (EIP-100) | Discretised EMA, 1/2048 step, −99 floor | At most ~4.8% per block | Up to ~175% of blocks for a block-withholding miner | Rejected |
| Kaspa (KIP-4) | 1000 samples over 30000 s | Very slow (500 min) | Designed for a DAG with stable hashrate | Only the sampling idea |
| EMA/WTEMA | First-order IIR filter, M blocks | Linear in st; no cap unless clipped | Singular at st ≤ −(M−1)T; rounding error ∝ M | Cross-check |
| **ASERT (aserti3-2d)** | Absolute schedule, τ = M·T | Exponential in wall-clock time, no cap | No accumulated error; tolerates negative st; sensitive to FTL/τ | **Main candidate** |
| RTT/TSA | Current block | Acts during the wait | Needs FTL ≲ T and monotonic timestamps; "future mining" | **Optional layer, long-delay version** |
| PID (Bismuth) | 1440 blocks, bandwidth ~3111 blocks | Very slow | D term exploitable by switching miners | Rejected |
| Dual-window / crossover | Switches between fast and slow N | Does not speed up the first block | False triggers, exploitable hysteresis | Rejected |
| BCH EDA / testnet 20 min | Threshold step | Fast but exploitable | Boom/bust cycles; 85–90% CPU blocks on testnet4 | Rejected |

### The details behind the table

DGW's problems are well documented. Its "average" loop gives double weight to the most recent target. It uses one timestamp fewer than it should, which stretches solvetime by 1/24, and the Erlang bias of a short window adds ~1.5%. In total that is **~6% extra solvetime** ([zawy12 #31](https://github.com/zawy12/difficulty-algorithms/issues/31)). Fullmer and Morse formally derive that an SMA-style retarget with window N has E[X] = β·N/(N−1) ([arXiv 1812.10792](https://arxiv.org/pdf/1812.10792)). For N=24 that predicts ~75.1 s. Our simulator measures 75.6 s, consistent with both sources.

The Zcash version of Digishield computes `avg(17 targets) * (0.75*T + 0.25*avg(17 ST)) / T` over MTP. It is very stable, but it has ~6 blocks of delay and asymmetric limits that stretch startup to ~500 blocks ([zawy12 #46](https://github.com/zawy12/difficulty-algorithms/issues/46); [zcash pow.cpp](https://raw.githubusercontent.com/zcash/zcash/master/src/pow.cpp)). Dogecoin's version is more interesting as a merge-mining precedent. Since height 145000 it retargets every block with `prevTarget * (1 + (st/T − 1)/8)`, bounded to [0.75, 1.5], and kept it when AuxPoW activated at 371337 ([dogecoin.cpp](https://raw.githubusercontent.com/dogecoin/dogecoin/master/src/dogecoin.cpp); [chainparams.cpp](https://raw.githubusercontent.com/dogecoin/dogecoin/master/src/chainparams.cpp)). In practice it is an EMA with N≈8, too noisy for Maximus.

CryptoNote/Monero uses 720 blocks with lag 15 and a cut of 60 at each end ([difficulty.cpp](https://raw.githubusercontent.com/monero-project/monero/master/src/cryptonote_basic/difficulty.cpp)). It is the classic counterexample: LWMA spread in 2018 because small coins on it got stuck after losing hashrate. Wownero replaced it with LWMA and lowered the FTL from 2 h to 500 s ([Wownero](https://gitdab.com/d0s3t3ch/wownero/commit/2c959a9b211486a549eb61a074c4c2edbf155b61)).

Grin launched with a DMA, a 60-block SMA with damping 3 and clamp 2, and replaced it with wtEMA at HF4 ([grin consensus.rs](https://raw.githubusercontent.com/mimblewimble/grin/master/core/src/consensus.rs)). Ethereum used `diff += diff/2048 * max(1 − Δt//9, −99)` ([EIP-100](https://eips.ethereum.org/EIPS/eip-100)), a discretised EMA that zawy ranks below EMA/ASERT/LWMA. Kaspa averages 1000 blocks sampled over 30000 s ([KIP-4](https://raw.githubusercontent.com/kaspanet/kips/master/kip-0004.md)), irrelevant for hashrate that changes within minutes.

The dual-window designs come from an author who ended up disowning them: the Dynamic EMA, the Dynamic LWMA and the Slipstream EMA, in which a fast EMA (~N=9) only runs within ±14% of a tempered SMA and a slow one (~N=28) outside that band. zawy dropped them because of false triggers (a 2x jump for 4 blocks roughly every 720 blocks) and because they amplified oscillations with on-off miners ([zawy12 #27](https://github.com/zawy12/difficulty-algorithms/issues/27); [zawy12 #25](https://github.com/zawy12/difficulty-algorithms/issues/25)). His verdict on LWMA-2/3/4 sums up the risk: they improved the ~70% of coins that did not need it and made the 30% that did worse ([zawy12 #50](https://github.com/zawy12/difficulty-algorithms/issues/50)).

PID controllers fare similarly. The only one deployed on a mainnet, on Bismuth, has a bandwidth of ~3111 blocks and a settling time of ~4000 blocks ([Hovland & Kučera 2017](https://www.mic-journal.no/PDF/2017/MIC-2017-4-1.pdf)). zawy's 2022 review concludes that the I term is redundant with the EMA filter and that the D term assumes an "inertia" miners do not have and can exploit ([zawy12 #20](https://github.com/zawy12/difficulty-algorithms/issues/20); [zawy12 #48](https://github.com/zawy12/difficulty-algorithms/issues/48)).

## The literature favours exponential filters but does not fix the parameter

The academic body is small and almost all of it is about BTC and BCH. No peer-reviewed work evaluates DGW or LWMA.

**Ilie, Werner, Stewart and Knottenbelt** show that cw-144 produces solvetimes autocorrelated every 24 h, with hour-long bursts and droughts. Their exponential filter NEFDA, `D_n = D_{n−1}·e^{(T−st_n)/S}`, "essentially equivalent" to ASERT, removes that autocorrelation: mean 599.97 s against 604.34 s ([arXiv 2006.03044](https://ar5iv.arxiv.org/html/2006.03044)). They also give the equivalence S ≈ (N+1)/2: an EMA with constant S matches the stability of an SMA-N. By that criterion, **DGW-24 is as noisy as ASERT with M ≈ 12**.

**Noda, Okumura and Hashimoto** model hashrate as an elastic supply and prove that stability depends on elasticity relative to the adjustment frequency: cw-144 is "stable if elasticity < 144" ([SSRN 3410460](https://papers.ssrn.com/sol3/papers.cfm?abstract_id=3410460); [CARF slides](https://www.carf.e.u-tokyo.ac.jp/wp/wp-content/uploads/2021/09/211001Noda.pdf)). With Kawaguchi they estimate elasticities of 5.4 for BCH and 0.6 for BTC, and conclude that ASERT "clearly outperforms" cw-144 ([BCH Research](https://bitcoincashresearch.org/t/empirical-analysis-of-the-aserts-performance/708)).

**Judmayer et al.** document that pools on merge-mined chains have operated for long periods at or beyond the limits of Nakamoto's guarantees ([ePrint 2017/791](https://eprint.iacr.org/2017/791)). An AuxPoW chain is the extreme case of elastic supply: a pool adds or drops the chain at almost no cost.

Direct quantitative comparisons between algorithms come from practitioners. In Toomim's simulation with 6x switching, at equal N, asert-288, wtema-288 and lwma-288 all land at 672–673 s of confirmation time. cw-144 reaches 1565 s, with −10.9% for steady miners ([zawy12 #62](https://github.com/zawy12/difficulty-algorithms/issues/62#issuecomment-646159947)). zawy's methodology compares at equal difficulty standard deviation: SMA-144 ≈ EMA/ASERT-72 ≈ LWMA-144 ≈ WT-190. Under those conditions only the SMA stands apart ([zawy12 #62](https://github.com/zawy12/difficulty-algorithms/issues/62)). The useful rule for translating parameters is **ASERT M ≈ LWMA N/2**, with difficulty standard deviation ≈ 1/√(2M) ([zawy12 #50](https://github.com/zawy12/difficulty-algorithms/issues/50)).

Where the literature disagrees is the parameter. Toomim found BCH's optimum at a mean lifetime of 400–500 blocks and chose a 2-day half-life. zawy prefers half of that and warns that coins with on-off miners "might be better off with half those values" ([Toomim](https://read.cash/@jtoomim/bch-upgrade-proposal-use-asert-as-the-new-daa-1d875696); [zawy12 #50](https://github.com/zawy12/difficulty-algorithms/issues/50)). Those optima were computed for profit switching between BTC and BCH at T=600, not for the permanent exit of a pool contributing 80–99% of the hashrate. No published simulation covers that scenario; our simulator has to decide it.

## Emergency rules failed and RTT is only useful in its minimal form

BCH EDA cut difficulty by 20% if the MTP advanced 12 h over 6 blocks ([UAHF spec](https://github.com/bitcoincashorg/bitcoincash.org/blob/master/spec/uahf-technical-spec.md)). Miners exploited it: BCH reached ~25 blocks per hour and built up a lead of thousands of blocks ([Bitcoin Magazine](https://bitcoinmagazine.com/articles/miners-are-milking-bcashs-difficulty-adjustments-and-why-problem1/); [BitDigest](https://bitdigest.substack.com/p/bitdigest-monday-august-21-2017-70368)). Bitcoin testnet's 20-minute rule caused "block storms" on testnet3. On testnet4 it was still being exploited in 2026, with 85–90% of blocks CPU-mined at difficulty 1 ([BIP 94](https://github.com/bitcoin/bips/blob/master/bip-0094.mediawiki); [bitcoin #34420](https://github.com/bitcoin/bitcoin/pull/34420)).

The abuse mechanism is always the same. The relief is triggered by a timestamp the miner chooses, and a wide FTL lets the miner trigger it at will. That is why **bringing the testnet rule inherited from Dash to mainnet** (prev + 4·T = 288 s ⇒ target ×10; prev + 2 h ⇒ powLimit) would be an immediate exploit with FTL = 7200 s.

RTT has two mature references. Harding sets the subtarget to `g(t) = G(n−1)·k·Γ(1+1/k)^k·t^(k−1)/T^(k−1)` and requires fork choice by the parent's chainwork, strictly increasing timestamps and a maximum time equal to "now" ([Harding, Ledger 2020](https://github.com/chain2/chain2/blob/master/specifications/rtt.pdf)). The other is zawy's TSA ([zawy12 #36](https://github.com/zawy12/difficulty-algorithms/issues/36)).

RTT has proven costs. Bissias shows a Nash equilibrium in which every miner does "future mining": mining against an easier future target and publishing when that time arrives. That causes block races and orphans ([arXiv 2007.05991](https://arxiv.org/pdf/2007.05991)). The only large deployment, eCash's Heartbeat (November 2024), **only raises the difficulty** of blocks that come too fast, and does so as a policy enforced through Avalanche using local receive times ([rtt.cpp](https://www.bitcoinabc.org/doc/dev/rtt_8cpp_source.html); [e.cash](https://e.cash/blog/heartbeat)). It does nothing for a stuck chain and cannot be reproduced without a post-consensus layer.

The defensible variant for Maximus is zawy's "long-delay-only" one. If the solvetime exceeds 6T + FTL, it applies `Diff *= 0.90^((st − 6T − FTL)/T)` ([zawy12 #36](https://github.com/zawy12/difficulty-algorithms/issues/36)). Komodo AdaptivePoW applies the same idea and only triggers after the maximum backdating, the future margin and a buffer have passed ([Komodo](https://komodoplatform.com/adaptive-proof-of-work/)). It meets the three conditions that separate safe relief from EDA:

- the relief is continuous and modest per T, not a jump to powLimit;
- it starts only when the wait exceeds what a forward timestamp can fake;
- it does not contaminate the DAA's base.

## What to simulate next, with ready formulas

ASERT's recovery after a drop has a closed form. Let r be the remaining fraction of hashrate and u the relative difficulty. In the continuous approximation, `u(t) = r + (1−r)·e^(−t/τ)`, with τ = M·T in wall-clock seconds. This derivation comes from the notes and was pending validation in the simulator (see the update below). Applying our simulator's metric (interval < 1.5·T ⇒ u < 1.5·r) gives `t = τ·ln((1−r)/(0.5·r))`. That gives factors of 0.69·τ at −50%, 2.08·τ at −80% and 5.29·τ at −99%.

| Variant | Half-life | τ | −50% | −80% | −99% | D noise (1/√2M) |
|---|---|---|---|---|---|---|
| ASERT M=24 | 1198 s | 0.48 h | 0.3 h | 1.0 h | 2.5 h | 14% |
| **ASERT M=36** | **1797 s** | **0.72 h** | **0.5 h** | **1.5 h** | **3.8 h** | **12%** |
| ASERT M=48 | 2396 s | 0.96 h | 0.7 h | 2.0 h | 5.1 h | 10% |
| ASERT M=72 | 3593 s | 1.44 h | 1.0 h | 3.0 h | 7.6 h | 8% |
| *DGW v3 (sim)* | — | — | *0.1 h* | *0.4 h* | *8.3 h* | — |
| *LWMA-60 (sim)* | — | — | *0.2 h* | *1.5 h* | *20.6 h* | — |

These figures are an analytical bound and do not include the delay of the trailing 24-block window our metric uses, so the simulator should give somewhat more. The comparison says that **ASERT M=36 has the noise of LWMA-72 and recovers from a −99% drop more than twice as fast as DGW**. In exchange, on moderate drops (−50%) it responds more slowly than DGW, because DGW reacts with little filtering. That is the unavoidable trade-off between stability and response.

LWMA-60 taking 20.6 h at −99% fits LWMA "falling ~50% slower" than ASERT ([zawy12 #50](https://github.com/zawy12/difficulty-algorithms/issues/50)). A hypothesis worth checking in the simulator's code is the 6T solvetime cap LWMA-1 usually carries. With that cap, every 2-hour block counts as 6 minutes.

For profit switching, the noise equivalence predicts that ASERT M=36 will land close to LWMA-60/90, about 12 gaps over 7 minutes per day against DGW's 18.6. That is also a prediction to check. Because merge-mining hashrate is free and the likely events are pool disconnects rather than switching, **M should be much lower than BCH's 415 blocks**. The sensible range is 24 to 48.

### Candidate 1: ASERT aserti3-2d, sweep M ∈ {24, 36, 48, 72}, default M=36

This is the exact Bitcoin ABC/BCHN code ([aserti32d.cpp](https://github.com/Bitcoin-ABC/bitcoin-abc/blob/master/src/pow/aserti32d.cpp); [BCHN spec](https://upgradespecs.bitcoincashnode.org/2020-11-15-asert/)), with `T = 72` and `nHalfLife = round(M·72·ln2)`, i.e. 1797 s for M=36:

```
// anchor: fixed fork block A; anchorTime = time(parent(A)); refTarget = target(A)
nTimeDiff   = time(prev) - anchorTime;              // prev block's own nTime, NOT MTP
nHeightDiff = height(prev) - height(A);
exponent    = ((nTimeDiff - T*(nHeightDiff + 1)) * 65536) / nHalfLife;  // truncating div
shifts      = exponent >> 16;                        // arithmetic shift (floor)
frac        = exponent & 0xFFFF;
factor      = 65536 + ((195766423245049*frac + 971821376*frac^2
                        + 5127*frac^3 + (1<<47)) >> 48);   // 2^frac, error < 0.013%
next        = refTarget * factor;  shifts -= 16;
next        = shifts <= 0 ? next >> -shifts : (overflow ? powLimit : next << shifts);
next        = clamp(next, 1, powLimit);
```

The implementation has no clamps, no median of 3 and no cap on difficulty increases. Lundeberg advises against any clipping because it breaks the equivalence between the relative and absolute forms and induces overshoot ([DA-ASERT v2](http://toom.im/files/da-asert.pdf)).

As a cheap cross-check, WTEMA can be simulated alongside: `next = prev * (T*(M−1) + st) / (T*M)`. It should match ASERT except for extreme solvetimes, where ASERT drops further (exponential rather than linear), which is exactly Maximus's regime ([zawy12 #62](https://github.com/zawy12/difficulty-algorithms/issues/62#issuecomment-648958314)).

### Candidate 2: ASERT M=36 plus a single-block long-delay layer

For the block being mined, with `st = nTime − max(nTime of the last 11 blocks)` and `L = 6·T + FTL`:

```
target_eff = target_asert                                  if st <= L
target_eff = min(powLimit, target_asert * 2^((st - L)/H))  if st >  L
```

With `H = 480 s` (≈ ×0.9 per T, zawy's rate) and the same integer 2^x polynomial. With FTL = 300 s, L = 732 s.

ASERT's absolute form has a useful property here. **ASERT only reads the anchor's nBits**, never the previous block's. The header can store `target_eff` in nBits, chainwork accumulates the work actually solved (conservative accounting), and the base is not contaminated without needing a separate "official target". The only condition is that the anchor is fixed in code and is not a relieved block.

At −99%, this layer cuts the median first block from ~83 minutes (ln2 · 7200 s) to ~35 minutes. That is an analytical estimate pending simulation. It is important to be clear about what this layer does **not** do: it does not speed up the base's recovery. ASERT falls with wall-clock time, so the base still needs ~3.8 h. What changes is that instead of hour-long gaps there are blocks every ~30–40 minutes. It must therefore be measured with the gap metric and the maximum wait, not only the 24-block one.

It has two side effects the simulator must model. First, future mining near L, which is bounded because the relief per T is small. Second, under AuxPoW the child's nTime is fixed when the aux work is built, so pools that refresh late do not get the relief. That is conservative, but it requires refreshing `createauxblock`/`getauxblock` every few seconds ([zawy12 #35](https://github.com/zawy12/difficulty-algorithms/issues/35)).

### Candidate 3 (control): corrected DGW

`next_target = avg(24 targets) * (ts[tip] − ts[tip−24]) / (24·T)`, with a true average and no off-by-one ([zawy12 #31](https://github.com/zawy12/difficulty-algorithms/issues/31)), keeping the clamps. Our replay already shows that LWMA-24's gain equals this fix. Simulating it as a minimal baseline separates how much of ASERT's improvement comes from the filter and how much from removing the bias.

### Timestamp changes that must accompany any of the three

The first is **lowering `MAX_FUTURE_BLOCK_TIME` from 7200 to ~300 s**. Under ASERT, the largest discount a forward timestamp can get is e^(−FTL/τ). With the current FTL and M=36 that is ×16 on one block. With 300 s it stays at ~11%, and the next honest timestamp undoes it. zawy recommends 300 s or a small fraction of N·T, and Toomim prefers tackling the FTL over lengthening τ ([zawy12 #62](https://github.com/zawy12/difficulty-algorithms/issues/62); [Toomim](https://read.cash/@jtoomim/bch-upgrade-proposal-use-asert-as-the-new-daa-1d875696)). There is a trap in `src/chain.h`: `TIMESTAMP_WINDOW = MAX_FUTURE_BLOCK_TIME` is used for wallet rescans. It must be decoupled and kept at 7200, because historical blocks were stamped under that tolerance.

The second is **lowering `DEFAULT_MAX_TIME_ADJUSTMENT` from 70 min to ≤ FTL/2 (150 s), or dropping peer time**. Otherwise lowering the FTL opens Sybil timejacking ([zawy12 #76](https://github.com/zawy12/difficulty-algorithms/issues/76)).

The third is **requiring monotonic timestamps (nTime > previous nTime)** if the long-delay layer is enabled, or at least measuring st against the maximum of the last 11. ASERT alone tolerates negative st safely, but WTEMA does not. `if (st < 0) st = 0` must never be used ([zawy12 #49](https://github.com/zawy12/difficulty-algorithms/issues/49)).

The fourth is keeping the min-difficulty rule inherited from Dash on testnet only, and giving testnet a shorter ASERT half-life, as BCH did with 3600 s ([BCHN spec](https://upgradespecs.bitcoincashnode.org/2020-11-15-asert/)).

One open question no source answers: how many orphans an FTL of ~4·T produces given the template latency of merge-mining pools. It has to be measured on testnet before fixing the number.

## Conclusion

The research shifts the focus of the question. Maximus's problem is not which filter to pick, since the good ones are nearly interchangeable at equal noise. The real problem is that recovery after a pool disconnect is limited by τ in wall-clock time and by the first wait of 100·T, and that the 2-hour FTL cancels any improvement in response. ASERT beats LWMA for this profile for structural reasons: it falls with wall-clock time, has no per-block cap, accumulates no error, and its absolute form allows real-time relief without contaminating the base. It does not win by being a better filter in general.

The sequence that follows from the sources is: timestamps first (FTL, peer time, monotonicity), then ASERT with M between 24 and 48, and only if the maximum-gap metrics justify it, the long-delay layer. Deploying in another order would repeat the mistakes of EDA and testnet3. The biggest uncertainty, which only our simulator can resolve, is whether M=36 keeps under profit switching the improvement the noise equivalence promises while also coping with −80% drops (the estimate gives ~1.5 h, the same as LWMA-60, against DGW's 0.4 h). Our replay's finding that LWMA-24 only matched a corrected DGW is a warning that any improvement must be measured against that control, not against the current DGW.

## Update: simulated in difficulty-lab (2026-10-23)

ASERT is now implemented in `src/algo.rs` as a port of the integer aserti3-2d code, anchored at the last block before the simulated switch, and runs in every scenario (τ = M in blocks).

| | DGW v3 | LWMA-60 | ASERT τ=24 | **ASERT τ=36** | ASERT τ=72 |
|---|---|---|---|---|---|
| Hours to normal pace, −50% / −80% / −99% | 0.1 / 0.4 / 8.3 | 0.2 / 1.5 / 20.6 | 0.1 / 0.7 / 2.7 | **0.2 / 1.1 / 3.6** | 0.5 / 2.3 / 6.8 |
| Blocks in the first 6 h at −99% (target 300) | 19 | 4 | 179 | **126** | 32 |
| Blocks in the first hour after the hashrate returns, −99% (target 50) | 96 | 135 | 156 | **201** | 304 |
| Replay: mean block time | 75.6 s | 72.3 s | 72.0 s | **72.0 s** | 72.0 s |
| Replay: difficulty jitter | 6.1% | 3.1% | 4.0% | **2.6%** | 1.3% |
| Profit switching: gaps over 7 min per day | 18.6 | 12.0 | 12.7 | **11.9** | 11.2 |

The analytical predictions hold: τ=36 recovers a −99% drop in 3.6 h (predicted 3.8 h) and matches LWMA-60 under profit switching. The 6T solvetime cap in our LWMA-1 is confirmed as the reason LWMA-60 is so slow after a near-total collapse.

The cost the analysis did not quantify is the burst when the hashrate returns. ASERT ties difficulty to how far the chain is behind its ideal schedule, so after a −99% drop it has to win back about τ·ln(100) blocks (~166 for τ=36) once the miners come back: about 200 blocks in the first hour against 96 for DGW. Total emission stays on schedule, but those blocks come every ~18 s for a while, with more orphan risk. A larger τ makes the burst larger. The long-delay layer (candidate 2) is not simulated yet; it needs the simulator to support a target that falls while a block is being mined.

### The two controls: corrected DGW and WTEMA

Both are now in the simulator too. "DGW corrected" keeps the clamps and fixes both bugs: the off-by-one timespan and the running "average" that counts the latest target twice. WTEMA uses M=36 to match ASERT τ=36.

| | DGW v3 | DGW corrected | ASERT τ=36 | WTEMA M=36 |
|---|---|---|---|---|
| Replay: mean block time | 75.6 s | 72.5 s | 72.0 s | 72.9 s |
| Replay: gaps over 7 min per day | 4.8 | 3.9 | 2.9 | 3.1 |
| Replay: difficulty jitter | 6.1% | 6.1% | 2.6% | 2.6% |
| Profit switching: gaps over 7 min per day | 18.6 | 17.9 | 11.9 | 12.1 |
| Hours to normal pace, −50% / −80% / −99% | 0.1 / 0.4 / 8.3 | 0.1 / 0.4 / 8.8 | 0.2 / 1.1 / 3.6 | 0.2 / 1.2 / 5.2 |
| Blocks in the first 6 h at −99% (target 300) | 19 | 15 | 126 | 62 |
| Blocks in the first hour after the return, −99% | 96 | 99 | 201 | 198 |

The corrected DGW only fixes the pace: blocks come at 72.5 s instead of 75.6 s, and gaps drop by about a fifth in the replay. Its jitter, its response to profit switching and its recovery from a collapse stay the same as the current DGW. The rest of ASERT's improvement therefore comes from the filter, not from removing DGW's bias.

WTEMA matches ASERT under normal conditions and under profit switching, as the literature predicts. It falls behind after a near-total collapse (5.2 h instead of 3.6 h, half the blocks in the first 6 h), because it lowers difficulty linearly with each long solvetime while ASERT lowers it exponentially with elapsed time. It also runs about 1% slow (72.9 s), the known bias of applying the filter to the target rather than to difficulty. ASERT has neither problem and also gets the absolute schedule, so it stays the main candidate.

### Sweeping τ from 20 to 40

The report now runs ASERT at every even τ from 20 to 40 through the same tests. Every metric moves monotonically with τ, so no value wins everything. Difficulty jitter falls from 4.8% to 2.4% and the switcher edge from 1.10 to 1.05. Recovery after a −99% collapse grows from 2.4 h to 4.0 h, and the burst on return from 140 to 214 blocks. With every column weighted equally, the best compromise is **τ=24 to 26** (half-life about 20 minutes), and the score is almost flat from 22 to 30. Below 24 the gain in recovery is small and jitter rises quickly. Above 32 every column that measures collapses gets clearly worse.
