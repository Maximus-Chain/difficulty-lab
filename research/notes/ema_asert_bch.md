# EMA-family difficulty algorithms (ASERT, WTEMA/EMA) and Bitcoin Cash DAA history (EDA, cw-144, ASERT, Grasberg)

Research date: 2026-10-23. Target context: small AuxPoW (merge-mined) X11 Dash fork, T = 72 s, currently DGW v3 (24-block window), suffering sudden 80-99% hashrate drops and some profit switching. LWMA-1 already evaluated.

Notation used throughout: T = target spacing (s), st = solvetime of previous block = t[h] - t[h-1], tau = e-folding "mean lifetime" in seconds, M (or N) = mean lifetime in blocks = tau/T, half-life = tau * ln 2.

---

## 1. ASERT / aserti3-2d: exact formula, integer math, anchor, half-life, behaviour, timestamp requirements

### Takeaway
ASERT sets target = anchor_target * 2^((t_prev - t_anchor_parent - T*(h_diff+1)) / half_life), computed with a 16-bit fixed-point exponent and a cubic polynomial for 2^frac; it is mathematically identical to a per-block "relative" exponential EMA but immune to accumulated rounding error, has no clipping, uses only the previous block timestamp (not MTP), and relaxes difficulty toward a new hashrate equilibrium exponentially in wall-clock time with time constant tau. BCH uses half-life 172800 s (2 days = 288 blocks, i.e. mean lifetime 415.5 blocks).

### Cited Findings

**Origin / dates**
- ASERT = "absolutely scheduled exponentially rising targets". Lundeberg's paper "Static difficulty adjustments, with absolutely scheduled exponentially rising targets (DA-ASERT) — v.2", dated July 31, 2020; a first version circulated November 2019. It credits Jacob Eliosoff ("simpexp") with first suggesting the relative exponential form. — [Lundeberg, DA-ASERT v2 PDF](http://toom.im/files/da-asert.pdf)
- Independently discovered by Jacob Eliosoff (2018), Mark Lundeberg, and an Imperial College group (Werner et al., arXiv 2006.03044 "Unstable Throughput: When the Difficulty Algorithm Breaks"). — [zawy12 issue #61](https://github.com/zawy12/difficulty-algorithms/issues/61); [Toomim, read.cash, 8 July 2020](https://read.cash/@jtoomim/bch-upgrade-proposal-use-asert-as-the-new-daa-1d875696)
- Jonathan Toomim published the BCH proposal "Use ASERT as the new DAA" on 8 July 2020; activated in BCH's 15 Nov 2020 upgrade ("Axion") as aserti3-2d. Reference implementation: BCHN merge request 692. — [Toomim](https://read.cash/@jtoomim/bch-upgrade-proposal-use-asert-as-the-new-daa-1d875696); [BCHN upgrade spec](https://upgradespecs.bitcoincashnode.org/2020-11-15-asert/); [BCHN MR 692](https://gitlab.com/bitcoin-cash-node/bitcoin-cash-node/-/merge_requests/692)

**Math (ideal forms, Lundeberg)**
- Relative form: `target[N+1] = target[N] * exp((t[N] - t[N-1] - T)/tau)`; tau is a relaxation time, "practically could be anywhere from (say) 50T to 1000T". — [Lundeberg PDF](http://toom.im/files/da-asert.pdf)
- Absolute form (equivalent): `target[N+1] = target_ref * exp((t[N] - t_ref - (N - h_ref)*T)/tau)`. All constants collapse into one: `target[N+1] = C * exp((t[N] - N*T)/tau)`. — [Lundeberg PDF](http://toom.im/files/da-asert.pdf)
- Anchor choice: pick fixed height M, set target_ref = target_M, h_ref = M-1, t_ref = t_{M-1}; this gives exact continuity as if the relative form had been activated at M. For a new coin: t_ref = genesis time, h_ref = 0, target_ref = educated initial guess. — [Lundeberg PDF](http://toom.im/files/da-asert.pdf)
- Footnote warning: using a moving reference M = N - n (n>0) is NOT equivalent and "creates a very badly behaving algorithm ... oscillations with a period of n+1". Either M = N or M = constant. — [Lundeberg PDF](http://toom.im/files/da-asert.pdf)
- Property 1.1: pushing the schedule ahead by +tau must raise difficulty by x e "without exception" → intrinsic protection against sustained rapid mining. Property 1.2: memoryless / reversible: if difficulty rises and falls by any factor, the algorithm returns to exactly the same state. — [Lundeberg PDF](http://toom.im/files/da-asert.pdf)
- Drift: if difficulty increases by factor X, drift (blocks ahead of schedule, in time) increases by exactly `Δdrift = +tau * ln(X)`. Longer tau → more drift. — [Lundeberg PDF](http://toom.im/files/da-asert.pdf); zawy phrases it as excess_blocks_emitted = mean_lifetime_in_blocks * ln(current_D/start_D) — [zawy12 #61](https://github.com/zawy12/difficulty-algorithms/issues/61)
- Clipping: "quite dangerous to artificially restrict the downward movement of target (upward movement of difficulty)" (timespan-limit-style pumping exploits). With clipping, relative and absolute forms are no longer equivalent; absolute form with clipping can "stick to the limit for a too-long time, and overshoot", inducing stalls, bursts, oscillations. He recommends against any clipping; also recommends a timestamp monotonicity rule as a strong defence against unknown exploits. — [Lundeberg PDF](http://toom.im/files/da-asert.pdf)
- Hitting powLimit is a form of clipping; the absolute form overshoots there, the relative form behaves more reasonably. — [Lundeberg PDF](http://toom.im/files/da-asert.pdf)
- Relative-form errors: if miners can induce a consistent multiplicative error (1+ε) per block, they can mine at average time (1 - ε*tau/T)*T; e.g. ε=1e-4, tau/T=1e3 → 10% advantage. Absolute form forgets errors on the next block, so a coarse exp approximation is fine. — [Lundeberg PDF](http://toom.im/files/da-asert.pdf)
- Real-time variant: involving t[N+1] makes difficulty fall while waiting; this converts the absolute form into one independent of previous block info. — [Lundeberg PDF](http://toom.im/files/da-asert.pdf)

**Exact BCH/ABC integer implementation (aserti3-2d)** — from [Bitcoin ABC src/pow/aserti32d.cpp](https://github.com/Bitcoin-ABC/bitcoin-abc/blob/master/src/pow/aserti32d.cpp) (same algorithm as BCHN):
```cpp
// anchorTime = timestamp of PARENT of anchor block (anchor itself iff genesis)
nTimeDiff   = pindexPrev->GetBlockTime() - anchorTime;      // prev block's own timestamp, NOT MTP
nHeightDiff = pindexPrev->nHeight - pindexAnchorBlock->nHeight;
refTarget   = SetCompact(pindexAnchorBlock->nBits);
// assert(llabs(nTimeDiff - T*nHeightDiff) < 2^47)
exponent = ((nTimeDiff - T*(nHeightDiff + 1)) * 65536) / nHalfLife;   // C++ truncating division
shifts   = exponent >> 16;            // arithmetic shift = floor
frac     = uint16_t(exponent);        // exponent == shifts*65536 + frac
// 2^x ~= 1 + 0.695502049x + 0.2262698x^2 + 0.0782318x^3, 0<=x<1, error < 0.013%
factor = 65536 + ((195766423245049ull*frac + 971821376ull*frac*frac
                   + 5127ull*frac*frac*frac + (1ull<<47)) >> 48);
nextTarget = refTarget * factor;      // < 2^241 since refTarget < 2^224 (powLimit has >=32 leading zero bits)
shifts -= 16;
if (shifts <= 0) nextTarget >>= -shifts;
else { shifted = nextTarget << shifts; nextTarget = (shifted>>shifts != nextTarget) ? powLimit : shifted; }
if (nextTarget == 0) nextTarget = 1; else if (nextTarget > powLimit) nextTarget = powLimit;
```
- Testnet rule in the same function: if `pblock time > prev time + 2*T` return powLimit (min difficulty). — [ABC aserti32d.cpp](https://github.com/Bitcoin-ABC/bitcoin-abc/blob/master/src/pow/aserti32d.cpp)
- Spec constants: ideal block time 600 s; half-life 172800 s (2 days); radix 65536; max target nBits 0x1d00ffff; testnet half-life 3600 s and min-difficulty if >1200 s after parent. Activation by MTP; anchor = first block whose MTP ≥ activation time (the last pre-fork block's parent timestamp is used); anchor may be hard-coded once well confirmed. 16-bit radix chosen to match nBits' ~15-bit mantissa precision; cubic chosen for monotonicity and low error. Test vectors in BCHN qa-assets (test_vectors/aserti3-2d). — [BCHN spec](https://upgradespecs.bitcoincashnode.org/2020-11-15-asert/)
- Lundeberg's 2^x approximation error range is -0.010% to +0.013%; competing option was Eliosoff's `exp_int_approx()` (Padé-style + repeated squaring loop), rejected as more opaque. Floating point excluded because C/C++ are not guaranteed IEEE-754 compliant. — [Toomim](https://read.cash/@jtoomim/bch-upgrade-proposal-use-asert-as-the-new-daa-1d875696); [Eliosoff comment in zawy12 #62](https://github.com/zawy12/difficulty-algorithms/issues/62#issuecomment-646866050)
- Lundeberg's own integer recipe: `a = (b*[t_N - t_ref - (N-h_ref)T] + 2^15) / 2^16`, `b = round(2^16 * ln2 * T/tau)`... target = [target_ref * 2^(a/2^16 - 16)] * f(a mod 2^16); 3rd-order polynomial gives ~0.01%. — [Lundeberg PDF](http://toom.im/files/da-asert.pdf)

**Half-life choice (BCH)**
- Toomim chose tau_half = 2 days ("aserti3-415.5" in mean-lifetime blocks = 2*144/ln2) as within the near-optimal simulated range and easy to explain ("for every 2 days ahead of schedule ... difficulty doubles"); 1 day (aserti3-207.75) also acceptable; "I recommend against going lower than 207.75 or higher than 576". — [Toomim](https://read.cash/@jtoomim/bch-upgrade-proposal-use-asert-as-the-new-daa-1d875696)
- Toomim's sweep (BCH/BTC profit-switching sim, June 18 2020): confirmation time minimised at asert-407 (677.11 s) and steady-miner loss minimised at asert-484 (-0.307%); asert-144: 815.30 s / -0.820%; asert-2304: 907.92 s / -0.724%. — [zawy12 #62 comment 646312380](https://github.com/zawy12/difficulty-algorithms/issues/62#issuecomment-646312380)
- zawy12 (Summary, issue #50): "I can't find an error in Jonathan Toomim's work for BCH that indicates N=288 for ASERT/EMA is best (... like N=576 for LWMA) and this is with T=600. With lower T's ... even a higher N might be best", but he personally prefers ~half (ASERT/EMA M=144, LWMA N=288), and "coins seeing large daily changes in price or on-off miners ... might be better off with 1/2 these values". Fear of a coin getting stuck. — [zawy12 #50](https://github.com/zawy12/difficulty-algorithms/issues/50)
- Std dev of difficulty under constant hashrate for EMA/ASERT ≈ 1/sqrt(2N) (N = mean lifetime in blocks). ASERT N=100: rises 5% within ~6 blocks after a 2x hashrate increase; N=72 has the same D variation as SMA-144. zawy advocated 100-150 for BCH. — [zawy12 #62 body](https://github.com/zawy12/difficulty-algorithms/issues/62)

**Timestamp / MTP / FTL requirements**
- ASERT uses only the previous block's timestamp, no median-of-3 prefilter. Toomim & zawy "strongly advise against the use of median prefilters": a 1-block delay creates a 2-block-period instability in EMAs. — [Toomim](https://read.cash/@jtoomim/bch-upgrade-proposal-use-asert-as-the-new-daa-1d875696)
- Known residual attacks: (a) selfish miner sets solvetime 1 s on block A1 → A2 difficulty +0.24% → can win a 2-block race on chainwork; mitigation: first-seen hysteresis (switch only if new chain work exceeds by e.g. 10-50% of a block). (b) FTL attack: stamp a block at the FTL (7200 s on BCH) to lower difficulty on a secret chain; slightly more effective for ASERT than cw-144 because ASERT responds after one block; mitigated by slow tau and "better addressed by simply lowering the FTL" to ≤600 s, adjusting the peer-time revert rule (DEFAULT_MAX_TIME_ADJUSTMENT) accordingly. — [Toomim](https://read.cash/@jtoomim/bch-upgrade-proposal-use-asert-as-the-new-daa-1d875696)
- zawy's checklist for EMA/ASERT on BCH: no 1/2 timespan limit, nothing limiting how fast D rises; FTL 300 s (or ≤ N*T/40); peer-time revert = FTL/2 or removed; no median of 3; cap solvetime at 7xT (anti-spam: 10% miner forwarding 333xT would otherwise drop an N=72 EMA to 1% of D in one block; with 7xT clipping it needs 55 blocks, ~12x costlier). Never use `if (st < 0) st = 0`. — [zawy12 #62](https://github.com/zawy12/difficulty-algorithms/issues/62); [zawy12 #49](https://github.com/zawy12/difficulty-algorithms/issues/49)
- Note tension: Lundeberg recommends no clipping at all on ASERT; zawy suggests an upper solvetime cap (≈ max difficulty drop) as anti-spam. Both agree: never clip difficulty increases. — [Lundeberg PDF](http://toom.im/files/da-asert.pdf); [zawy12 #49](https://github.com/zawy12/difficulty-algorithms/issues/49)
- Private-mine timestamp attack: ASERT can emit up to 38% more blocks in a long private mining attack (first timestamp forwarded by ~2x mean lifetime, then 0/1 s stamps); EMA ~33-34%, SMA 50%, LWMA showed no benefit. — [zawy12 #61](https://github.com/zawy12/difficulty-algorithms/issues/61); [zawy12 #50](https://github.com/zawy12/difficulty-algorithms/issues/50)
- Absolute ASERT handles negative solvetimes gracefully (only makes difficulty harder); target approaches 0 asymptotically rather than crossing it. — [dgenr8 comment, zawy12 #62](https://github.com/zawy12/difficulty-algorithms/issues/62#issuecomment-648958314)
- Testnet consequence: time-based ASERT defeats the "timewarp" trick testnets used to reset difficulty, so testnet may be stuck with 20-min blocks for days after an ASIC leaves (hence BCH testnet half-life 1 h). — [Toomim](https://read.cash/@jtoomim/bch-upgrade-proposal-use-asert-as-the-new-daa-1d875696); [BCHN spec](https://upgradespecs.bitcoincashnode.org/2020-11-15-asert/)

### Inferences
- **Analytic response to a sudden drop (derived from the absolute formula, assuming expected solvetimes):** let r = remaining hashrate fraction and u = difficulty relative to pre-drop. In continuous approximation `du/dt = -(u - r)/tau`, so `u(t) = r + (1 - r)*e^(-t/tau)` with t in wall-clock seconds and tau = half_life/ln2. Expected block interval ≈ T*u/r. Time until blocks are back to 2T: solve u = 2r → `t = tau * ln((1-r)/r)`. For an 80% drop (r=0.2): t ≈ 1.39 tau; for 99% (r=0.01): t ≈ 4.6 tau. Per-block discretisation (difficulty frozen until next block in non-RTT ASERT) means the very first block after a 99% drop expects ~100T (7200 s at T=72), after which target jumps by exp((st-T)/tau).
- **Mapping BCH's 2-day half-life to T=72 s:** if kept in *blocks* (288-block half-life, mean lifetime 415.5 blocks) → half-life 5.76 h, tau ≈ 8.3 h → recovery to 2T after a 99% drop ≈ 38 h, after 80% ≈ 11.5 h. If kept in *wall time* (2 days = 2400 blocks) → recovery to 2T after 99% drop ≈ 13.3 days: unacceptable for this chain. Toomim/zawy optimisations are expressed in blocks (stability ~1/sqrt(2M) is per-block noise), so block-based scaling is the relevant one; zawy explicitly suggests smaller M for chains with large on-off miners.
- Rough candidate grid for simulation at T=72: mean lifetime M ∈ {36, 50, 72, 100, 144, 208} blocks (half-life = M*72*ln2 s → 1797 s ... 10,380 s). Using zawy's equivalence (ASERT M ≈ LWMA N/2, see Q2), LWMA-1 N=60..90 corresponds to ASERT M≈30..45; DGW-24 is roughly SMA-24-like, i.e. ASERT M≈12 in noise terms. M around 50-100 trades 7-10% per-block noise for a recovery time-to-2T after 99% drop of ~4.6*M*72 s ≈ 4.6-9.2 h.
- Because ASERT's hard 99%-drop recovery is bounded by tau*ln(1/r) and cannot be accelerated without lowering tau (more noise), a chain with 80-99% drops likely needs an additional mechanism (RTT layer, or testnet-style emergency rule) on top of ASERT; this is a design decision to evaluate by simulation, not a sourced recommendation.
- Dash-derived nodes typically keep BTC's 2 h FTL and 70 min peer-time revert; verify in the target codebase. With T=72 s, a 7200 s FTL equals 100 blocks of schedule shift, so the FTL attack is relatively far stronger than on BCH (where it is 12 blocks); lowering FTL (e.g. to ≤ a few T, zawy's ≤ N*T/40 rule) should accompany any ASERT/EMA switch.
- AuxPoW-specific: pools that do not refresh the child timestamp produce same/out-of-sequence stamps (zawy #35); ASERT tolerates negative solvetimes, WTEMA needs guards.

### Gaps
- BCHN source file (pow.cpp in BCHN GitLab) not fetched directly; ABC's aserti32d.cpp is used as equivalent. The spec page was only partially readable (summary), so Python reference code was not copied verbatim.
- No primary source found quantifying ASERT behaviour specifically for 80-99% hashrate drops at small T; the drop-response formula above is my derivation.
- Whether BCHN ultimately lowered FTL after ASERT was not verified.

---

## 2. WTEMA / EMA (zawy12, Eliosoff, Harding): formulas, equivalence to ASERT, N relationship to LWMA, pros/cons, users

### Takeaway
WTEMA is the first-order Taylor approximation of relative ASERT: `target[h+1] = target[h] * (1 + st/(T*N) - 1/N)`, implemented as `next = prev / (T*N) * (st + T*(N-1))`. It performs essentially identically to ASERT in simulation, needs only two headers, but has a singularity (target → 0/negative) for large negative solvetimes and accumulates nBits/rounding error linearly with N. For equal stability under constant hashrate, EMA/ASERT M ≈ LWMA N / 2.

### Cited Findings
- WTEMA (Tom Harding / dgenr8, on or before 1 Jan 2018, kyuupichan/difficulty PR #30) pseudocode: `next_target = prior_target / (IDEAL_BLOCK_TIME * alpha_recip); next_target *= block_time + IDEAL_BLOCK_TIME * (alpha_recip - 1)`. Described as a first-order IIR filter equivalent to an RC filter. — [Toomim](https://read.cash/@jtoomim/bch-upgrade-proposal-use-asert-as-the-new-daa-1d875696); [kyuupichan/difficulty PR #30](https://github.com/kyuupichan/difficulty/pull/30)
- Lundeberg derivation: with exp(z) ≈ 1+z, `target[N+1] = target[N]*(1 - α + α x)`, x = st/T, α = T/tau. Target passes through 0 (difficulty singularity) when x = 1 - 1/α, i.e. st = -(N-1)T: "requires a large backwards timestamp jump but is disastrous". WTEMA difficulty "is never easier than the exponential one", inherits rapid-mining safety; WTEMA shows "soft clipping" of extreme drops. "If one wants an algorithm that doesn't require calculating exponentials, then WTEMA is a fantastic choice (supposing the singularity is inaccessible due to timestamp ordering rules)." — [Lundeberg PDF](http://toom.im/files/da-asert.pdf)
- With large α^-1, mining variance makes WTEMA's effective T noticeably longer than nominal T. — [Lundeberg PDF footnote 9](http://toom.im/files/da-asert.pdf)
- zawy's forms: `target = prev_target * (1 + st/T/N - 1/N)` (wtema) vs `prev_target * e^(st/T/N - 1/N)` (relative ASERT); "wtema-144 and asert-144 refer to N = 144/ln(2) = 208" (i.e. half-life naming). — [zawy12 #62](https://github.com/zawy12/difficulty-algorithms/issues/62). NOTE naming conflict: Toomim's tables name by mean lifetime ("aserti3-415.5" = 2-day half-life). Check which convention each table uses.
- Simple EMA for difficulty (Cryptonote code, 2018): `next_D = (D*N*T)/(N*T - T + ST)` with ST clamped to [-5T, 6T], T multiplied by 0.982 correction, N=28 "same speed as LWMA N=60"; "math by Jacob Eliosoff and Tom Harding". Equivalent Digishield rearrangement `next_D = avg(17 D)*4/(3 + avg(17 ST)/T)` vs EMA `next_D = prev_D * 36/(35 + prev_ST/T)`. — [zawy12 #21](https://github.com/zawy12/difficulty-algorithms/issues/21)
- Equal-stability parameter mapping (same D std dev at constant hashrate): SMA/LWMA = 100, WT = 133, ASERT/EMA = 50, ETH = 64, Digishield = 25. — [zawy12 #50](https://github.com/zawy12/difficulty-algorithms/issues/50). Also "Most LWMAs are N=60 which has the stability of EMA with N=30"; BTG LWMA N=45 ≈ ASERT N=22.5. — [zawy12 #62](https://github.com/zawy12/difficulty-algorithms/issues/62). In the June 2020 BCH tests: SMA-144, EMA-72, ASERT-72, LWMA-144, WT-190 at equal std dev. — [zawy12 #62 comment](https://github.com/zawy12/difficulty-algorithms/issues/62#issuecomment-645735330)
- Response asymmetry: ASERT's expected max rate of change is a factor e in M blocks; "LWMA rises about 50% faster and falls about 50% more slowly". — [zawy12 #50](https://github.com/zawy12/difficulty-algorithms/issues/50)
- Errors: relative ASERT/EMA error up to N*2^-15 from nBits rounding (≈1% avg solvetime per +330 in N in worst nBits range); absolute ASERT removes it. zawy suggests N=100-200 for EMA. — [zawy12 #61](https://github.com/zawy12/difficulty-algorithms/issues/61); [zawy12 #50](https://github.com/zawy12/difficulty-algorithms/issues/50)
- Implementation hazards for EMA: prevent out-of-sequence timestamps or use the wt-144/LWMA-style look-back over 11 blocks; never `if st<0 st=0`; use most recent solvetime and previous target (a 1-block delay causes 2-block oscillation); pool timestamp staleness degrades performance; Cryptonote coins can't use it due to a 1-block timestamp delay. — [zawy12 #49](https://github.com/zawy12/difficulty-algorithms/issues/49); [zawy12 #50](https://github.com/zawy12/difficulty-algorithms/issues/50)
- Debate summary (WTEMA vs ASERT for BCH): integer approximation easier for WTEMA; singularity/negative solvetimes problematic for WTEMA (would need e.g. st > -0.5*144*600 for wtema-144); elegance and rounding-error accumulation favour ASERT. BCH chose ASERT to avoid changing other consensus rules to forbid negative solvetimes. — [Toomim](https://read.cash/@jtoomim/bch-upgrade-proposal-use-asert-as-the-new-daa-1d875696)
- dgenr8: ASERT superior for abnormally long blocktimes (nonlinear, drops faster) and negative blocktimes (asymptotic). — [zawy12 #62 comment 648958314](https://github.com/zawy12/difficulty-algorithms/issues/62#issuecomment-648958314)
- ETH's Homestead DA is an EMA variant with integer truncation (hysteresis), ranked below EMA/ASERT/LWMA. — [zawy12 #50](https://github.com/zawy12/difficulty-algorithms/issues/50); [zawy12 #45](https://github.com/zawy12/difficulty-algorithms/issues/45)
- zawy ranking: TSA (RTT) > {CDF-EMA, EMA, ASERT, LWMA, WT} (subtle differences) > ETH > Digishield/Grin > KGW/BRNDF > Digishield with MTP delay > SMA > Cryptonote > BTC/LTC. — [zawy12 #50](https://github.com/zawy12/difficulty-algorithms/issues/50)

### Inferences
- For a Rust simulator: implement WTEMA as `next = prev * (T*(N-1) + st) / (T*N)` on 256-bit targets with st clamped below at something > -(N-1)T (or enforce monotonic timestamps), and ASERT with the exact ABC integer code above; at M ≥ 20 they should be nearly indistinguishable except on extreme solvetimes, which is exactly the regime (99% drops → 100T solvetimes) this chain hits. ASERT drops harder after a very long block (exp vs linear), favouring ASERT for this chain.
- Since LWMA-1 was already evaluated with some N, the like-for-like ASERT/EMA comparison point is M = N_LWMA/2.

### Gaps
- No list of production altcoins currently running plain WTEMA was found in this session (zawy mentions some Cryptonote EMA attempts, but no confirmed live deployments).

---

## 3. BCH EDA (Aug 2017), cw-144 (Nov 2017), its oscillations, and the 2020 Grasberg vs ASERT debate

### Takeaway
EDA (1 Aug 2017) cut difficulty 20% whenever the MTP span over 6 blocks was ≥ 12 h, on top of BTC's 2016-block retarget; miners gamed it, producing violent boom/bust cycles. cw-144 (13 Nov 2017) replaced it with a chainwork-based 144-block SMA with median-of-3 endpoints and [0.5x, 2x] timespan clamps, which produced ~1-day-period oscillations driven by BTC/BCH profit switching (up to 4-8x daily hashrate swings in 2019-2020, ~6-11% losses for steady miners). In 2020 Bitcoin ABC pushed Grasberg (relative ASERT + drift correction targeting slower blocks), while BCHN/BU/others backed aserti3-2d, which activated 15 Nov 2020.

### Cited Findings
**EDA**
- Spec REQ-7: "In case the MTP of the tip of the chain is 12h or more after the MTP 6 block before the tip, the proof of work target is increased by a quarter, or 25%, which corresponds to a difficulty reduction of 20%." — [UAHF technical spec](https://github.com/bitcoincashorg/bitcoincash.org/blob/master/spec/uahf-technical-spec.md)
- Exploitation: coordinated/timed mining can bring difficulty down ~75% within a day; once low, profit-maximising miners pile in and mine an enormous number of blocks before difficulty readjusts. — [Bitcoin Magazine](https://bitcoinmagazine.com/articles/miners-are-milking-bcashs-difficulty-adjustments-and-why-problem1/)
- 21 Aug 2017: BCH producing ~25 blocks/hour (avg 2 min 11 s) vs normal 6/hour. — [BitDigest, 21 Aug 2017](https://bitdigest.substack.com/p/bitdigest-monday-august-21-2017-70368)

**cw-144 (activated when MTP-11 ≥ 1510600000, 13 Nov 2017)** — [Nov 13 hardfork spec](https://github.com/bitcoincashorg/bitcoincash.org/blob/master/spec/nov-13-hardfork-spec.md)
- B_last = median-by-timestamp of [B_n-2, B_n-1, B_n]; B_first = median of [B_n-146, B_n-145, B_n-144].
- TS = time(B_last) - time(B_first), clamped to [72*600, 288*600].
- W = chainwork(B_last) - chainwork(B_first); PW = W*600/TS; target = (2^256 - PW)/PW, capped at powLimit. Integer arithmetic only.
- The median-of-3 sort adds ~1 block of delay; Toomim: this delay "tends to cause oscillation issues". — [Toomim](https://read.cash/@jtoomim/bch-upgrade-proposal-use-asert-as-the-new-daa-1d875696)

**cw-144 oscillation data (Toomim, 8 July 2020)** — [Toomim](https://read.cash/@jtoomim/bch-upgrade-proposal-use-asert-as-the-new-daa-1d875696)
- Oscillation period ~1.05 days (invisible on daily charts). Nov 2017 amplitude ~1 EH/s, daily peak ~2x minimum; from ~1 Oct 2019 amplitude 11 EH/s, peaks 4x-8x the daily minimum (also Jan 20 2020, April 2020).
- Difficulty oscillations frequently >10%; switch miners mine ~2-6 h/day capturing profitable blocks; dedicated miners mine at a loss vs BTC most of the time.
- Confirmation times ~15 min average since Sep 2019, peak 28 min in Jan 2020. Rational behaviour by 90% of hashrate → ~2x confirmation times; 100% rational → chain stalls.
- Switch miners can earn 5-20% more than constant-HR miners; EMA family reduces the incentive ~90-95%. Simulations (kyuupichan's suite, Sep-Oct 2017, extended by Toomim Feb 2020 into a web UI — [jtoomim/difficulty comparator branch](https://github.com/jtoomim/difficulty/tree/comparator); [kyuupichan/difficulty](https://github.com/kyuupichan/difficulty)) reproduce the oscillations with purely profit-following miners, with no malice.
- Also: Toomim's Feb 27, 2020 video and BitMEX Research article on the Oct 2019 oscillations are referenced in the post.

**Grasberg (Bitcoin ABC)**
- Announced 23 July 2020 by Amaury Séchet; Toomim called it "a big step on the path to corruption" and "not properly simulated". A consortium (BU, BCHN, BCHD, Electron Cash, Flowee, Toomim, others) issued a joint statement 5 Aug 2020 committing to aserti3-2d on 15 Nov 2020. — [Bitcoin.com News, 5 Aug 2020](https://news.bitcoin.com/bitcoin-cash-difficulty-algorithm-debate-heats-up-with-fears-of-another-chain-split/); [General Protocols joint statement](https://medium.com/@GeneralProtocols/joint-statement-on-aserti3-2d-algorithm-c5193ca50696)
- ABC said no concrete alternative reached it before the 15 Aug feature freeze; goals: consistent block production and predictable emission schedule. — [ABC "Announcing the Grasberg DAA"](https://www.publish0x.com/bitcoin-abc/announcing-the-grasberg-daa-xkkrkyn)
- Grasberg code (ABC src/pow/grasberg.cpp): *relative* ASERT with mean lifetime 288 blocks (`tau32 = 288*T*ln2*2^32`, exponent base 2 → e^(offset/(288T))), reference = previous block, with a 32-bit deterministic exp2 (4 buckets × quartic polynomial). Drift correction: `drift = (h*T + genesis_time) - last_block_time`; `x = drift/nPowTargetTimespan` (2^32 fixed point) clipped to ±ln2(675/600); target block time = T*2^x → blocks targeted between ~533 s and 675 s until the chain returns to the genesis-based schedule. — [ABC grasberg.cpp](https://github.com/Bitcoin-ABC/bitcoin-abc/blob/master/src/pow/grasberg.cpp); [ABC doxygen](https://www.bitcoinabc.org/doc/dev/grasberg_8cpp_source.html)

**Post-activation**
- BCH research thread "ASERT: Before and After" (freetrader, Feb-May 2021) compiled before/after data (DAA era blocks 504031-661647, ASERT era 661648+); figures are images; a May 2021 post reports "a bit of improvement" in the current/median difficulty ratio; confirmation-time data not available. — [bitcoincashresearch.org](https://bitcoincashresearch.org/t/asert-before-and-after/312)
- April 2024 halving discussion: hashrate appeared to leave ~10 blocks before the halving; with 2-day half-life the expected disruption was judged "more than a day, maybe two"; no consensus change made. — [bitcoincashresearch.org](https://bitcoincashresearch.org/t/strategies-to-lessen-the-impact-of-the-halving-on-blocktimes/1281)

### Inferences
- EDA is the cautionary example for any "emergency" rule a small chain might add for 99% drops: a one-sided, threshold-based difficulty cut with no symmetric fast rise is directly exploitable by switch miners. Any emergency rule should be symmetric/continuous (e.g. ASERT's natural response, or an RTT that also penalises fast blocks).
- cw-144's problems (SMA resonance + median-of-3 delay + timespan clamps) are structurally shared by DGW (SMA-like, 3x/1/3 timespan limits, see zawy #31), so the cw-144 → ASERT gains are a reasonable prior for DGW → ASERT, scaled to the chain's block count.

### Gaps
- Exact outcome of the Nov 2020 split (ABC → eCash/XEC, and the role of the Infrastructure Funding Plan vs Grasberg) was not verified from a primary source in this session; ABC's current code uses ASERT anchored at `axionHeight` (see Q5), implying ABC abandoned Grasberg for consensus.
- Precise EDA C++ code and the exact date of the most extreme EDA oscillations were not retrieved.

---

## 4. Published simulation results: ASERT/EMA vs LWMA vs Digishield/SMA

### Takeaway
In both Toomim's and zawy12's simulations, once parameters are matched for equal noise, ASERT, WTEMA, LWMA and WT perform within a few percent of each other; SMA/cw-144 and Digishield are clearly worse. For BCH the optimum responsiveness was ASERT mean lifetime ~400-500 blocks (but zawy prefers about half).

### Cited Findings
- Toomim, 18 June 2020 (6x on-off switching model, BCH vs BTC): confirmation time (s) / steady-miner profit vs BTC:
  - asert-144 753.62 / -0.699%; wtema-144 757.14 / -0.693%; lwma-144 755.05 / -0.720%; wt-144 761.64 / -0.721%
  - asert-288 672.23 / -0.333%; wtema-288 673.50 / -0.333%; lwma-288 673.10 / -0.348%; wt-288 676.09 / -0.348%
  - asert-576 667.28 / -0.290%; wtema-576 661.51 / -0.286%; lwma-576 671.99 / -0.305%; wt-576 673.36 / -0.305%
  - cw-144/2 (≈ lwma-072 impulse response) 1565.14 / -10.899% (greedy switchers +1.245%)
  — [zawy12 #62 comment 646159947](https://github.com/zawy12/difficulty-algorithms/issues/62#issuecomment-646159947)
- Summary: best (asert-576) needed 51% of cw-144's confirmation time and cut switch-mining incentive 94.4%; all non-cw algorithms clustered closely; steady-miner losses ~0.35-0.65% for EMA family vs ~6% for cw-144 in his test. — [Toomim](https://read.cash/@jtoomim/bch-upgrade-proposal-use-asert-as-the-new-daa-1d875696)
- ASERT sweep (same date): asert-407 best confirmation time (677.11 s), asert-484 best steady profit (-0.307%). — [zawy12 #62 comment 646312380](https://github.com/zawy12/difficulty-algorithms/issues/62#issuecomment-646312380)
- zawy methodology: match N for equal D std dev at constant hashrate, then simulate on-off mining triggered by difficulty thresholds; metrics "% delayed" (sum of (st - 4T) for st>4T / blocks) and "% stolen" (time-weighted target seen by on-off miner vs dedicated). Confirmation delay should be computed as sum(st*(st+1)/2)/sum(st) (Toomim's suggestion). Results: SMA-144 worst; EMA-72, ASERT-72, LWMA-144, WT-190 "pretty much the same"; WT slightly fewer stolen, slightly more delays. Code: test_DAs.cpp in zawy's repo. — [zawy12 #62](https://github.com/zawy12/difficulty-algorithms/issues/62); [comment 645735330](https://github.com/zawy12/difficulty-algorithms/issues/62#issuecomment-645735330); [zawy12 #50](https://github.com/zawy12/difficulty-algorithms/issues/50)
- EMA vs Digishield: simple EMA "has half as many delays and blocks lost to big miners as Digishield". — [zawy12 #21](https://github.com/zawy12/difficulty-algorithms/issues/21)
- DGW critique: its loop is an SMA giving double weight to the newest target; uses one fewer timestamp (1/24 longer avg solvetime at N=24) plus ~1.5% Erlang bias → ~6% longer solvetime; 3x and 1/3 timespan limits enable the unlimited-blocks timestamp attack if timestamps are not sequential. — [zawy12 #31](https://github.com/zawy12/difficulty-algorithms/issues/31)
- Merge-mining problems noted by zawy: disinterested big miners, lazy pool code reusing parent timestamps → same or out-of-sequence child timestamps. — [zawy12 #35](https://github.com/zawy12/difficulty-algorithms/issues/35)

### Inferences
- The published simulations model *profit switching* (oscillatory on-off around a difficulty threshold), not a permanent 80-99% departure of a merge-mining pool. For the latter, the dominating factor is tau (wall-clock recovery ≈ tau*ln((1-r)/r)), not the algorithm family; LWMA's ~50% faster rise / slower fall vs ASERT means ASERT recovers faster from drops at equal noise.
- Toomim's optimum (~400-500 mean-lifetime blocks) was for a large chain with 600 s blocks where switching miners compare against BTC; it should not be copied blindly to a 72 s chain with a single dominant merge-mining pool.

### Gaps
- Kyuupichan's original 2017 simulation results (per-algorithm tables) were not retrieved; only referenced via Toomim.
- No published sim specifically for AuxPoW chains with a single pool providing >80% hashrate was found.

---

## 5. Other chains using ASERT/EMA; incidents

### Takeaway
eCash (XEC, Bitcoin ABC) still runs aserti3-2d anchored at the Nov 2020 "axion" height and since 15 Nov 2024 layers an Avalanche-enforced Real-Time-Targeting (RTT) policy on top to punish switch-miner "turbo blocks". Evidence for other chains (Nexa, Kaspa etc.) was not found in this session.

### Cited Findings
- Bitcoin ABC (eCash node) computes ASERT with `pindexPrev->GetAncestor(params.axionHeight)` as anchor. — [ABC aserti32d.cpp](https://github.com/Bitcoin-ABC/bitcoin-abc/blob/master/src/pow/aserti32d.cpp)
- eCash "Heartbeat" upgrade (activated 15 Nov 2024): RTT layer "on top of the current DAA", enforced via Avalanche post-consensus polling, not by changing base consensus rules; rationale: switch miners mine bursts of "turbo blocks" and leave, causing hour-long delays. Inspired by Tom Harding's "Real-Time Block Rate Targeting" (Ledger). — [e.cash blog](https://e.cash/blog/heartbeat)
- RTT code (policy, `policy-bad-rtt`): `target(t) = target(prev) * K*Γ(1+1/K)^K / T_w^(K-1) * t^(K-1)`, K = 6, using header *received* times (not timestamps), over windows of 1, 2, 5, 11, 17 blocks with T_w = 150, 600, 2400, 6000, 9600 s respectively; the lowest target (highest difficulty) across windows applies. Windows chosen as primes to avoid resonance. — [ABC src/policy/block/rtt.cpp](https://github.com/Bitcoin-ABC/bitcoin-abc/blob/master/src/policy/block/rtt.cpp)
- eCash self-reported block-time std dev fell from 919.01 s to 665.34 s after Heartbeat (promotional source). — [e.cash blog, 12 Dec 2024](https://e.cash/blog/heartbeat-upgrade-results)
- A BCH fork "BCH2" whitepaper states it uses aserti3-2d as in BCHN v29.0.0 with a modified half-life for launch security (low-quality/obscure source). — [BitcoincashII/Whitepaper](https://github.com/BitcoincashII/Whitepaper)
- zawy: ~60 coins use LWMA; none got permanently stuck; ~5% couldn't eliminate oscillations; LWMA/EMA/ASERT nearly indistinguishable on identical random seeds. Komodo sub-chains with 1,000,000x hashrate swings used a TSA/RTT-style scheme that stopped attackers getting more than 2 cheap blocks and prevented stuck chains. — [zawy12 #62](https://github.com/zawy12/difficulty-algorithms/issues/62); [zawy12 #50](https://github.com/zawy12/difficulty-algorithms/issues/50)

### Inferences
- The eCash RTT design is directly relevant to the "sudden drop + switching" profile: RTT punishes fast blocks after difficulty overshoots, and (in Lundeberg's/zawy's RTT forms) can lower difficulty during long waits, addressing the 99%-drop stall that pure ASERT can only fix over ~tau*ln(1/r). Since this chain lacks Avalanche, an RTT would have to be consensus-level and requires a tight FTL.
- A search hit claiming an "ecash-com/bitcoin" PR adding ASERT as opt-in is unrelated to Bitcoin ABC's eCash and should be disregarded.

### Gaps
- Nexa, Kaspa, Dogecash and other chains: no reliable confirmation of ASERT/EMA use found; not reported.
- No documented incident of an ASERT chain getting stuck after a 90%+ hashrate exit was found.
