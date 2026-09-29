# Loss-of-Funds Audit Lenses — my-chain

Scope for a security review, by the owner, of the whole repository. The review looks for
vulnerabilities that cause **loss of funds**. Each lens names the files it covers, the funds at
risk, and the invariants a finder must try to break.

The workflow that runs these lenses is `.claude/workflows/fund-loss-audit.js`. It is not
executed yet. The lens list is embedded in that script and must stay in sync with this file.

---

## Threat model

**Untrusted (in scope as attackers)**
- Any L1 or L2 EOA or contract, any RPC client, and any devp2p or flashblocks peer.
- Bonded proposers and challengers. They can be malicious; the bond is their only stake.
- The Nitro enclave host / parent instance, and everything it feeds the enclave (preimages, witnesses, time, network).
- SP1 prover-network operators. Proofs are verified on-chain; the inputs they prove over are untrusted.
- ERC-4337 bundlers, PBH users, and Safe module callers.

**Trusted (out of scope unless the code or docs claim to constrain them)**
- L1 governance, ProxyAdmin, and contract owners.
- The OP guardian.
- The security council, up to its threshold.
- The rollup-boost authorizer key.
- The sequencer and batcher keys.
- The builder key, for flashblock contents.
- The AWS Nitro root CA.
- The SP1 verifier gateway and the vkeys/PCRs set by governance.

> Exception: `pkg/contracts/README.md` states the staking vault "exposes no administrative path
> for moving participant balances or extracting backing tokens directly." An admin path that
> contradicts that promise **is** in scope.

**Out of scope**
- DoS or liveness issues with no fund loss. Crash vectors were covered by a separate review.
- Gas and style.
- Unimplemented WIPs: 1001 (native AA), 1002/1003 (subsidies), 1004 (EdDSA precompile), 1008 (staked subblocks).
- Test, bench, devnet-only and `Mock*` code.
- Third-party library bugs, unless my-chain's integration makes them exploitable.

## Severity rubric

| Severity | Meaning |
|---|---|
| Critical | An untrusted actor directly steals or permanently destroys bridge funds (OptimismPortal2 / L1 bridges) or pooled funds held by a contract (staking vault, fee escrow, Safe wallets). |
| High | Theft or loss of one bounded party's funds (a bond, a builder or prover account, a user's L2 balance), or a permanent freeze of bridge withdrawals. |
| Medium | Loss that needs unlikely preconditions, or funds frozen until governance recovers them. |
| Low | Small, bounded value leakage (rounding, fee misattribution). |

Guardian or council intervention is a backstop, not a refutation. It can lower severity by at
most one level.

---

## Lens index

| ID | Lens | Primary asset at risk | Max impact |
|---|---|---|---|
| L01 | Dispute-game resolution & withdrawal soundness | All bridged L1 funds | Critical |
| L02 | Bond accounting & game payouts | WLD proposal/challenge bonds | High |
| L03 | ERC20StakingVault custody | Entire pooled bond token balance | Critical |
| L04 | On-chain proof verifiers & journal binding | All bridged L1 funds | Critical |
| L05 | Proof-program soundness (SP1 range/aggregation, kona client) | All bridged L1 funds | Critical |
| L06 | Nitro enclave TEE & attestation | All bridged L1 funds | Critical |
| L07 | Node execution ↔ proof-program equivalence | L2 balances, bonds, bridge | Critical |
| L08 | Protocol types, lineage & claim encoding | Bonds, bridge (via wrong claims) | High |
| L09 | Challenger & defender correctness | Bridge (unchallenged invalid game), honest bonds | Critical |
| L10 | Proposer & bond manager | Proposer bond and gas funds | High |
| L11 | Prover service, workers, host & key custody | Prover credits, signer keys, bridge | High |
| L12 | PBH contracts & 4337 Safe module | Safe wallet funds, EntryPoint deposits | Critical |
| L13 | Builder payload construction & builder-funded txs | Builder balance, fee recipient revenue | High |
| L14 | Transaction pool & PBH off-chain validation | Builder gas, user funds (conditional txs) | Medium |
| L15 | Flashblocks preconfirmation integrity | Funds of parties acting on preconfirmations | High |
| L16 | RPC, sequencer forwarding & admin surface | User funds, prover inputs | High |
| L17 | Fee contracts (FeeEscrow, FeeRecipient) | Escrowed ETH/WLD fee revenue | High |
| L18 | Upgradeability, initialization & deployment wiring | Vault, PBH, games, bridge | Critical |

All paths are relative to the repo root. `C/` = `pkg/contracts/`.

---

## L01 — Dispute-game resolution & withdrawal soundness

**Files**
- `C/src/dispute/MultiProofGame.sol`
- `C/src/dispute/interfaces/IMultiProofGame.sol`
- `C/src/dispute/lib/LibProof.sol`
- `C/src/dispute/lib/GameTypes.sol`
- `C/src/dispute/lib/Errors.sol`
- Unmodified upstream, for reference: `C/lib/optimism/packages/contracts-bedrock/src/L1/OptimismPortal2.sol`, `.../src/dispute/AnchorStateRegistry.sol`, `.../src/dispute/DisputeGameFactory.sol`
- Spec: `wips/wip-1005.md`, `wips/wip-1006.md`, `pkg/contracts/README.md` (withdrawal boundary table)

**Impact (Critical):** An invalid L2 output root reaches `DEFENDER_WINS` and `isGameClaimValid`.
The attacker then proves and finalizes a fraudulent withdrawal, draining the ETH and ERC-20s held
by OptimismPortal2 and the L1 bridges. Conversely, forcing a valid root to `CHALLENGER_WINS`
freezes honest withdrawals.

**Invariants / questions**
- `DEFENDER_WINS` requires every required proof lane to be satisfied for exactly this `rootClaim`, `l2BlockNumber`, parent and L1 head.
- An invalid or blacklisted parent invalidates in-progress children.
- `status`, `createdAt`, `resolvedAt` and the clocks cannot be manipulated to skip proof maturity or finality delay.
- `closeGame()` advances the anchor only on valid lineage and cannot be called in a way that corrupts future games.
- `extraData` / `l2BlockNumber` decoding matches what the verifiers bind.

## L02 — Bond accounting & game payouts

**Files**
- `C/src/dispute/MultiProofGame.sol` (initialize bond lock, `challenge*`, `resolve`, `closeGame`, normal vs refund mode, credit)
- `C/src/dispute/ERC20StakingVault.sol`
- `C/src/dispute/interfaces/IERC20StakingVault.sol`
- `C/src/dispute/interfaces/IMultiProofGame.sol`

**Impact (High):** Theft or permanent lock of WLD bonds. Possible routes: the pot is credited
twice; an honest party's bond goes to the attacker; a loser recovers a bond through refund mode;
credits land on the wrong account.

**Invariants / questions**
- The sum of credits paid out equals the sum of bonds locked, per game, exactly once.
- Only the correct winners are credited.
- Refund-mode selection cannot be steered by the losing side.
- Bonds cannot be unlocked while the game is live.
- Resolve and close ordering races are handled; repeated calls are no-ops.

## L03 — ERC20StakingVault custody

**Files**
- `C/src/dispute/ERC20StakingVault.sol`
- `C/src/dispute/interfaces/IERC20StakingVault.sol`
- `C/src/abstract/Base.sol` (UUPS + Ownable2Step)
- Wiring reference: `C/scripts/devnet/DeployProofSystem.s.sol`, `C/scripts/devnet/ActivateProofSystem.s.sol`

**Impact (Critical):** All pooled bond tokens are drained. Possible routes:
- withdraw more than your available balance;
- bypass the withdrawal delay;
- a forged or unregistered clone passes the "deterministic factory address" authentication, then locks or credits other users' balances;
- an old game implementation is abused after upgrades;
- an admin or upgrade path moves participant balances, contradicting the README guarantee;
- fee-on-transfer or rebasing tokens desync the accounting.

**Invariants / questions**
- `token.balanceOf(vault) >= Σ(available + locked + pending withdrawals)` at all times.
- Only genuine factory-created game clones can lock or credit.
- A new request resets the delay for the full pending amount and cannot be used to shorten it.

## L04 — On-chain proof verifiers & journal binding

**Files**
- `C/src/dispute/sp1/SP1ValidityVerifier.sol`
- `C/src/dispute/nitro/NitroProofVerifier.sol`
- `C/src/dispute/nitro/NitroAttestationVerifier.sol`
- `C/src/dispute/nitro/NitroEnclaveKeyRegistry.sol`
- `C/src/dispute/council/SecurityCouncilVerifier.sol`
- `C/src/dispute/lib/LibProof.sol`
- `C/src/dispute/interfaces/IMyChainProofVerifier.sol`
- `C/src/dispute/interfaces/INitroAttestationVerifier.sol`
- Integration: `C/lib/sp1-contracts`, `C/lib/nitro-validator`

**Impact (Critical):** A verifier accepts a proof or signature that is not bound to the full
tuple (game/chain id, `rootClaim`, `l2BlockNumber`, parent root, L1 head, vkey/PCRs, config). It
can then be replayed from another game or chain, or forged, to prove a false root and drain the
bridge.

**Invariants / questions**
- Every public value the program or enclave commits is checked on-chain, and every checked value is committed.
- The key registry accepts only fresh, valid attestations for the expected PCRs.
- Revocation takes effect for already-registered signers.
- Council signatures are threshold-checked, deduplicated, domain-separated and not malleable.

## L05 — Proof-program soundness (SP1 range/aggregation, kona client)

**Files**
- `proofs/measured/sp1-programs/range-ethereum/src/main.rs`
- `proofs/measured/sp1-programs/range-utils/src/lib.rs`
- `proofs/measured/sp1-programs/aggregation/src/main.rs`
- `proofs/measured/core/src/{boot.rs,range.rs,witness/mod.rs,types.rs,artifacts.rs,lib.rs}`
- `proofs/measured/kona-client/src/{executor.rs,pipeline.rs,precompiles/factory.rs,precompiles/mod.rs,lib.rs}`

**Impact (Critical):** The program commits public values that are not derived from verified L1
data or verified oracle preimages. Or the aggregation program fails to enforce contiguous ranges,
a shared L1 head, the range vkey, or the rollup config hash. In either case a valid proof of a
false output root exists, which drains the bridge.

**Invariants / questions**
- Every host-supplied witness or preimage is hash-verified before use.
- Boot info (L1 head, agreed/claimed output root, block number, chain config) is committed exactly as the contract expects.
- Aggregation checks `range[i].claimed == range[i+1].agreed` and the vkey of each inner proof.

## L06 — Nitro enclave TEE & attestation

**Files**
- `proofs/measured/nitro-enclave/src/{attestation.rs,cose.rs,p384_hints.rs,enclave.rs,protocol.rs,host.rs,lib.rs,prewarm.rs,main.rs}`
- `proofs/backends/nitro/register/src/lib.rs`
- `proofs/workers/nitro/src/cmd/{run.rs,register.rs,get_attestation.rs,mod.rs,common.rs}`
- On-chain side: `C/src/dispute/nitro/NitroAttestationVerifier.sol`, `C/src/dispute/nitro/NitroEnclaveKeyRegistry.sol`

**Impact (Critical):** A registered enclave signer attests a false output root, which drains the
bridge. Possible causes:
- the untrusted host feeds unverified preimages or witnesses;
- the enclave signs a message that is not domain-separated or is attacker-chosen;
- a non-enclave key gets registered because attestation parsing, the COSE signature, the certificate chain, PCR, nonce or timestamp checks can be bypassed;
- the signing key can be extracted.

**Invariants / questions**
- The enclave signs only after it has itself executed and verified the program.
- The attestation's `user_data` / `public_key` binds exactly the registered signer.
- The P-384 hint path cannot be used to fake verification.

## L07 — Node execution ↔ proof-program equivalence

**Files**
- `crates/evm/src/{lib.rs,factory.rs,utils.rs,collector.rs,cache.rs,execution/bal.rs,execution/basic.rs,execution/executor.rs}`
- `crates/chainspec/src/{spec.rs,hardfork.rs,builder.rs}`
- `crates/validator/src/execution_strategy.rs`
- `proofs/measured/kona-client/src/{executor.rs,precompiles/factory.rs}`
- `proofs/protocol/src/types.rs` (output-root computation)

**Impact (Critical/High):** The node's canonical state differs from what the proof program
derives. Sources to check: BAL parallel-execution ordering, deposit `mint`, fee-vault crediting,
system transactions, the hardfork schedule, and the precompile set. The consequences:
- honest proposals become unprovable or challengeable, so honest bonds are lost;
- parallel execution can duplicate or mint balance, and the attacker then withdraws it through a matching proof.

**Invariants / questions**
- BAL / parallel execution yields a bit-identical state to sequential execution for every transaction ordering, including conflicting accesses and reverted transactions.
- The hardfork activation table is identical in the node and the proof program.

## L08 — Protocol types, lineage & claim encoding

**Files**
- `proofs/protocol/src/{types.rs,lineage.rs,proof_game.rs,consensus_provider.rs,bindings.rs,lib.rs}`

**Impact (High):** The output root, `extraData`, journal or lineage is computed off-chain in a
way that differs from what the contract expects, or a non-canonical parent is accepted. Honest
services then propose or defend wrong claims (bond loss) or fail to recognize invalid games
(bridge risk).

**Invariants / questions**
- Off-chain encoding round-trips exactly with the Solidity decoding.
- Lineage walks only proper, respected, non-blacklisted parents.
- The consensus provider uses finalized or safe data where required.

## L09 — Challenger & defender correctness

**Files**
- `proofs/services/challenger/src/{challenger.rs,resolution_manager.rs,bond_manager.rs,alloy.rs,config.rs,types.rs,main.rs,traits.rs}`
- `proofs/services/defender/src/{defender.rs,lane.rs,game.rs,alloy.rs,config.rs,main.rs,traits.rs}`

**Impact (Critical):** An invalid game is never challenged before its deadline, so a fraudulent
withdrawal finalizes. Possible causes:
- the game is skipped by a filter, a pagination bug, or a reorg;
- an RPC error is silently swallowed;
- decoding fails;
- attacker spam games exhaust the challenger's bond or gas.

The same failures can leave a valid game undefended, so the honest bond is lost, or leave credits
unclaimed.

**Invariants / questions**
- Every game of the respected type is evaluated at least once before its challenge deadline.
- Failures are retried, not dropped.
- Bond budgeting cannot be starved by attacker-created games.

## L10 — Proposer & bond manager

**Files**
- `proofs/services/proposer/src/{proposer.rs,bond_manager.rs,alloy.rs,config.rs,types.rs,main.rs,traits.rs}`

**Impact (High):** The proposer posts a root from an unsafe or unfinalized L2 head, or on a bad
parent, and loses its bond. Other losses:
- double proposals after a nonce error or reorg;
- bonds deposited or withdrawn incorrectly;
- unbounded gas spending.

**Invariants / questions**
- It proposes only roots that the node has computed from finalized L1 data.
- The parent is the latest valid game.
- The bond balance is checked before each proposal and withdrawn safely.

## L11 — Prover service, workers, host & key custody

**Files**
- `proofs/services/prover-service/src/{rpc.rs,service.rs,store.rs,status_poller.rs,types.rs,config.rs,lib.rs,main.rs}`
- `proofs/workers/core/src/{worker.rs,backend.rs,retry.rs,heartbeat.rs}`
- `proofs/workers/sp1/src/{backend.rs,planner.rs,main.rs}`
- `proofs/backends/sp1/host/src/{network_prover.rs,validity.rs,vkeys.rs,cpu_prover.rs,mock_prover.rs,lib.rs}`
- `proofs/services/tx-signer/src/lib.rs`
- `proofs/kona-host/src/{online.rs,lib.rs}`

**Impact (High):**
- Unauthenticated or unbounded proof requests drain paid SP1 prover-network credits.
- A proof result is mapped to the wrong game or request (DB race or status confusion), so a wrong proof is submitted.
- The mock prover is selectable in a production config.
- The tx-signer signs arbitrary transactions or leaks its key.
- The host serves unverified preimages. That matters only if the guest trusts them (see L05).

**Invariants / questions**
- Only authorized callers can enqueue proof jobs.
- A job's parameters are immutable once submitted.
- The signer restricts its targets and calldata.

## L12 — PBH contracts & 4337 Safe module

**Files**
- `C/src/pbh/{PBHEntryPointImplV1.sol,PBHEntryPoint.sol,PBHSignatureAggregator.sol,PBH4337Module.sol}`
- `C/src/lib/{SafeModuleSignatures.sol,PBHExternalNullifier.sol,ByteHasher.sol}`
- `C/src/interfaces/{IPBHEntryPoint.sol,IWorldIDVerifier.sol,IMulticall3.sol}`
- `C/src/abstract/Base.sol`
- Integration: `C/lib/account-abstraction`, `C/lib/safe-modules`, `C/lib/safe-contracts`, `C/lib/world-id-contracts`

**Impact (Critical):**
- Signature parsing in `PBH4337Module` / `SafeModuleSignatures` lets an attacker get a UserOp validated for a victim Safe and drain it.
- `pbhMulticall` executes calls under the wrong `msg.sender` context.
- A bundler or beneficiary drains EntryPoint deposits.
- The authorization on `spendNullifierHashes` is abused.
- The implementation can be taken over or re-initialized.

**Invariants / questions**
- A UserOp is valid for a Safe only with that Safe's owners' signatures over the exact `userOpHash`.
- Validation data (aggregator address, time bounds) cannot be spoofed.
- Proof data is bound to the sender and nonce (signal hash).
- A nullifier can be spent once.

## L13 — Builder payload construction & builder-funded transactions

**Files**
- `crates/builder/src/{payload_builder.rs,execution_context.rs,payload_txns.rs,lib.rs,traits/context.rs,traits/context_builder.rs,traits/payload_builder.rs}`
- `crates/payload/src/{job.rs,generator.rs}`
- `crates/node/src/{payload.rs,payload_service.rs}`

**Impact (High):**
- An attacker makes the builder sign and pay for `spendNullifierHashes` transactions repeatedly or at high gas, draining the builder balance.
- Fees or tips are miscomputed and the wrong fee recipient is credited.
- Deposit transactions are mishandled.
- PBH transactions are included but their nullifiers are never spent, so a proof can be reused.

**Invariants / questions**
- The cost the builder pays per block is bounded and not attacker-scalable.
- Every included PBH nullifier is spent in the same block.
- Fee accounting matches the EVM's own fee accounting.

## L14 — Transaction pool & PBH off-chain validation

**Files**
- `crates/pool/src/{validator.rs,tx.rs,root.rs,ordering.rs,bindings.rs,eip4337.rs,noop.rs}`
- `crates/pbh/src/{payload.rs,external_nullifier.rs,date_marker.rs}`
- `crates/node/src/{pool.rs,tx_propagation.rs}`

**Impact (Medium):**
- A PBH transaction is valid off-chain but reverts on-chain, and the builder pays for it.
- Nullifiers or roots are accepted when stale.
- Conditional-transaction (`KnownAccounts` / block-bound) checks are bypassed, so users' transactions execute in states they excluded (MEV loss).

**Invariants / questions**
- Off-chain PBH validity is at least as strict as on-chain validity.
- Root validity windows match the contract.
- Conditional checks are re-evaluated at inclusion time.

## L15 — Flashblocks preconfirmation integrity

**Files**
- `crates/validator/src/{coordinator.rs,execution_strategy.rs,validator.rs,state_root_strategy.rs}`
- `crates/p2p/src/protocol/{handler.rs,connection.rs,event.rs}`
- `crates/primitives/src/{p2p.rs,flashblocks.rs,primitives.rs,access_list.rs}`
- `crates/rpc/src/eth/{pending_block.rs,receipt.rs,transaction.rs,block.rs,call.rs}`

**Impact (High):** The preconfirmed (pending) state or receipts served to users differ from the
final canonical block. Causes to check:
- unverified BAL / access-list data;
- a stale pending block that survives a reorg or a new payload;
- a replayed authorization;
- a flashblock from an old payload mixed into a new one.

Exchanges, bridges or merchants that credit on preconfirmation are then double-spent.

**Invariants / questions**
- A pending receipt is served only for state that re-executed and matched `block_hash`.
- The pending state is cleared on every canonical change.
- The BAL is verified against the execution result before it is trusted.

## L16 — RPC, sequencer forwarding & admin surface

**Files**
- `crates/rpc/src/{sequencer.rs,transactions.rs,admin.rs,engine.rs,witness.rs,simulate.rs,simulate_consts.rs,core.rs,eth/mod.rs}`
- `crates/node/src/{add_ons.rs,engine.rs,context.rs,node.rs}`
- `crates/cli/src/cli.rs`

**Impact (High):**
- `eth_sendRawTransactionConditional` validation is bypassed, or the forwarding path skips it.
- Admin or engine endpoints are reachable without authentication, allowing trust or peer manipulation or forced payloads.
- Witness or debug endpoints feed provers attacker-influenced data.
- `simulate` leaks state or commits it.

**Invariants / questions**
- Privileged namespaces require auth by default.
- Forwarded transactions are validated identically on both sides.
- The witness is derived only from canonical data.

## L17 — Fee contracts

**Files**
- `C/src/fees/FeeEscrow.sol` (permissionless `executeBurn` + `burnCallback`, WLD/USD and ETH/USD oracles, `_safeTransferETH`)
- `C/src/fees/FeeRecipient.sol` (burn/recipient ratio split, `withdraw`)
- Deployment: `C/scripts/devnet/DeployFeeVaults.s.sol`, `C/scripts/mainnet/DeployFeeVaults.s.sol`

**Impact (High):** A caller extracts escrowed ETH for too little WLD burned. Possible causes:
- a stale, manipulated or zero oracle price;
- a decimals mismatch between the two feeds;
- rounding;
- callback reentrancy, or a callback that does not actually burn;
- the `minimumInterval` can be bypassed.

The ratio split in `FeeRecipient` can also misroute revenue.

**Invariants / questions**
- `ETH out ≤ value(WLD burned)` at a fresh, sane price.
- The burn is verified after the callback.
- There is one burn per interval.

## L18 — Upgradeability, initialization & deployment wiring

**Files**
- `C/src/abstract/Base.sol`
- `C/src/pbh/PBHEntryPoint.sol`
- `C/src/pbh/PBHEntryPointImplV1.sol`
- `C/src/dispute/ERC20StakingVault.sol`
- `C/src/dispute/MultiProofGame.sol` (clone `initialize`)
- `C/src/dispute/nitro/NitroEnclaveKeyRegistry.sol`
- `C/scripts/devnet/{DeployProofSystem.s.sol,ActivateProofSystem.s.sol,DeployNitro.s.sol,DeployDevnet.s.sol,DeployCouncilSafe.s.sol}`
- `C/scripts/mainnet/{Deploy.s.sol,DeployUpgrade.s.sol,Create2Deploy.sol}`

**Impact (Critical):** Any of the following leads to vault, PBH or bridge drain:
- an uninitialized implementation or clone is taken over;
- a contract can be re-initialized;
- a CREATE2 deployment can be front-run;
- a storage-layout collision on upgrade;
- ownership diverges between the factory and the vault's ProxyAdmin.

**Invariants / questions**
- Implementations disable initializers.
- Clones can be initialized only by the factory, once.
- Upgrades preserve storage layout.
- Production scripts never wire `Mock*` verifiers.

---

## Cross-lens interactions (checked after individual lenses)

| Lenses | Interaction checked |
|---|---|
| L01 × L09 | Game clocks, deadlines and finality delays vs the challenger's and defender's polling, pagination, error handling and bond availability |
| L05 × L07 | Does the proof program derive exactly the state the node commits? |
| L04 × L05 × L06 | Every field committed by the program or enclave vs every field bound on-chain |
| L02 × L03 | Game ↔ vault lock, credit and unlock authentication and amounts |
| L08 × L01 × L10 | Off-chain claim / `extraData` / lineage encoding vs on-chain decoding |
| L12 × L13 × L14 | PBH validity as judged by the pool, the builder and the contracts |
| L15 × L16 | Preconfirmed state over RPC vs the final block |
| L18 × L01 × L03 × L12 × L17 | Initialization, upgrade and wiring across all contracts |
