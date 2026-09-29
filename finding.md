# My Chain — Loss-of-Funds Review Findings (#1–#24)

Security review of the owner's own code, covering lenses L01–L18 from `lens.md`. The review date is
2026-09-28.

**Read this first**

- These are **code-review findings**. None has been confirmed with a proof-of-concept yet. Before any of
  them is reported externally, each impact claim needs an executed test that demonstrates it.
- Numbering follows the order in which findings were reported, not severity. The table below gives
  severity. The "Recommended order" section gives a fix priority.
- **Status** says how each finding was checked:
  - **Verified**: the main reviewer re-read the cited code and confirmed every link in the path.
  - **Partially verified**: the key lines were confirmed, but not every link.
  - **Sub-review**: reported by a parallel reviewer and not independently re-checked.
- All paths are relative to the repository root. `C/` means `pkg/contracts/`.
- A longer standalone write-up of #3 exists at `findings/03-lane-reward-front-running.md`.

---

## Summary

| # | Severity | Lens | Title | Status |
|---|---|---|---|---|
| 1 | Critical on a custom L1 · N/A on registry L1s | L04/L05/L06 | L1 chain config is not bound into proofs | Verified |
| 2 | Design risk (amplifier) | L01/L05 | SP1 and TEE lanes run the same program, and reaching the threshold closes challenges | Verified |
| 3 | Low | L01/L02 | Proof-lane rewards can be front-run or burned to `address(0)` | Verified |
| 4 | **High** if the RPC is reachable | L11 | Anyone who can reach the prover-service can poison proof lanes | Verified |
| 5 | **High** with one compromised lane | L09 | The challenger silently drops games it can't evaluate | Verified |
| 6 | Medium | L08/L09 | A hardfork cutover orphans in-flight honest games | Verified |
| 7 | Medium | L11 | Outsiders can make the operator pay for SP1 proving | Partially verified |
| 8 | Medium | L09 | No SP1 fallback for unchallenged games | Partially verified |
| 9 | Medium | L18/L01 | The incident procedure (retirement) freezes new proposals and withdrawals | Verified |
| 10 | Low | L07 | Tropo/Strato are not required to activate after Karst | Verified |
| 11 | Low | L09 | The challenger's bond can be exhausted by spam games | Sub-review |
| 12 | Low | L09/L10 | Bond managers forget games after a restart | Partially verified |
| 13 | Low / Info | L09/L10 | Challenger startup cursor gaps; proposer stalls at attempt 64 | Sub-review |
| 14 | Low–Medium | L17 | `executeBurn` sells the escrow at a lagging, floored oracle price | Verified |
| 15 | Low | L13/L14 | Nullifier bookkeeping leak and free PBH-slot griefing | Verified |
| 16 | Low | L13 | A failed spend transaction still ships the PBH transactions | Verified |
| 17 | Low | L18 | Owner divergence blocks `challenge()` | Verified |
| 18 | Low | L18/L17 | Unsafe defaults in the mainnet fee-vault script | Partially verified |
| 19 | Low | L16 | Public `eth_simulateV1` can overwrite cached witnesses | Verified |
| 20 | Low–Medium | L15/L16 | The pending block is not tied to the canonical tip | Verified |
| 21 | **Medium** | L15/L07 | BAL flashblock validation is not independent of the builder | Verified |
| 22 | Low | L14 | PBH validity is not rechecked after pool entry | Sub-review |
| 23 | Info (latent Critical) | L12/L14 | No on-chain World ID check on mainnet, and nested calls skip the pool | Partially verified |
| 24 | Info | L14/L16 | Transaction conditionals are disabled, and would go unenforced if enabled | Sub-review |

### Recommended order

1. **#4:** authenticate the prover-service and verify proofs before storing them. This is the only High an outsider can trigger directly.
2. **#5:** stop the challenger from silently dropping games it can't evaluate, and require proposals to extend beyond the current anchor.
3. **#21:** make BAL validation independent of the builder's access list.
4. **#9:** fix parent validity after retirement, before anyone needs the incident runbook.
5. **#6, #7, #8:** defender domain cutovers, worker game checks, and an SP1 fallback.
6. Everything else.

---

## #1 — L1 chain config is not bound into proofs

**Severity:** Critical when the L1 is not in Kona's built-in registry; not applicable when the L1 is
Ethereum mainnet, Sepolia or Holesky · **Lens:** L04/L05/L06 · **Status:** Verified

### Summary
Both proof lanes commit a `rollupConfigHash`, but that hash covers only the L2 rollup config plus the
Tropo/Strato fork times. The L1 chain config, which sets L1 blob-fee parameters and therefore the L1 fees
charged on L2, is loaded separately. When Kona doesn't recognise the L1 chain id, the config comes from
an unverified input supplied by the host. So the prover can choose it, while the committed hash stays the
same.

### Vulnerability path
1. `proofs/measured/core/src/boot.rs:93-111` hashes the rollup config and the World fork times. `l1_config` is not included.
2. `proofs/measured/sp1-programs/range-utils/src/lib.rs:48` and `proofs/measured/nitro-enclave/src/enclave.rs:374` pass `boot_info.l1_config` into derivation.
3. Kona's `BootInfo::load` takes `L1_CONFIGS` from its registry. For an unknown L1 it falls back to the local preimage key, which nothing verifies. Kona's own code logs this as insecure (`kona/crates/proof/proof/src/boot.rs:200-209`).
4. A prover picks L1 fee parameters that change L2 fees, derives a different output root, and gets a valid SP1 proof and TEE signature for it under the correct `rollupConfigHash`.

### Impact
A false output root passes both lanes. A proven false root lets withdrawals drain the bridge.

### Current exposure
Safe for any deployment whose L1 is in Kona's registry (Ethereum mainnet, Sepolia or Holesky) and that
has no custom config compiled in. A sub-review also found that the host passes no L1 config
(`proofs/kona-host/src/online.rs:381`).

### Mitigation
In both the SP1 guest and the enclave, reject any L1 config that didn't come from the registry, or add it
to the `rollupConfigHash` input. The second option means rotating the vkeys, PCRs and game
implementation.

---

## #2 — SP1 and TEE lanes run the same program, and reaching the threshold closes challenges

**Severity:** Design risk; it amplifies any soundness bug · **Lens:** L01/L05 · **Status:** Verified

### Summary
WIP-1006 promises that a disputed root needs two *independent* lanes. The SP1 lane and the TEE lane both
run the same Kona range program. The enclave comments that it "mirrors" `range-utils`. Once two lanes are
in, `gameOver()` becomes true and nobody can challenge any more. So a single bug in the shared code
produces an unchallengeable false root.

### Vulnerability path
1. `proofs/measured/nitro-enclave/src/enclave.rs:350-352` mirrors `sp1-programs/range-utils`. Both use the same Kona derivation and execution, op-revm, and world fork mapping.
2. `C/src/dispute/MultiProofGame.sol:764-766`: `gameOver()` is true once `PROOF_THRESHOLD` is reached.
3. `MultiProofGame.sol:472`: `challenge()` reverts once `gameOver()` is true.
4. One soundness bug in the shared path (for example #1) lets both lanes prove the same false root, and nobody can raise an objection afterwards.

### Impact
Bridge funds, if a shared-path bug exists. The only remaining defence is the registry finality delay
plus the guardian.

### Mitigation
Options:
- count only one of SP1 and TEE toward the threshold and require the council as the other lane;
- add a lane built from an independent client;
- keep a minimum challenge window even after the threshold is reached.

---

## #3 — Proof-lane rewards can be front-run or burned to `address(0)`

**Severity:** Low · **Lens:** L01/L02 · **Status:** Verified · **Full write-up:** `findings/03-lane-reward-front-running.md`

### Summary
When a challenged game is successfully defended, the forfeited challenger bond is split between the
proposer and each accepted proof lane's reward recipient. The recipient travels in the same calldata as
the proof, but none of the verifiers check it. Anyone can copy a pending `submitProofLane`, swap in their
own address, and outbid the original. A zero recipient isn't rejected either, and a credit to
`address(0)` can never be withdrawn.

### Vulnerability path
1. The `submitProofLane` payload is `laneId (1 byte) | recipient (20 bytes) | proof` (`C/src/dispute/lib/LibProof.sol:49-50, 76-91`).
2. The verifiers check only the proof against the game's statement (`MultiProofGame.sol:528-532, 841-853`).
3. `laneRecipient[laneId] = compact.recipient` (`MultiProofGame.sol:535`).
4. `_creditDefenderWins` pays `challengerBond / (lanes + 1)` per lane (`:627-643`). `settle` then credits it into the vault (`ERC20StakingVault.sol:209-211`).

### Impact
The prover who did the work loses its reward (one third of the challenger bond per lane with two lanes).
Tokens credited to `address(0)` are locked for good. Game outcomes and the bridge are unaffected.

### Mitigation
- Reject a zero recipient.
- Bind the recipient into each lane's proof: the council digest, the enclave-signed message, and an SP1 public value.
- Alternatively, use commit-reveal.
- Setting the recipient to `msg.sender` does not help.

---

## #4 — Anyone who can reach the prover-service can poison proof lanes

**Severity:** High when the prover-service port is reachable by an attacker; otherwise defence in depth ·
**Lens:** L11 · **Status:** Verified

### Summary
The prover-service JSON-RPC has no authentication and binds `0.0.0.0:8080` by default. Anyone who can
reach it can act as a worker: lease the proof jobs for a game, then submit junk (stored as SUCCEEDED
without verification) or hold the lease forever with heartbeats. The defender treats SUCCEEDED as final
and keeps resubmitting the junk proof on-chain, where it reverts every time. It never asks for a fresh
proof. An attacker who challenged a *valid* game then wins it.

### Vulnerability path
1. `proofs/services/prover-service/src/main.rs:38` defaults `LISTEN_ADDR` to `0.0.0.0:8080`. `rpc.rs:248-257` starts the server with no auth middleware.
2. The attacker challenges a valid proposal, locking its challenger bond.
3. The defender requests SP1 and Nitro proofs. The request fields are public game values.
4. The attacker calls `prover_getNextProof` with those verifier ids and a made-up `worker_id`, and wins the lease before an honest worker (`store.rs:385-412`).
5. The attacker calls `prover_submitProof` with junk. It's accepted after only backend and lock checks, then stored as SUCCEEDED (`store.rs:693-760`). Alternatively it calls `heartbeat` indefinitely (`store.rs:863-880`), so the lease never expires.
6. Re-requesting a SUCCEEDED id returns the stored row (`store.rs:233-243`). The defender re-submits a SUCCEEDED proof on every tick and retries only on Failed or Cancelled (`proofs/services/defender/src/lane.rs:128-205`).
7. The threshold is never reached, and the game resolves `CHALLENGER_WINS` / `PROOF_TIMEOUT` (`MultiProofGame.sol:596-610`).

A sub-review also reported that a lease holder can record a poisoned "completed" SP1 session, which later
honest workers resume (`proofs/workers/sp1/src/backend.rs:98-108, 920-943`). That was not re-checked.

### Impact
- **Victim:** the honest proposer. **Beneficiary:** the attacker, as challenger. The attacker receives its own bond plus 50% of the proposer bond.
- A valid root is rejected, which delays withdrawals.
- The only backstop is a manual security-council lane.

### Mitigation
- Authenticate the RPC, with separate credentials for requesters and workers, and bind it to a private address by default.
- Verify proofs before storing them (SP1 against the aggregation vkey and the full public values; Nitro by recovering the signer and checking the registry).
- When the chain rejects a stored proof, mark it Failed and re-prove.
- Cap the total lease or heartbeat time, and clear sessions on requeue.

---

## #5 — The challenger silently drops games it can't evaluate

**Severity:** High, but exploitable only together with one compromised proof lane · **Lens:** L09 ·
**Status:** Verified

### Summary
`initialize` accepts any valid, non-retired parent, so a new game can target an L2 block far in the past
or far in the future. The challenger can't compute the output root for blocks outside its node's proof
window (about 7 days) or beyond the finalized head. It retries those games, then drops them at the
deadline with only a warning. An unchallenged game needs just one proof lane to win. So one forged lane
drains the bridge, even though WIP-1006 promises that any objection raises the bar to two lanes.

### Vulnerability path
1. `MultiProofGame.sol:358, 430-454`: the parent only has to be registered, respected, not blacklisted, not retired and not doomed. The child's block is `parent + blockInterval`.
2. For a future block, `challenger.rs:151-160` returns `L2BlockNotFinalized`. For an old block, `output_root_at_block` fails once the block is outside the node's proof-history window (302,400 blocks by default; see the CLI test in `crates/cli/src/cli.rs`), unless the node is an archive node.
3. The game is queued for retry (`challenger.rs:170-173`).
4. At `now >= challenge_deadline` it's removed from the retry queue with only a `warn!` (`challenger.rs:307-315`).
5. With a single forged lane (a compromised TEE key or a ZK bug), the unchallenged game resolves `DEFENDER_WINS`.

### Impact
Bridge funds, with bridge depositors as the victims. It requires one compromised lane, which is exactly
the case the challenge mechanism exists to cover.

### Mitigation
- **Contract:** require new proposals to extend beyond the current anchor block.
- **Challenger:**
  - fail closed: near the deadline, challenge any game it still can't evaluate that has an accepted lane;
  - challenge any game beyond the unsafe head immediately;
  - for historical blocks, compare against the canonical lineage game's `rootClaim`;
  - run an archive endpoint;
  - page an operator instead of logging a warning.

---

## #6 — A hardfork cutover orphans in-flight honest games

**Severity:** Medium · **Lens:** L08/L09 · **Status:** Verified

### Summary
`domainHash` includes `rollupConfigHash`, and that hash includes the fork schedule. So every fork-schedule
change is a domain change. The services look up the lineage under one domain only, and the defender stops
defending anything outside it. Honest games still inside their challenge window under the old domain are
dropped, and an attacker can challenge them and take half of each proposer bond.

### Vulnerability path
1. `MultiProofGame.sol:217-218`: `domainHash` covers `rollupConfigHash`. `proofs/measured/core/src/boot.rs:93-111`: that hash covers the fork times.
2. Every lineage lookup uses the single domain read at startup (`proofs/protocol/src/lineage.rs:243-276, 325-346`; `proofs/services/defender/src/alloy.rs:105`).
3. The defender stops supporting any game that has left the selected lineage (`defender.rs:134-145`). The comment at `defender.rs:112-113` says domain-changing cutovers are "not supported".
4. Workers on the new config reject jobs for old-domain games (`proofs/protocol/src/proof_game.rs`, `RollupConfigHashMismatch`).
5. The attacker challenges each orphaned game before its deadline. With no defence, each one resolves `PROOF_TIMEOUT`.

### Impact
- **Victim:** the honest proposer. **Beneficiary:** the challenger, who nets `proposerBond / 2` per game.
- Scale: up to `challengePeriod / (blockInterval × 2s)` games per cutover.

### Mitigation
- Keep defending proposer-owned games until their deadlines, whatever their domain.
- Keep old-config workers running until those games are safe.
- Operational fix: stop proposing, and rotate only once every old-domain game is past its challenge deadline with at least one lane in.

---

## #7 — Outsiders can make the operator pay for SP1 proving

**Severity:** Medium (same exposure as #4) · **Lens:** L11 · **Status:** Partially verified. The
arbitrary game read is confirmed; the refill and planner details come from a sub-review.

### Summary
Proof requests aren't tied to real games. The worker reads its proving context (`blockInterval`, roots,
block numbers) from whatever contract address the job names, and never checks that it's a
factory-registered game. An attacker can deploy a fake "game" with a huge `blockInterval` and have the
operator pay the SP1 network to prove it. The PROVE-credit auto-refill has no total cap.

### Vulnerability path
1. `prover-service/src/store.rs:80-140`: `request_proof` doesn't validate the game.
2. `proofs/protocol/src/proof_game.rs:107-135`: the context is read from an arbitrary address. There is no factory, registry or game-type check.
3. The planner caps blocks per range but not the number of ranges (`proofs/workers/sp1/src/planner.rs:29`).
4. The status poller asks the fake contract whether the job is obsolete, so it can stay alive forever (`prover-service/src/status_poller.rs:40-57`).
5. Refills deposit `max(shortfall, refill_amount)` with no total cap (`proofs/workers/sp1/src/cmd/run.rs:486-523`). `max_price_per_pgu` is optional.

### Impact
The operator's PROVE credits and refill wallet are drained. This is griefing with real cost; nobody
directly profits.

### Mitigation
- Authenticate the RPC (see #4).
- In the worker, require the game to be factory-registered, respected and of type 1006.
- Limit the span to the expected `blockInterval`.
- Cap refills per period, and set a default maximum price.

---

## #8 — No SP1 fallback for unchallenged games

**Severity:** Medium · **Lens:** L09 · **Status:** Partially verified. Lane abandonment is confirmed; the
TEE-only selection comes from a sub-review.

### Summary
For an unchallenged game, the defender drives only the TEE lane. If the Nitro backend exhausts its
retries, the TEE lane is marked Abandoned for good and SP1 is never requested. The game ends proofless,
and the honest proposer loses its whole bond.

### Vulnerability path
1. `proofs/services/defender/src/defender.rs:197` sets `tee_only` when one lane is required. `:228-230` then skips SP1.
2. On `TooManyRetries` the TEE lane becomes `Abandoned` (`lane.rs:99-101`, `:238-240`).
3. SP1 stays `Pending`, so the all-terminal check (`defender.rs:317`) never fires. The defence idles until the deadline passes.
4. The game resolves `CHALLENGER_WINS` / `PROOF_TIMEOUT`. If anyone challenges first, the threshold of 2 can't be reached, and the challenger takes its own bond plus 50% of the proposer bond.

### Impact
The honest proposer's bond. Beneficiaries are the protocol fee recipient and an opportunistic
challenger.

### Mitigation
Fall back to SP1 when the TEE lane is abandoned or the deadline is near, and make abandonment non-sticky.

---

## #9 — The incident procedure (retirement) freezes new proposals and withdrawals

**Severity:** Medium (frozen until governance upgrades the registry; no party profits) · **Lens:**
L18/L01 · **Status:** Verified

### Summary
The README's incident procedure is "pause, then `updateRetirementTimestamp()`". That retires every
existing game, including the current anchor game. `MultiProofGame` accepts the registry sentinel as a
parent only when no anchor game exists. So after retirement, no parent is valid and no new game can ever
be created. Existing games become improper too, so proven-but-unfinalized withdrawals can't finalize and
nothing can be re-proven. This contradicts the README's claim that "proposal activity resumes from games
created after the cutover".

### Vulnerability path
1. In steady state `anchorGame != 0`, because the first `closeGame` → `setAnchorState` sets it (`MultiProofGame.sol:711`; `AnchorStateRegistry.sol:346-360`).
2. The guardian calls `updateRetirementTimestamp()` (`AnchorStateRegistry.sol:163-170`). Every game with `createdAt <= retirementTimestamp` is now retired (`:248-252`).
3. `_isValidParent` fails for every candidate (`MultiProofGame.sol:449-454`, `:430-436`):
   - the anchor game is retired;
   - the registry sentinel is rejected because `anchorGame != 0`;
   - every other game is retired as well.
4. `anchorGame` is cleared only in `ASR.initialize`, which is `reinitializer(initVersion())` and gated to the ProxyAdmin (`AnchorStateRegistry.sol:94-117`). Recovery therefore needs a new registry implementation and a governance upgrade.

Blacklisting the current anchor game has the same effect, unless a valid descendant created after it
already exists.

### Impact
Every L2→L1 withdrawal is frozen until the registry is upgraded.

### Mitigation
- Accept the sentinel when the anchor game is retired or blacklisted, and start from `getAnchorRoot()`. This matches the semantics of stock OP games.
- Alternatively, script the registry re-initialization into the incident runbook.
- Add a test for anchor → retire → `create`. The existing test `MultiProofGame.t.sol:747` only covers the case with no anchor.

---

## #10 — Tropo/Strato are not required to activate after Karst

**Severity:** Low (needs an operator misconfiguration) · **Lens:** L07 · **Status:** Verified

### Summary
The proof program forces the `KARST` EVM spec whenever Tropo or Strato is active. The node maps
Tropo/Strato to no OP fork, so its EVM follows the Karst activation alone. Nothing requires
`karst ≤ tropo ≤ strato`. With a misordered schedule, blocks in that window run pre-Karst on the node and
Karst in the proofs, and any user can trigger a divergence with Karst-only EVM behaviour.

### Vulnerability path
1. `crates/chainspec/src/spec.rs:241-256`: `op_fork_activation` maps Tropo/Strato to `ForkCondition::Never`.
2. `proofs/measured/kona-client/src/precompiles/factory.rs:43-51`: Tropo/Strato → `OpSpecId::KARST`.
3. `spec.rs:557-584`: `order_world_hardforks` only sorts the forks. Genesis parsing accepts any Tropo/Strato times, independently of Karst.

### Impact
- Honest roots become unprovable, so proposers lose bonds and withdrawals stall.
- Alternatively, the proof-derived root differs from canonical L2 state, which allows a double spend.

### Mitigation
Reject schedules unless `karst ≤ tropo ≤ strato`, both in `MyChainSpec` and in the guest and enclave.
Error out in `spec_for_timestamp` if Tropo is active while the Kona spec is below Karst.

---

## #11 — The challenger's bond can be exhausted by spam games

**Severity:** Low (design; it pays off only combined with a lane compromise) · **Lens:** L09 · **Status:**
Sub-review

### Summary
The challenger challenges every invalid game, including unproven ones that would lose anyway, ordered
only by deadline. Each challenge locks a challenger bond through the proof period and the finality delay.
An attacker who spends proposer bonds on invalid games can drain the challenger's vault balance. Then a
real attack game (one forged lane) gets `challenge()` reverting with `InsufficientBalance`, and the game
is dropped (see #5).

### Vulnerability path
`proofs/services/challenger/src/challenger.rs:135` (challengeable includes `Unchallenged`) and `:220`
(challenge order by deadline). The drop path is `:307-315`.

### Impact
Bridge funds, but only together with a lane compromise and at the cost of forfeiting the spam bonds.

### Mitigation
- Challenge games that already have an accepted lane first.
- Keep a reserve of bond capacity.
- Defer unproven invalid games while watching them for a lane.
- Alert on a low vault balance.

---

## #12 — Bond managers forget games after a restart

**Severity:** Low (recoverable by hand) · **Lens:** L09/L10 · **Status:** Partially verified. The
proposer side is confirmed; the challenger side comes from a sub-review.

### Summary
The proposer and challenger bond managers track their own games only in memory. After a restart they
rescan just the last `initial_scan_limit` factory games (default 1000). That count covers all game types,
which anyone can create. Older games that still need resolving or closing are forgotten.

### Vulnerability path
1. `proofs/services/proposer/src/bond_manager.rs:14, 26` keeps an in-memory `HashSet`. `:61-66` restarts at `game_count - initial_scan_limit`. The default is 1000 (`config.rs`).
2. On the challenger side, `bond_manager.rs:51-54` does the same, and `resolution_manager.rs:53-56` only resolves owned games.
3. A game left outside the window is never closed. If governance later blacklists or retires it, `closeGame` switches to REFUND (`MultiProofGame.sol:712-713`). For a defended game, the forfeited challenger bond then goes back to the challenger instead of the proposer.

### Impact
Bonds stay stuck until someone acts manually. Real loss happens only if a blacklist or retirement lands
in the gap. `resolve()` and `closeGame()` are permissionless, so recovery is possible.

### Mitigation
Persist the tracked set and cursor. On startup, seed it from the lineage. Alert on owned games that are
finalized but not settled.

---

## #13 — Challenger startup cursor gaps; proposer stalls at attempt 64

**Severity:** Low / Info · **Lens:** L09/L10 · **Status:** Sub-review

### Summary
- **Challenger cursor:** the startup binary search (`challenger.rs:103-106`) assumes challenge deadlines increase with factory index and that other game types are old. Interleaved game types, or an implementation rotation with a different challenge period, can put the cursor past live games. The initial scan has no lookback (`:266-268`). The second tick's 100-game lookback (`:269`) covers most cases.
- **Proposer attempt cap:** `proofs/protocol/src/lineage.rs:334-357` scans only attempts 0–63, while `proposer.rs:83-89` proposes `attempt + 1`. After attempt 64 is created, the proposer can't see it and stalls, and that one bond is never handled automatically. Reaching it takes 64 consecutive proof timeouts.

### Impact
Only a missed challenge after a restart, and only with a lane compromise. Otherwise one stuck bond and a
stalled proposer.

### Mitigation
- Search linearly with a deadline filter, or apply the lookback at startup too.
- Stop at `MAX_ATTEMPT_SCAN - 1` and alert, or scan without a fixed cap.

---

## #14 — `executeBurn` sells the escrow at a lagging, floored oracle price

**Severity:** Low–Medium (depends on the oracle update thresholds) · **Lens:** L17 · **Status:** Verified

### Summary
`executeBurn` is permissionless. The caller receives the entire ETH balance and must burn at least
`floor(ETH/WLD) × balance × 99.97%` WLD. Push oracles can drift from the market by up to their update
threshold before refreshing, and only `expiresAt` is checked. Callers will therefore burn when the oracle
rate favours them. The price is also floored to a whole number of WLD per ETH: the comment says 64.64
fixed point, but `shr(64)` discards the fraction.

### Vulnerability path
1. `C/src/fees/FeeEscrow.sol:130-157`: the whole balance goes to the caller, who must burn `expectedBurn`.
2. `:161-170`: the only staleness check is `expiresAt`.
3. `:181-186`: `shr(64, div(shl(64, …), …))` floors the result to an integer.

### Impact
- **Victim:** the fee-burn programme. **Beneficiary:** the arbitrageur.
- The escrow gets up to about (ETH/USD drift + WLD/USD drift − 0.03%) less WLD than the ETH is worth, on every burn.
- Flooring adds under 0.1% at current prices, and more if WLD appreciates against ETH.

### Mitigation
- Keep the fractional price.
- Reject prices whose `timestamp` is older than a few seconds.
- Cross-check against a DEX TWAP.
- Cap the ETH released per burn, or restrict `executeBurn` to a keeper.

---

## #15 — Nullifier bookkeeping leak and free PBH-slot griefing

**Severity:** Low (no user ETH or tokens are taken) · **Lens:** L13/L14 · **Status:** Verified

### Summary
The builder adds a transaction's nullifiers to the block's spent set *before* executing it, and never
removes them when the transaction is rejected or fails. The duplicate check also inserts as a side
effect, so a transaction rejected as a duplicate still leaves its earlier nullifiers behind. All of them
are then written on-chain by the builder's spend transaction. That has two effects:

- Anyone can burn another user's PBH slot for free.
- The leftover entries break the spend transaction's gas reservation (see #16).

### Vulnerability path
1. `crates/builder/src/execution_context.rs:339-347`: `payloads.iter().any(|p| !spent.insert(p.nullifier_hash))` inserts, stops at the first duplicate, and keeps the earlier inserts.
2. `:371-393`: on `InvalidTx` the loop `continue`s without removing anything.
3. **Griefing:**
   1. The attacker submits a bundle containing a victim's pending proof `V`, followed by a proof `D` that another pending transaction also carries.
   2. The pool accepts it, because it de-duplicates only within one transaction (`crates/pool/src/validator.rs:206-213`).
   3. The builder inserts `V`, finds `D` is a duplicate, and drops the bundle without executing it, so the attacker pays nothing.
   4. The victim's real transaction then hits `DuplicateNullifier` and is evicted.
   5. `V` is spent on-chain by the spend transaction (`:412-416`).
4. The 100k `FIXED_GAS` is reserved only when `spent.len() == payloads.len()` (`:362-364`). Leftover entries make that condition false, so the spend transaction's gas (`100k + 20k × set size`) may not fit.

### Impact
Victims lose a monthly PBH slot and have their operation dropped. The builder pays for writing
nullifiers that aren't needed. Unspent nullifiers from #16 allow quota bypass.

### Mitigation
- Check all nullifiers first, then commit them per transaction, and only after successful execution.
- Don't evict later duplicates unless the earlier transaction was actually included.
- Size the reservation from the final set: `FIXED_GAS` once, plus 20k per nullifier.

---

## #16 — A failed spend transaction still ships the PBH transactions

**Severity:** Low · **Lens:** L13 · **Status:** Verified

### Summary
If the builder's `spendNullifierHashes` transaction fails, the builder deliberately keeps the block, and
its PBH transactions, with their nullifiers unspent. The contract records nothing on use and relies
entirely on this backrun (`PBHEntryPointImplV1.sol:238, 356-362`). So those proofs stay reusable.

### Vulnerability path
`crates/builder/src/execution_context.rs:427-447` logs and continues when the spend transaction fails.
Triggers include the gas overflow from #15, an exhausted builder balance, or a builder whose
authorization was revoked (`onlyBuilder`).

### Impact
PBH users can exceed their monthly quota. Today that buys priority blockspace only; see #23 for why this
matters if value is ever tied to PBH.

### Mitigation
Rebuild the attempt without the PBH transactions, or fail it, when the spend fails. Alert on low builder
balance and on repeated spend failures.

---

## #17 — Owner divergence blocks `challenge()`

**Severity:** Low (triggered by governance; fails open) · **Lens:** L18 · **Status:** Verified

### Summary
The vault's `ownersAligned` modifier is meant to fail closed for new proposals when the factory owner and
the vault ProxyAdmin owner differ. It is also applied to `lockChallengerBond`, so `challenge()` reverts
in that state too. During a two-step governance migration, invalid games can't be challenged, and an
unchallenged game needs only one lane.

### Vulnerability path
`C/src/dispute/ERC20StakingVault.sol:179` (`lockChallengerBond ... ownersAligned`) and `:242-250`. This
is called from `MultiProofGame.challenge()` (`MultiProofGame.sol:486`).

### Impact
Bridge funds, but only together with a lane compromise during the divergence window.

### Mitigation
Remove `ownersAligned` from `lockChallengerBond`, or rotate both owners in one atomic transaction.

---

## #18 — Unsafe defaults in the mainnet fee-vault script

**Severity:** Low (operator misconfiguration) · **Lens:** L18/L17 · **Status:** Partially verified. The
`FeeRecipient` behaviour is confirmed; the script defaults come from a sub-review.

### Summary
`C/scripts/mainnet/DeployFeeVaults.s.sol` falls back to `OWNER = msg.sender` and `RECIPIENT =
address(0)` when the env vars are missing. Under `forge script` without `--sender`, `msg.sender` is
Foundry's default sender, which has no known key. `FeeRecipient` has no setters, and its constructor
checks neither the recipient nor the ratio.

### Vulnerability path
1. With the env vars unset, the owner is a keyless address or the recipient is `address(0)`.
2. `C/src/fees/FeeRecipient.sol:42-46` sets both permanently.
3. With a keyless owner, `withdraw()` can never be called. With a zero recipient, `withdraw()` (`:53-57`) sends the ETH to `address(0)`.
4. A ratio above 100% would forward the accumulated vault share to the escrow.

### Impact
The retained share of fee revenue is permanently locked or burned.

### Mitigation
Require the env vars (`vm.envAddress`, not `envOr`). In the constructor, require
`_recipient != address(0)` and `_ratio <= SCALE`.

---

## #19 — Public `eth_simulateV1` can overwrite cached witnesses

**Severity:** Low (no in-repo consumer yet; the guest verifies preimages) · **Lens:** L16 · **Status:**
Verified

### Summary
With `--witness.collect`, every block executor built from the node's EVM config carries the witness
sender, including those used by public simulation RPCs. Records are keyed only by block number, and the
cache overwrites the entry without checking the canonical hash. A simulation against a historical base
block can therefore replace the real witness for that height. A flood of simulations can also crowd real
witnesses out of the bounded channel.

### Vulnerability path
1. `crates/evm/src/factory.rs:47-60`: every executor gets the sender.
2. `crates/evm/src/execution/executor.rs:66-83`: `finish()` sends `{block_number, record}` via `try_send`.
3. `crates/evm/src/collector.rs:30-69`: records are buffered by number and assembled against the canonical parent, then `cache.insert(block_number, …)`.

### Impact
`debug_collectRangeWitness` serves wrong or incomplete witnesses. Once a prover consumes them, honest
proofs can fail, which is the "valid game undefended" class of #4 and #8.

### Mitigation
Attach the sender only to the engine's import executor. Key records by block hash, and insert only when
the hash matches the canonical block at that height.

---

## #20 — The pending block is not tied to the canonical tip

**Severity:** Low–Medium (the final block has to differ from the preconfirmation) · **Lens:** L15/L16 ·
**Status:** Verified (the two sub-reviews reported it independently; the main reviewer confirmed the
hook and write sites)

### Summary
The pending block is cleared only when its hash equals the new tip, or when its number is at or below the
tip. Nothing checks that its parent is still canonical. RPC lookups prefer the pending block over the
database. Stale preconfirmations can therefore keep returning receipts. Those receipts carry the latest
block number but a different hash, so naive "1 confirmation" checks pass.

### Vulnerability path
1. `crates/validator/src/coordinator.rs:160-171`: the clear rule is `pending.hash == tip.hash || pending.number <= tip.number`.
2. **Race:** a flashblock validation already running calls `send_replace` after the canonical hook has cleared the pending block (`coordinator.rs:526`).
3. **Reorg:** a switch to a sibling block at the same height, or to a lower tip, never meets the clear rule.
4. **Failover:** a replacement payload with the same timestamp is dropped (`crates/p2p/src/protocol/handler.rs:1330-1341`, `crates/primitives/src/flashblocks.rs:345-349`), so the old builder's preconfirmations stay served.
5. `crates/rpc/src/eth/transaction.rs:108-139` checks the pending block before the provider. `receipt.rs:54-63` falls back to it.

### Impact
A double spend against integrators that credit on a receipt: the attacker replaces the transaction by
reusing its nonce.

### Mitigation
- Serve and store the pending block only when `parent_hash` equals the canonical head. Guard both `send_replace` sites (`coordinator.rs:410, 526`) the same way.
- Clear the pending block whenever the tip hash is neither its own hash nor its parent's hash.

---

## #21 — BAL flashblock validation is not independent of the builder

**Severity:** Medium (needs a buggy or malicious builder) · **Lens:** L15/L07 · **Status:** Verified.
Validator mechanics and revm read semantics are confirmed; the canonical-insertion path comes from two
sub-reviews.

### Summary
On the access-list (BAL) path, the flashblock validator's "re-execution" isn't independent:

- parallel workers read earlier transactions' writes from the builder-supplied access list;
- the state root, trie updates and hashed state are computed from that same list;
- the only execution-bound check compares the hash of what was *executed* with the hash the builder *claims*.

The received list is never checked against either. A builder, or a BAL-generation bug, can supply a list
that makes an otherwise-failing transaction succeed. All checks still pass, and the block becomes
canonical on flashblocks-enabled nodes.

### Vulnerability path
1. `crates/validator/src/execution_strategy.rs:209`: workers run with `.with_bal(received_bal)`. revm fills account reads from the supplied list's writes up to the current index (`revm-database-interface-41.0.0/src/bal.rs:128-148`).
2. `:283-297`: `predicted_hashed_state` and the state root come from the received `access_list`.
3. `:384`: only `access_list_hash(computed) == expected_access_list_hash` is checked, and the builder chooses `expected_access_list_hash`.
4. `:459-506`: the receipts root, state root and block hash are compared with builder-supplied values that are consistent with steps 1–3.
5. `coordinator.rs:537-538` broadcasts the payload into the engine tree. A later `newPayload` for the same hash is a cache hit with no re-execution.

Example: the list claims transaction 1 left Alice with 2000. Transaction 2 then spends 1000 from Alice
successfully, although sequential execution would reject it.

### Impact
- Merchants see forged balances and receipts, not only as pending.
- Flashblocks-enabled nodes diverge from nodes that re-execute.
- An honest proposer reading a flashblocks node can post a root no lane can prove, and lose its bond.
- The bridge is not at risk, because the proof programs execute sequentially.
- The legacy (non-BAL) path is unaffected: its state root comes from the executed bundle.

### Mitigation
- Reject unless `access_list_hash(&access_list) == expected_access_list_hash`, or, stronger, unless `computed_access_list == access_list`. Either check makes the parallel reads equivalent to sequential execution.
- Also assert that the predicted hashed state equals `HashedPostState::from_bundle_state(executed bundle)`.

---

## #22 — PBH validity is not rechecked after pool entry

**Severity:** Low · **Lens:** L14 · **Status:** Sub-review

### Summary
PBH transactions are validated once, when they enter the pool. The date check uses wall-clock time, while
the contract uses `block.timestamp`. The pool never checks on-chain spent nullifiers, and pooled
transactions aren't re-validated on new blocks. Transactions pending across a month rollover, or carrying
already-spent nullifiers, keep top priority, get included, and revert.

### Vulnerability path
- `crates/pbh/src/payload.rs:109` uses `Utc::now()`, versus `PBHExternalNullifier.sol:76-83`.
- `crates/pool/src/validator.rs:215-230` has no `nullifierHashes` check, versus `PBHEntryPointImplV1.sol:238`.
- `validator.rs:300-324` (`on_new_head_block`) does no re-validation. `ordering.rs:37-40` gives PBH priority.

### Impact
The sender (usually a bundler) pays gas for each revert, and the builder pays to re-spend. Small and
bounded.

### Mitigation
Check `nullifierHashes` in the pool and the builder. Evict PBH transactions at month or root changes.
Validate the date against the next block's timestamp.

---

## #23 — No on-chain World ID check on mainnet, and nested calls skip the pool

**Severity:** Info today; latent Critical if value is ever tied to PBH execution · **Lens:** L12/L14 ·
**Status:** Partially verified. The contract behaviour is confirmed; the deploy parameter and pool gate
come from two sub-reviews.

### Summary
The mainnet deploy initializes the PBH entry point with `worldId = 0`, so proofs are never checked
on-chain. The pool checks PBH proofs only for top-level transactions sent to the entry point. A wrapper
contract, or an EIP-7702 account, can call `handleAggregatedOps` with made-up proofs and fresh
nullifiers. Those nullifiers are never recorded, so they can be reused. The PBH-keyed Safe operations
execute, and `PBH` events are emitted.

### Vulnerability path
- `C/scripts/mainnet/Deploy.s.sol:60` sets `worldId = 0`, and `numPbhPerMonth = type(uint16).max` at `:27`.
- `C/src/pbh/PBHEntryPointImplV1.sol:247` skips proof verification when `worldId == 0`.
- `crates/pool/src/validator.rs:293` gates PBH validation on `to == pbh_entrypoint`.
- Neither `handleAggregatedOps` nor the EntryPoint restricts the caller.

### Impact
None today: no priority is gained, and nothing gives value for PBH status. Any future subsidy or
paymaster keyed on PBH execution (WIP-1002/1003) could be drained by non-humans.

### Mitigation
Keep `worldId` set on-chain before tying any value to PBH, or restrict `handleAggregatedOps` to
builder-validated top-level transactions.

---

## #24 — Transaction conditionals are disabled, and would go unenforced if enabled

**Severity:** Info · **Lens:** L14/L16 · **Status:** Sub-review

### Summary
`enable_tx_conditional` is hard-coded to `false`, so `eth_sendRawTransactionConditional` is never served
and `--rollup.enable-tx-conditional` is silently ignored. That fails closed. If it's re-enabled as-is, the
custom pool never spawns upstream's conditional maintenance task, and the builder never checks
conditionals. `blockNumberMax` and `timestampMax` would then go unenforced, so users' transactions could
execute after their own deadlines.

### Vulnerability path
- `crates/node/src/context.rs:532` hard-codes `enable_tx_conditional: false`, which gates the method at `add_ons.rs:473`.
- `crates/node/src/pool.rs:130-169` never spawns `maintain_transaction_pool_conditional_future` (upstream: op-reth `node.rs:1440-1451`).
- The builder loop has no conditional check.

### Impact
None today. If re-enabled, users could lose funds to MEV because their conditionals aren't enforced.

### Mitigation
Before re-enabling, wire in the upstream maintenance task and a check at inclusion time. Also stop
silently ignoring the CLI flag.
