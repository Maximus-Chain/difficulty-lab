# Academic and systematic evaluations of PoW difficulty adjustment algorithms (DAAs)

Context: small merge-mined (AuxPoW) X11 Dash fork, 72 s target, currently DGW v3. Candidates: LWMA-1, EMA/ASERT, others. Problem profile: sudden 80-99% hashrate drops and occasional profit switching.

## Q1. Papers modelling DAAs as control systems / stochastic processes / economic games

### Takeaway
The formal literature is small and mostly about Bitcoin (2016-block SMA) and Bitcoin Cash (CW-144 SMA, then ASERT). Its main results: (a) SMA-type retargeting carries an N/(N-1) bias in expected block time and its variance blows up for small N (Fullmer & Morse); (b) economic models with elastic hash supply show that slow/periodic DAAs fail and that stability depends on hash-supply elasticity relative to the adjustment window (Noda et al.); (c) moving-average windows create a positive feedback loop and resonant oscillation with period equal to the window, which exponential (EMA/ASERT) filters remove (Ilie/Werner et al.; ASERT spec). No peer-reviewed paper benchmarks DGW or LWMA. The head-to-head numbers for LWMA vs ASERT vs WT come from practitioner simulations (Toomim, zawy12), not from peer-reviewed venues.

### Cited Findings

**Kraft (2016)**
- Daniel Kraft, "Difficulty control for blockchain-based consensus systems," *Peer-to-Peer Networking and Applications* 9(2):397–413, 2016 (online 2015), DOI 10.1007/s12083-015-0347-x. About 297 citations on Semantic Scholar. — [DBLP](https://dblp.org/pid/84/101); [Semantic Scholar API record](https://api.semanticscholar.org/graph/v1/paper/DOI:10.1007/s12083-015-0347-x?fields=title,abstract,year,venue,citationCount)
- Later papers describe Kraft's contribution as an improved Bitcoin difficulty rule built on the assumption that hashrate grows exponentially. — [Zhang & Ma 2020, ref. [5]](https://arxiv.org/pdf/2004.10670)
- I could not retrieve the abstract (the publisher elides it and the page is paywalled), so its detailed results are listed under Gaps.

**Fullmer & Morse (2018), stochastic model with random difficulty**
- Daniel Fullmer and A. Stephen Morse, "Analysis of Difficulty Control in Bitcoin and Proof-of-Work Blockchains," IEEE Conference on Decision and Control (CDC) 2018, pp. 5988–5992; arXiv:1812.10792. — [arXiv](https://arxiv.org/abs/1812.10792)
- Model: difficulty is treated as a random variable that depends on previous block times. Under the Bitcoin-style rule d_{k+1} = d_k·Nβ/ΣX (window N, target β) with constant hashrate, block times after the first period are **Lomax(N, θ)** distributed instead of exponential. — [arXiv PDF](https://arxiv.org/pdf/1812.10792)
- **Corollary 1:** E[X] = **β·N/(N−1)** and Var[X] = **β²·N³/((N−1)²(N−2))**. The variance is infinite for N = 2. The authors suggest replacing N by N−1 in the numerator, which gives E[X] = β exactly and Var = N/(N−2)·β². Note that the text says the chain "runs too fast by a factor of N/(N−1)", but the formula gives an expected block time *longer* than β (slower blocks). This internal wording inconsistency should be checked against the formula. — [arXiv PDF](https://arxiv.org/pdf/1812.10792)
- The paper also notes that Bitcoin's code has a "well-known bug which excludes the final X_k in the sum" (the 2015-of-2016 off-by-one). — [arXiv PDF](https://arxiv.org/pdf/1812.10792)
- Proposed evaluation criterion: how closely the expected block time 1/λ_k stays at the target, illustrated by simulations with N = 20 and N = 2. The conclusion lists other coins' rules and interaction between chains that share a PoW as future work. — [arXiv PDF](https://arxiv.org/pdf/1812.10792)

**Hovland & Kucera (2017), nonlinear feedback control**
- Geir Hovland and Jan Kucera, "Nonlinear Feedback Control and Stability Analysis of a Proof-of-Work Blockchain," *Modeling, Identification and Control* 38(4):157–168, 2017, DOI 10.4173/mic.2017.4.1. — [MIC](https://mic-journal.no/ABS/MIC-2017-4-1)
- The model treats difficulty as the controller output, average block time as the feedback variable and hashrate as a disturbance. The controller was tested against constant, step, ramp and high-frequency sinusoidal hashrate disturbances, then on a testnet and on a live mainnet (Bismuth). It adjusts every block, and the authors claim stability and fast response in all cases. The abstract gives no numeric gains or windows. — [MIC](https://mic-journal.no/ABS/MIC-2017-4-1)

**Meshkov, Chepurnoy & Jansen (2017), coin-hopping**
- Dmitry Meshkov, Alexander Chepurnoy and Marc Jansen, "Short Paper: Revisiting Difficulty Control for Blockchain Systems," DPM/CBT 2017 (ESORICS workshops), LNCS, pp. 429–436; IACR ePrint 2017/731. — [ePrint](https://eprint.iacr.org/2017/731)
- They define the **coin-hopping attack**: a miner exploiting Bitcoin's retargeting raises their own profit *and* raises the average inter-block delay for everyone. They propose an alternative DAA that "reduces the incentive" to coin-hop and makes delays more stable, and claim it outperforms Bitcoin's rule. The PDF was blocked (HTTP 403), so I have no numbers. — [ePrint](https://eprint.iacr.org/2017/731)

**Noda, Okumura & Hashimoto (2019–2025), economic model with elastic hash supply**
- Shunya Noda, Kyohei Okumura and Yoshinori Hashimoto, "An Economic Analysis of Difficulty Adjustment Algorithms in Proof-of-Work Blockchain Systems," *International Economic Review* (accepted 2025; Wiley lists it as 2026), DOI 10.1111/iere.70028; SSRN 3410460. An earlier version was titled "A Lucas Critique to the Difficulty Adjustment Algorithm of the Bitcoin System." — [Wiley](https://onlinelibrary.wiley.com/doi/10.1111/iere.70028); [SSRN](https://papers.ssrn.com/sol3/papers.cfm?abstract_id=3410460); [ScalingBitcoin 2019 transcript](https://scalingbitcoin.org/transcript/telaviv2019/wip-a-lucas-critique-of-the-difficulty-adjusmtent-algorithm-of-the-bitcoin-system)
- Model: miners shut down when the reward falls below their marginal cost, so hashrate responds to price and difficulty. Bitcoin's DAA (periodic, N = 2016) fails to stabilise block times under highly elastic hash supply and can collapse after a sharp drop in fiat reward. BCH's CW-144 is stable even with volatile prices and elastic supply. — [SSRN](https://papers.ssrn.com/sol3/papers.cfm?abstract_id=3410460); [HKBU slides](https://cbade.hkbu.edu.hk/wp-content/uploads/2020/09/20201016_NODA.pdf)
- Formal stability thresholds as summarised by the authors: the Bitcoin DAA "performs well iff elasticity < 1", and **CW-144 is "stable iff elasticity < 144"**. In other words, the elasticity a moving-average DAA tolerates scales with its window or update frequency. — [Kawaguchi & Noda slides, U-Tokyo CARF 2021](https://www.carf.e.u-tokyo.ac.jp/wp/wp-content/uploads/2021/09/211001Noda.pdf)
- Incentive result: high-marginal-cost miners profit from an unstable DAA and may oppose reform. — [SUFE newsletter summary](https://academicnewsletter.sufe.edu.cn/info/318658)

**Kawaguchi & Noda (2021–2023), competing chains sharing one hash market**
- Kohei Kawaguchi and Shunya Noda, "Algorithmic Competition and Coordination Failure between Cryptocurrencies" (AEA 2023 program), also circulated as "Security-Cost Efficiency of Competing Proof-of-Work Cryptocurrencies" (SSRN). — [AEA 2023](https://aeaweb.org/conference/2023/program/paper/yrKnD47n); [slides](https://www.carf.e.u-tokyo.ac.jp/wp/wp-content/uploads/2021/09/211001Noda.pdf)
- Method: hashrate is estimated by maximum likelihood from Poisson block arrivals, around the 2020 BTC halving treated as an exogenous shock (a regression-discontinuity design). Hash supply is modelled as log-linear in the rewards of all SHA-256 chains. — [slides](https://www.carf.e.u-tokyo.ac.jp/wp/wp-content/uploads/2021/09/211001Noda.pdf)
- Estimated own-reward elasticities: **BTC 0.626, BCH 5.386, BSV 4.869**. Cross-elasticities: a 1% rise in BTC reward cuts BCH hash by 3.98% and BSV hash by 3.19%. A forum summary prints the own-elasticities with a negative sign, but the slides show them as positive. — [slides](https://www.carf.e.u-tokyo.ac.jp/wp/wp-content/uploads/2021/09/211001Noda.pdf); [BCH Research forum](https://bitcoincashresearch.org/t/empirical-analysis-of-the-aserts-performance/708)
- Counterfactual: if BCH or BSV had used Bitcoin's 2016-block DAA through the 2020 halving, they **would have collapsed**. Hash would flood in, difficulty would overshoot, and the chain would then fail to produce the 2016 blocks needed for the next retarget. CW-144 absorbed the shock quickly. — [slides](https://www.carf.e.u-tokyo.ac.jp/wp/wp-content/uploads/2021/09/211001Noda.pdf)
- Upgrading all three chains to **ASERT** "clearly outperforms CW-144 and BTC's original DAA" and would save about 0.21 GW (3.2%) of energy at equal security. — [BCH Research forum, Feb 2022](https://bitcoincashresearch.org/t/empirical-analysis-of-the-aserts-performance/708)

**Ilie, Werner, Stewart & Knottenbelt (2020/2021), "Unstable Throughput"**
- Dragos I. Ilie, Sam M. Werner, Iain Stewart and William J. Knottenbelt, "Unstable Throughput: When the Difficulty Algorithm Breaks," arXiv:2006.03044 (v3, Apr 2021). The arXiv page lists no venue. — [arXiv](https://arxiv.org/abs/2006.03044)
- Empirical result: BCH's cw-144 produces cyclic solve times, with block times about 144 blocks (24 h) apart autocorrelated, and many 1-hour buckets with bursts of blocks or no blocks at all. The daily average hides this. — [ar5iv full text](https://ar5iv.arxiv.org/html/2006.03044)
- Mechanism: in an SMA, a solvetime leaving the window weighs as much as one entering it. Hopper-induced fast blocks therefore push the estimate wrong twice, once on entry and once on exit, giving a **positive feedback loop**. — [ar5iv](https://ar5iv.arxiv.org/html/2006.03044)
- Proposal: NEFDA (negative exponential filter), D_n = D_{n−1}·e^{(T−st_n)/S}, or in absolute form D_0·e^{(t_0+nT−t_n)/S}. It is history-agnostic and the authors say it is "essentially equivalent" to ASERT. It uses a correction factor e^{T/S} for discrete-block bias. — [ar5iv](https://ar5iv.arxiv.org/html/2006.03044)
- Simulation: 100,000 blocks with baseline miners plus greedy miners (4× base hash, switching on at +5% profitability) and variable miners (4× base hash, logistic response). Mean solvetime was **NEFDA 599.97 s vs cw-144 604.34 s** (target 600 s). cw-144 shows positive autocorrelation at 24 h multiples, while NEFDA shows none. — [ar5iv](https://ar5iv.arxiv.org/html/2006.03044)
- Window equivalence heuristic: an EMA with time constant S equal to (N+1)/2 blocks matches the stability of an N-block SMA, so N = 144 corresponds to S ≈ 72 blocks (12 h). — [ar5iv](https://ar5iv.arxiv.org/html/2006.03044)
- Timestamp side-effect: under cw-144 one dishonest timestamp lowers difficulty for 144 blocks. Under NEFDA it affects only that block. — [ar5iv](https://ar5iv.arxiv.org/html/2006.03044)

**Zhang & Ma (2020), neural-network DAA**
- Shulai Zhang and Xiaoli Ma, "A General Difficulty Control Algorithm for Proof-of-Work Based Blockchains," IEEE ICASSP 2020; arXiv:2004.10670. — [arXiv](https://arxiv.org/pdf/2004.10670)
- Stated design criteria: (1) simplicity and low memory, using header data only; (2) **fast updating and low volatility**, with difficulty that does not bounce under constant hashrate; (3) no incentive to fiddle with timestamps. — [arXiv](https://arxiv.org/pdf/2004.10670)
- With Ethereum data and ±20%/40% injected hash steps, difficulty MSE fell **66.3% and 76.7%** relative to Ethereum's Homestead rule, with similar convergence time. The method deliberately suppresses reaction to short anomalies, which slows its response to genuine sudden drops. The anomaly classifier reached only 78.4% accuracy after 1,000 blocks. — [arXiv](https://arxiv.org/pdf/2004.10670)

**Garay, Kiayias & Leonardos (CRYPTO 2017), formal security with variable difficulty**
- Juan Garay, Aggelos Kiayias and Nikos Leonardos, "The Bitcoin Backbone Protocol with Chains of Variable Difficulty," CRYPTO 2017; IACR ePrint 2016/1048. A journal version appeared in J. ACM in 2024. — [ePrint](https://eprint.iacr.org/2016/1048); [IOG](https://www.iog.io/papers/the-bitcoin-backbone-protocol-with-chains-of-variable-difficulty)
- This is the first cryptographic-model analysis of the target recalculation function. It gives the conditions on how the miner population may change (how fast parties join or leave) under which common-prefix and chain-quality hold with an adversary below 50%. It is security-oriented and does not compare DAAs for performance. — [ePrint](https://eprint.iacr.org/2016/1048)

**Bissias, Thibodeau & Levine (2019), Bonded Mining**
- George Bissias, David Thibodeau and Brian N. Levine, "Bonded Mining: Difficulty Adjustment by Miner Commitment," DPM/CBT 2019 (Springer LNCS, pp. 372–390); arXiv:1907.00302. — [arXiv](https://arxiv.org/pdf/1907.00302)
- Classifies conventional DAAs as reactive feedback controllers with three weaknesses: over/under-correction causing oscillation, instability on minority-hash chains after contentious forks, and gaming by miners. Bonded Mining sets difficulty from bonded hashrate commitments instead. It beats the BCH DAA (cw-144) on amplitude and duration of deviations from target, even when miners will sacrifice 25% of bond. It supports miners down to 1% of hash, and its detection test catches a drop to 20% of commitment lasting a week or more. — [arXiv](https://arxiv.org/pdf/1907.00302)

**Bissias & Levine, Bobtail (NDSS 2020)**
- George Bissias and Brian N. Levine, "Bobtail: Improved Blockchain Security with Low-Variance Mining," NDSS 2020; arXiv:1709.08750. — [NDSS](https://ndss-symposium.org/ndss-paper/bobtail-improved-blockchain-security-with-low-variance-mining)
- Compares the mean of the k lowest hashes to the target, which cuts block-time variance by O(1/k). A 40% double-spender succeeds 53% of the time with a 1-block embargo today, but under 1% for k ≥ 40. The costs are bigger blocks and more traffic. This changes the PoW itself, not the DAA. — [NDSS](https://ndss-symposium.org/ndss-paper/bobtail-improved-blockchain-security-with-low-variance-mining)

**Harding (2020), Real-Time Targeting (RTT)**
- Thomas M. Harding, "Real-Time Block Rate Targeting," *Ledger* 5 (2020) 11–19, DOI 10.5195/ledger.2020.195. — [Ledger](https://ledgerjournal.org/ojs/ledger/article/view/195)
- The target eases as time passes during the search for the current block. The design stays progress-free but gives up memorylessness, and requires a tighter future-time limit (max timestamp equal to "now"). The paper notes that CW-144 "exhibits a worrisome resonant hashrate oscillation with a period of 144 blocks." It explicitly gives no quantitative comparison with other DAAs. — [Ledger PDF](https://ledgerjournal.org/ojs/ledger/article/download/195/187)

### Inferences
- The control-theory and economics literature converges on a few points. Pure SMA windows are the worst family under elastic or hopping hash: they create the double-counting feedback and resonance at the window period. Exponential filters (EMA/ASERT/NEFDA) remove the "exit" effect and are the best-supported choice. Relative EMA and absolute ASERT are mathematically near-equivalent; ASERT adds immunity to accumulated rounding error.
- DGW v3 is essentially an SMA over 24 blocks (average target times actual/target timespan, clamped to 3×). It therefore belongs to the family that Fullmer & Morse and Ilie et al. analyse. With N = 24, the N/(N−1) bias predicts about **+4.3% mean block time (≈75.1 s instead of 72 s)** under constant hashrate if DGW behaves like the harmonic-mean estimator in Fullmer & Morse. The variance factor N³/((N−1)²(N−2)) ≈ 1.18 means about 9% more standard deviation than exponential block times. This is my extrapolation, not a published DGW result. DGW averages *targets* (arithmetic) rather than difficulties, so the exact constant should be verified in the project's Rust simulator.
- From Noda et al.'s "stable iff elasticity < window" result: a small merge-mined coin with near-total hash elasticity (80–99% drops) needs a DAA whose effective response is fast relative to the elasticity. A short window alone is not enough, because Fullmer & Morse show variance explodes as N gets small. This is the core stability-vs-responsiveness trade-off.

### Gaps
- Kraft (2016): full abstract and quantitative results unavailable (paywalled, abstract elided by the publisher).
- Meshkov et al. (2017): the PDF returned 403, so the algorithm details and profit numbers were not retrieved.
- Aggarwal & Tan: the only trace found is a bibliography entry, "A Structural Analysis of Bitcoin Cash's Emergency Difficulty Adjustment Algorithm," SSRN 3383739 (c. 2019; the citing reference misprints the date). The title "A Structural Model of Difficulty Adjustment" could not be confirmed, and I could not retrieve the abstract or findings. — [cited in arXiv 1812.10792 search context](https://arxiv.org/pdf/1812.10792)
- No peer-reviewed paper analyses **DGW** or **LWMA** specifically. I found nothing on X11 or Dash-family DAAs in academic venues.
- Hovland & Kucera: the numeric gains and windows are in the PDF, which I did not read.

## Q2. Metrics used and conclusions on window length, EMA vs SMA vs LWMA, asymmetric rules

### Takeaway
Metrics in use fall into four groups: mean block-time bias, block-time and difficulty variance, response or recovery to hash steps, and incentive metrics (hopper excess profit, "% stolen", "% delayed", steady-miner profit loss). Comparisons at **matched stability** (equal difficulty standard deviation under constant hash) show ASERT/EMA, LWMA and WT within a few tenths of a percent of each other, while SMA (CW-144) is clearly worse. ASERT was chosen for BCH on these metrics. Practitioners advise against asymmetric limits that slow difficulty *rises*.

### Cited Findings
- **Matched-stability methodology** (zawy12/Toomim): pick N for each algorithm so that difficulty standard deviation under constant hashrate is equal, then compare responses to hash steps and on-off miners. Equivalences: LWMA N ≈ 2 × EMA/ASERT N; SMA N ≈ 4 × Digishield N. For EMA/ASERT, the standard deviation of D ≈ 1/√(2N), so ASERT-72 matches SMA-144. ASERT mean lifetime τ/T = half-life / ln 2. — [zawy12/difficulty-algorithms #62](https://github.com/zawy12/difficulty-algorithms/issues/62)
- **Metrics:** "% delayed" is Σ max(0, st − 4T) divided by block count. "% stolen" is the time-weighted target an on-off miner sees relative to dedicated miners. The best algorithm minimises % stolen + % delayed across attack sizes. — [zawy12 #62](https://github.com/zawy12/difficulty-algorithms/issues/62)
- **Toomim's profitability simulation** (June 2020; % change in profit vs a BTC-equivalent baseline): steady miners lose −0.579% (asert-144), −0.927% (lwma-144), −0.648% (wt-144), −0.463% (asert-288), −0.482% (lwma-288), −0.515% (lwma-576). Greedy-miner gains are within ±0.07% for all of them. **CW-144 lost about 13% for steady miners and was rejected.** — [zawy12 #62](https://github.com/zawy12/difficulty-algorithms/issues/62)
- In zawy12's matched-stability ranking only SMA differed significantly. WT had slightly lower % stolen but more delay. — [zawy12 #62](https://github.com/zawy12/difficulty-algorithms/issues/62)
- Practitioner recommendations in the same thread:
  - No 1/2 timespan limit and nothing that limits how fast difficulty **rises**.
  - FTL of about 300 s, with peer time removed or set to FTL/2.
  - No median-of-3.
  - Cap solvetime at 7T.
  - For 60 s coins, WTEMA 1/α ≈ 180, with 90 as the minimum (zawy12), vs ≥ 576 (Toomim).
  - Grin's tromp judged a 4 h half-life the upper limit.
  - The right N depends on the coin's hash market and on how big greedy miners are.
  - Rounding errors in WTEMA/ASERT grow with N.
  - Negative solvetimes must not be clamped to 0.
  - Floating point is a consensus risk.

  — [zawy12 #62](https://github.com/zawy12/difficulty-algorithms/issues/62)
- **ASERT spec (BCH, Nov 2020)**, by freetrader, Jonathan Toomim, Calin Culianu and Mark Lundeberg (v0.6, 2020-08-12). Goals: remove CW-144 oscillations, reduce the steady-vs-switching profitability gap, hold 600 s, and cut confirmation times. In Toomim's simulations ASERT came closest to both the block-time and confirmation-time targets. The half-life is 2 days (172,800 s). The integer cubic approximation of 2^x has error < 0.013%, against single-block variance of about 0.347% over 2 days. There is no accumulated rounding error. Equivalent formulas were found independently by Eliosoff (2018) and Werner et al. (2020). — [BCHN upgrade spec](https://upgradespecs.bitcoincashnode.org/2020-11-15-asert/)
- Ilie et al.: a smaller time constant S suits volatile chains, a larger S suits stable chains, and S ≈ (N+1)/2 blocks matches SMA-N stability. — [ar5iv 2006.03044](https://ar5iv.arxiv.org/html/2006.03044)
- Fullmer & Morse metric: the expected block time 1/λ_k and its variance. Bias is N/(N−1) and the variance term N³/((N−1)²(N−2)) blows up at small N. — [arXiv 1812.10792](https://arxiv.org/pdf/1812.10792)
- Zhang & Ma metric: difficulty MSE and mean in fixed periods after hash steps, plus convergence time. — [arXiv 2004.10670](https://arxiv.org/pdf/2004.10670)
- Bonded Mining metric: amplitude and duration of deviation from target block time. — [arXiv 1907.00302](https://arxiv.org/pdf/1907.00302)
- Noda et al. metric: whether block time converges or collapses after reward shocks, as a function of hash-supply elasticity. — [SSRN](https://papers.ssrn.com/sol3/papers.cfm?abstract_id=3410460)

### Inferences
- A suitable evaluation suite for the target chain combines all four metric groups, compared at matched constant-hash difficulty standard deviation:
  1. Mean block-time bias under constant hash.
  2. Block-time standard deviation and tail metrics (% delayed with st > 4T).
  3. Recovery time after −80/−90/−99% steps, counting how many blocks and how much wall-clock time until block time is within ±X% of 72 s.
  4. Hopper excess profit or % stolen plus steady-miner loss under on-off miners of 1×–10× base hash.
- On the literature, ASERT/EMA is the best-supported choice. LWMA performs comparably at matched stability in Toomim's tables, slightly worse at equal nominal N, and within noise at matched N.
- For 99% drops, the dominant problem is the *wall-clock* time to mine the blocks the DAA needs before it can react. Per-block exponential rules react on the first slow block. RTT (Harding) or tail-removal rules address the stuck-chain case directly, at the cost of tighter timestamp rules.

### Gaps
- No peer-reviewed head-to-head LWMA vs ASERT vs DGW study exists. The best quantitative comparisons are practitioner GitHub and forum work.
- No academic treatment of asymmetric rules (fast down, slow up) was found, beyond practitioner advice to avoid limiting rises.

## Q3. Empirical studies of hashrate shocks (BCH 2017, small-coin attacks, merge-mined chains)

### Takeaway
The empirical evidence covers BCH's EDA/CW-144 oscillations (Ilie et al.; Harding; Aggarwal & Tan, not retrieved), the 2020 halving shock (Kawaguchi & Noda), and merge-mined chains where pools have operated beyond Nakamoto security bounds (Judmayer et al.). Elastic hash supply also amplifies selfish mining (Shibuya et al.).

### Cited Findings
- BCH's base hash is about 3% of BTC's, so hoppers have outsized effects. cw-144 produces 24 h-periodic burst-and-drought patterns. — [ar5iv 2006.03044](https://ar5iv.arxiv.org/html/2006.03044)
- Minority-hash chains after contentious forks must "resort to a backup controller" (BCH EDA) that compensates for swings in miner allocation. — [Bonded Mining, arXiv 1907.00302](https://arxiv.org/pdf/1907.00302)
- Coin-hopping swings have been large enough to end altcoins: miners leave and blocks become so slow that users give up. — [Harding, Ledger 2020](https://ledgerjournal.org/ojs/ledger/article/download/195/187)
- After the 2020 BTC halving, BTC's hashrate fell and its block time rose for weeks. BCH and BSV (both on CW-144) absorbed large hash inflows quickly. — [Kawaguchi & Noda slides](https://www.carf.e.u-tokyo.ac.jp/wp/wp-content/uploads/2021/09/211001Noda.pdf)
- **Merge-mined chains:** Aljosha Judmayer, Alexei Zamyatin, Nicholas Stifter, Artemios G. Voyiatzis and Edgar Weippl, "Merged Mining: Curse or Cure?", CBT 2017 (ESORICS workshops); IACR ePrint 2017/791. Mining pools in merge-mined coins have operated at the edge of, and beyond, the Nakamoto security guarantees for extended periods. The paper also presents a block-attribution scheme for measuring centralisation. — [ePrint](https://eprint.iacr.org/2017/791); [SBA Research](https://sba-research.org/?p=13516)
- **Elastic hash and attacks:** Yoko Shibuya, Go Yamamoto, Fuhito Kojima, Elaine Shi, Shin'ichiro Matsuo and Aron Laszka, "Short Paper: Selfish Mining Attacks Exacerbated by Elastic Hash Supply," Financial Cryptography 2021. They find a significant correlation between profitability and total hashrate. Under elastic supply, selfish mining leads either to chain collapse (all honest miners leave) or to a stable equilibrium, depending on the attacker's initial share. — [Georgetown page](https://people.cs.georgetown.edu/matsuo/publication/fc2021/); [PDF](https://fc21.ifca.ai/papers/233.pdf)

### Inferences
- A merge-mined chain's hashrate is the extreme case of elastic supply: parent-chain pools can add or drop AuxPoW at almost zero cost. The Noda-style elasticity results and the Judmayer findings together imply that the DAA must survive near-instant loss of most hash. The coin should also assume single-pool majority as a realistic threat.

### Gaps
- Aggarwal & Tan's structural analysis of BCH's EDA (SSRN 3383739) could not be retrieved. Its quantitative estimates of EDA oscillations (Aug–Nov 2017) are missing.
- No academic study quantifies DAA recovery on merge-mined altcoins specifically (Namecoin, Syscoin, Dogecoin AuxPoW and similar).

## Q4. Formal results on bias and on timestamp and difficulty-manipulation attacks

### Takeaway
The cleanest formal bias result is Fullmer & Morse's E[X] = β·N/(N−1) for SMA/harmonic-type retargeting, fixed by using N−1. Exponential filters have an analogous discrete bias that NEFDA corrects with a factor e^{T/S}. On attacks: difficulty raising (Bahack), coin-hopping (Meshkov et al.), single-timestamp exploitation of per-block DAAs, and timestamp-based uncle-maker attacks (Hu et al. 2025).

### Cited Findings
- E[X] = β·N/(N−1) and Var = β²·N³/((N−1)²(N−2)), with the Lomax distribution under constant hash. Replacing N with N−1 removes the bias. Bitcoin also has an off-by-one that excludes the last interval from the sum. — [Fullmer & Morse, arXiv 1812.10792](https://arxiv.org/pdf/1812.10792)
- NEFDA/ASERT-type exponential updates need a correction c = e^{T/S} to remove the bias from discrete blocks vs continuous hashrate. — [Ilie et al., ar5iv](https://ar5iv.arxiv.org/html/2006.03044)
- ASERT's integer implementation does not accumulate rounding error (absolute schedule from an anchor block). — [BCHN spec](https://upgradespecs.bitcoincashnode.org/2020-11-15-asert/)
- **Difficulty Raising Attack:** Lear Bahack, "Theoretical Bitcoin Attacks with less than Half of the Computational Power (draft)," arXiv:1312.7013 / IACR ePrint 2013/868, 2013. A secret chain with compressed timestamps raises difficulty to win on total work, putting the practical threshold slightly below 1/4 of hashrate. Theoretical only. — [arXiv](https://arxiv.org/pdf/1312.7013); [2024 SLR summary, arXiv 2411.00349](https://arxiv.org/pdf/2411.00349)
- **Timestamp manipulation (2025):** Junjie Hu, Na Ruan and Sisi Duan, "Timestamp Manipulation: Incentive Attacks on Timestamp-Based Proof-of-Work Blockchains with Minimal Additional Risk," arXiv:2505.05328 (2025, revised 2026). At 25% hash, the UUM and SUUM attacks yield 28.41% and 33.30% profit share vs 26.12% for the earlier RUM attack. Four large pools on Ethereum 1.x, ETC and EthereumPoW showed SUUM-like behaviour, earning about $2.96M extra. A search summary says the paper's mitigations are clamping time deltas and windowed statistics; I did not verify this on the abstract page. — [arXiv](https://arxiv.org/abs/2505.05328)
- Per-block SMA: one false timestamp lowers difficulty for the whole window (144 blocks under cw-144). Per-block exponential: it affects only one block. — [ar5iv 2006.03044](https://ar5iv.arxiv.org/html/2006.03044)
- Under RTT the max-future timestamp must be tightened to "now". — [Harding, Ledger](https://ledgerjournal.org/ojs/ledger/article/download/195/187)
- Practitioners recommend an FTL of about 300 s (vs 2 h in Bitcoin) and a solvetime cap of 7T for fast per-block DAAs. — [zawy12 #62](https://github.com/zawy12/difficulty-algorithms/issues/62)

### Inferences
- For a 72 s chain, a Bitcoin/Dash-style 2 h FTL is about 100 block intervals. Any per-block DAA (LWMA, ASERT, DGW) is then exposed to forward-timestamp difficulty lowering. FTL reduction should go together with the DAA change.
- The project's Rust simulator should measure the bias directly for DGW (N = 24, average-target form, off-by-one in the timespan count) and for LWMA and ASERT, rather than relying on the analytic N/(N−1).

### Gaps
- No formal (peer-reviewed) bias derivation exists for LWMA or for DGW's average-target formulation.
- I did not verify a formal academic analysis of the classic "timewarp" attack on per-block DAAs. Coverage is in practitioner sources and in Bahack's related difficulty-raising attack.
