# Chain-Halt Audit Lenses — my-chain

Scope for the owner's security review of the whole repository, covering anything that can **halt the
chain**. Each lens lists the files it covers, the kind of halt at stake, and the invariants a reviewer
must confirm hold.

Only the lenses are defined here. The review hasn't been run yet, and no workflow script has been
written for it.

---

## What "chain halt" means here

| Halt type | Meaning |
|---|---|
| **H1: Unsafe-head halt** | The sequencer/builder stops producing blocks, or keeps producing blocks the rest of the network rejects. |
| **H2: Follower halt** | RPC and validator nodes stop following the chain: they crash, get stuck, or diverge from canonical. |
| **H3: Safe/finalized-head halt** | The unsafe chain can't become safe or finalized, because blocks can't be batched or derived, or nodes disagree at a hardfork. |
| **H4: Settlement halt** | L2→L1 withdrawals stop: no new dispute games can be created, games can't resolve or finalize, or the anchor can't advance. |
| **H5: Node crash** | A single node process dies. With `panic = "abort"` in the `maxperf` production profile, any reachable panic kills the whole process. |

## Threat model

**In scope (untrusted)**
- Any L2 user, RPC client, devp2p or flashblocks peer.
- Bonded proposers and challengers.
- SP1 prover-network operators and the Nitro enclave host.

**Also in scope**
- Honest operator or guardian actions that follow a documented procedure, when that procedure halts the chain. #9 in `finding.md` is an example.
- Configuration values inside documented ranges.

**Trusted**
- Governance, the guardian, and contract owners, acting outside documented procedures.
- The sequencer, batcher and builder keys.
- The rollup-boost authorizer key.

**Out of scope**
- Volumetric network DDoS against infrastructure.
- L1 outages.
- Bugs in unmodified upstream code (reth, op-node, kona), unless my-chain's integration makes them reachable.
- Test, bench, devnet-only and `Mock*` code.

## Severity rubric

| Severity | Meaning |
|---|---|
| Critical | An untrusted actor halts block production or block validity chain-wide, or permanently halts withdrawals. |
| High | A halt that lasts until an operator steps in, a halt of all follower/RPC nodes, or withdrawals halted until a governance action. |
| Medium | A temporary halt that recovers on its own, or one that needs unlikely preconditions. |
| Low | A single non-critical node crashes or degrades. |

## Related prior results
- `finding.md`:
  - #9: retirement freezes new games, an H4 halt.
  - #10: fork-order misconfiguration diverges node and proof program.
  - #13: proposer stalls at attempt 64.
  - #20 and #21: follower divergence and preconfirmation integrity.
- Earlier crash review: `panic = "abort"` in the `maxperf` profile; a latent `unwrap` in `crates/pbh/src/date_marker.rs:31`.

---

## Lens index

| ID | Lens | Halt types | Max severity |
|---|---|---|---|
| C01 | Block building & payload-job liveness | H1 | Critical |
| C02 | Produced-block validity & resource limits | H1, H2, H3 | Critical |
| C03 | Execution determinism across node roles | H1, H2, H3 | Critical |
| C04 | Engine API & consensus-client interaction | H1, H2 | Critical |
| C05 | Flashblocks P2P, coordinator & reorg handling | H1, H2 | High |
| C06 | Multi-builder publishing coordination & failover | H1 | Critical |
| C07 | Critical background tasks & shutdown semantics | H1, H2, H5 | Critical |
| C08 | Node crash surface from untrusted input (`panic = abort`) | H5, H1, H2 | Critical |
| C09 | Resource exhaustion in admission & block production | H1, H2 | High |
| C10 | PBH & builder-funded transaction interplay | H1 | High |
| C11 | Hardfork activation & chainspec consistency | H2, H3 | Critical |
| C12 | Settlement: game creation, resolution & anchor advance | H4 | Critical |
| C13 | Proof-generation liveness | H4 | High |
| C14 | Configuration, operator parameters & deployment | H1–H4 | High |

All paths are relative to the repo root. `C/` means `pkg/contracts/`.

---

## C01 — Block building & payload-job liveness

**Files**
- `crates/builder/src/{payload_builder.rs,execution_context.rs,payload_txns.rs,lib.rs,traits/context.rs,traits/context_builder.rs,traits/payload_builder.rs}`
- `crates/payload/src/{job.rs,generator.rs}`
- `crates/node/src/{payload.rs,payload_service.rs}`

**Halt at stake:** a forkchoice update with attributes never yields a usable payload before `getPayload`,
or every build attempt fails, so the sequencer produces no blocks.

**Invariants / questions**
- Every job always has a fallback payload (deposits and sequencer transactions only) that can be returned, even when every pool transaction fails.
- No single pool transaction, or combination of them, can make every build attempt return an error, rather than just skipping that transaction.
- Errors from builder-side extra transactions (the nullifier spend transaction) can't abort the whole payload.
- Job cancellation and deadlines always resolve. Check the `unreachable!` and `panic!` sites in `job.rs` and the `unwrap` at `generator.rs:268`.
- Frozen or committed payload state transitions can't reach an impossible state.

## C02 — Produced-block validity & resource limits

**Files**
- `crates/builder/src/{execution_context.rs,payload_builder.rs}` (gas, DA footprint and size accounting, `effective_gas_limit`)
- `crates/evm/src/{utils.rs,lib.rs}` (`effective_gas_limit`, gas config)
- `crates/primitives/src/flashblocks.rs` (block reconstruction)
- `crates/cli/src/cli.rs` (builder limit flags and defaults)

**Halt at stake:**
- The builder produces a block that followers or the consensus client reject, causing an unsafe-head stall or reorg loop.
- Or it produces a block that can't be batched or derived (size, DA footprint, transaction limits), causing a safe-head stall.

**Invariants / questions**
- Every block the builder seals is within the gas limit, DA footprint and size limits enforced by validators and by derivation.
- Every builder-side limit check is at least as strict as the consensus rule it mirrors.
- Accounting across flashblocks (cumulative gas, DA, bytes) is consistent between the first flashblock and later ones.
- Deposit, system and post-exec transactions are always included and ordered as consensus requires.

## C03 — Execution determinism across node roles

**Files**
- `crates/evm/src/{lib.rs,factory.rs,utils.rs,collector.rs,cache.rs,execution/bal.rs,execution/basic.rs,execution/executor.rs}`
- `crates/validator/src/{execution_strategy.rs,state_root_strategy.rs,validator.rs}`
- `crates/builder/src/execution_context.rs`
- `proofs/measured/kona-client/src/{executor.rs,precompiles/factory.rs}`

**Halt at stake:** the builder, flashblock validators, standard `newPayload` import, and the proof program
reach different results for the same block. The network splits, followers stall, and honest roots become
unprovable.

**Invariants / questions**
- Builder execution, standard import and the proof program produce bit-identical state for every block.
- The BAL and parallel paths are equivalent to sequential execution (see `finding.md` #21).
- The `unimplemented!` sites in `execution/bal.rs` and `execution/basic.rs` can never be reached in production.
- Post-exec mode, fee accounting and system-transaction handling are identical across roles.

## C04 — Engine API & consensus-client interaction

**Files**
- `crates/rpc/src/engine.rs`
- `crates/node/src/{engine.rs,add_ons.rs,node.rs}`
- `crates/validator/src/coordinator.rs` (`resolve_pending`, `event_hook`, payload-event broadcast)

**Halt at stake:** `newPayload` or `forkchoiceUpdated` hangs, times out, or returns a wrong status. The
consensus client then stalls, or accepts a block the rest of the network rejects.

**Invariants / questions**
- The race between the cached-payload path and full validation always finishes within the engine API timeout, and always returns the real validation result.
- A cache hit can never admit a block that full validation would reject.
- The custom flashblocks forkchoice methods can't leave the engine inconsistent with the consensus client.

## C05 — Flashblocks P2P, coordinator & reorg handling

**Files**
- `crates/p2p/src/protocol/{handler.rs,connection.rs,event.rs,metrics.rs,recorder.rs,error.rs}`
- `crates/p2p/src/monitor/mod.rs`
- `crates/primitives/src/{p2p.rs,flashblocks.rs,primitives.rs,access_list.rs,payload_id.rs}`
- `crates/validator/src/{coordinator.rs,execution_strategy.rs,validator.rs}`
- Spec: `specs/flashblocks/{p2p.md,p2p_v2.md}`

**Halt at stake:**
- The flashblocks executor or P2P state gets stuck, deadlocks, or buffers without limit. Pending state stops advancing.
- Worse, the stuck component is a critical task, and its exit takes the node down (see C07).

**Invariants / questions**
- The shared P2P state mutex is never held across blocking or long-running work, and lock order is consistent.
- The single-permit semaphore is always released, including on panic or cancellation.
- Buffered flashblocks and epochs are bounded, and are dropped on reorgs and new payloads.
- The event stream never waits forever on a canonical tip that isn't coming.
- Peer rotation, request timeouts and rate limits can't starve the receive set to zero while valid peers exist.

## C06 — Multi-builder publishing coordination & failover

**Files**
- `crates/p2p/src/protocol/handler.rs` (`PublishingStatus`, `start_publishing`, `stop_publishing`, `await_clearance`, `MAX_PUBLISH_WAIT_SEC`)
- `crates/p2p/src/protocol/connection.rs` (`handle_start_publish`, `handle_stop_publish`)
- `crates/rpc/src/engine.rs` (the forkchoice path that carries `Authorization`)
- `crates/payload/src/job.rs` (the wait for clearance before publishing)

**Halt at stake:** during HA failover or a double failover, no builder is ever cleared to publish, or two
builders publish at once. Flashblocks stop, or block production stalls behind `await_clearance` (whose
doc comment says it is "never guaranteed to return").

**Invariants / questions**
- From every reachable publishing state, a builder holding a fresh authorization always becomes cleared within a bounded time.
- Stale `StartPublish`/`StopPublish` messages, or messages from departed publishers, can't pin a node in `WaitingToPublish` or `NotPublishing`.
- Block production never depends on flashblock publishing clearance.

## C07 — Critical background tasks & shutdown semantics

**Files: every `spawn_critical*` site**
- `crates/node/src/pool.rs:139, 151` (pool maintenance)
- `crates/node/src/payload_service.rs:128, 150` (payload builder service)
- `crates/validator/src/coordinator.rs:200` (flashblocks executor)
- `crates/evm/src/collector.rs:23` (witness collector)
- `crates/node/src/proof_history.rs:137` (proof-storage metrics), plus the proof-history ExEx installed at `proof_history.rs`
- `crates/p2p/src/protocol/handler.rs` (rotation loop and trust-resolution tasks spawned with `tokio::spawn`)

**Halt at stake:** when a critical task exits or panics, the whole node shuts down. If the trigger is
external input, every node that receives that input goes down at once.

**Invariants / questions**
- No critical task can return or panic because of peer, RPC, transaction or chain input.
- Errors inside critical loops are handled and the loop continues.
- The proof-history ExEx can't stall block import through back-pressure or storage errors.
- Channels feeding critical tasks are bounded and never make the producer block.

## C08 — Node crash surface from untrusted input (`panic = "abort"`)

**Files**
- **P2P decoding and handling:** `crates/primitives/src/{p2p.rs,primitives.rs,flashblocks.rs,access_list.rs}`, `crates/p2p/src/protocol/{connection.rs,handler.rs,event.rs}`
- **RPC:** `crates/rpc/src/{simulate.rs,simulate_consts.rs,transactions.rs,sequencer.rs,witness.rs,admin.rs,engine.rs,eth/*.rs}`
- **Pool and PBH:** `crates/pool/src/{validator.rs,tx.rs,root.rs,ordering.rs,eip4337.rs}`, `crates/pbh/src/{payload.rs,external_nullifier.rs,date_marker.rs}`
- **Execution:** `crates/evm/src/**`, `crates/validator/src/**`, `crates/builder/src/**`
- **Build profile:** `Cargo.toml` (`[profile.maxperf] panic = "abort"`), `Dockerfile` (`PROFILE=maxperf`)

**Halt at stake:** any panic, unchecked arithmetic in a checked build, out-of-bounds index, or
unbounded allocation reachable from untrusted input kills the process. On the sequencer, that is H1.

**Invariants / questions**
- No `unwrap`, `expect`, `panic!`, `unreachable!`, `unimplemented!`, slice index or `as`-cast truncation is reachable from untrusted input without a prior check.
- Every decode of untrusted bytes has a length bound before it allocates.
- Mutexes can't be poisoned by input-driven panics (moot under `abort`, but relevant if the profile changes).
- Decide deliberately between `panic = "abort"` and `unwind` for the node binary.

## C09 — Resource exhaustion in admission & block production

**Files**
- `crates/pool/src/{validator.rs,root.rs,ordering.rs,tx.rs}` (per-transaction cost of PBH proof verification, root cache, priority ordering)
- `crates/pbh/src/payload.rs` (semaphore verification)
- `crates/node/src/{pool.rs,tx_propagation.rs}`
- `crates/rpc/src/simulate.rs` (CPU and memory per call)
- `crates/evm/src/{collector.rs,cache.rs}` (witness channel and cache bounds)
- `crates/p2p/src/protocol/recorder.rs` (recorder database growth)
- `crates/node/src/proof_history.rs` (proof-history storage window)
- `crates/cli/src/cli.rs` (limit defaults)

**Halt at stake:** cheap inputs force expensive work (proof verification, simulation, re-validation) or
unbounded growth. The node falls behind, the builder misses slots, or storage fills up.

**Invariants / questions**
- Expensive admission checks run after cheap ones, and are bounded or rate-limited per sender and peer.
- Priority ordering can't let invalid or reverting priority transactions monopolise the verified blockspace in every block.
- Every cache, channel, buffer and database has a size or retention bound with a safe default.
- Public RPC defaults can't consume the CPU the builder needs.

## C10 — PBH & builder-funded transaction interplay

**Files**
- `crates/builder/src/execution_context.rs` (`spend_nullifiers_tx`, gas reservation, builder nonce and balance)
- `crates/pool/src/validator.rs` (PBH gas limit, `numPbhPerMonth` storage reads)
- `C/src/pbh/PBHEntryPointImplV1.sol` (`spendNullifierHashes`, `pbhGasLimit`, `authorizedBuilder`)
- `crates/node/src/context.rs` (builder key wiring)

**Halt at stake:** a failure while constructing or executing the builder's own transaction aborts the
build, or PBH gas accounting makes every build fail. Block production stops, or PBH lanes are starved
permanently.

**Invariants / questions**
- Failures on the builder-transaction path, including construction errors (not just execution errors), never abort the payload.
- A builder with no balance, a revoked `authorizedBuilder`, or a nonce gap degrades gracefully.
- `pbhGasLimit` and the verified-blockspace capacity stay valid when the block gas limit changes.

## C11 — Hardfork activation & chainspec consistency

**Files**
- `crates/chainspec/src/{spec.rs,hardfork.rs,builder.rs,lib.rs}`
- `crates/cli/src/chainspec.rs`
- `crates/evm/src/lib.rs` (spec selection)
- `proofs/measured/core/src/range.rs` (`WorldRangeHardforkConfig`)
- `proofs/measured/kona-client/src/precompiles/factory.rs` (Tropo/Strato → Karst mapping)

**Halt at stake:** nodes, the builder and the proof program disagree about which fork is active at a
timestamp. At activation the network splits (H2), derivation stalls (H3), and honest roots become
unprovable (H4).

**Invariants / questions**
- Every role derives the same activation for every fork from the same genesis or rollup config.
- Fork ordering is enforced (see `finding.md` #10).
- Hard-coded default activation times match the registry the proof program uses.
- Missing or partial fork entries fail closed at startup instead of diverging at runtime.

## C12 — Settlement: game creation, resolution & anchor advance

**Files**
- `C/src/dispute/{MultiProofGame.sol,ERC20StakingVault.sol}`
- `C/src/dispute/lib/{LibProof.sol,GameTypes.sol,Errors.sol}`
- `C/src/dispute/interfaces/{IMultiProofGame.sol,IERC20StakingVault.sol}`
- Upstream references: `C/lib/optimism/packages/contracts-bedrock/src/dispute/{AnchorStateRegistry.sol,DisputeGameFactory.sol}`, `.../src/L1/OptimismPortal2.sol`
- `proofs/protocol/src/{lineage.rs,proof_game.rs,types.rs}`
- `proofs/services/proposer/src/{proposer.rs,bond_manager.rs,alloy.rs,config.rs}`
- `proofs/services/challenger/src/{challenger.rs,resolution_manager.rs,bond_manager.rs}`
- `proofs/services/defender/src/{defender.rs,lane.rs,game.rs}`
- Spec: `wips/wip-1006.md`, `pkg/contracts/README.md`

**Halt at stake:**
- No valid parent exists for new games (see `finding.md` #9).
- Games can't resolve, because a parent never resolves.
- `closeGame` can't settle.
- The anchor can't advance.
- The proposer stalls (see `finding.md` #13).
- The vault blocks bond locking.

Any of these freezes withdrawals.

**Invariants / questions**
- Some valid parent always exists for the next proposal in every reachable registry state, including after pause, blacklist, retirement or an implementation rotation.
- Every game eventually becomes resolvable. No parent chain can block its descendants forever.
- `closeGame` can always settle after finality. Every vault revert condition (owner alignment, pause, payout total) is recoverable.
- Timestamp and block-number bounds (`_toTimestamp`, the `uint64` limits, `blockInterval`) can't make every future `initialize` revert.
- The proposer, challenger and defender recover from every reachable on-chain state without manual database edits.

## C13 — Proof-generation liveness

**Files**
- `proofs/measured/sp1-programs/{range-ethereum/src/main.rs,range-utils/src/lib.rs,aggregation/src/main.rs}`
- `proofs/measured/core/src/**`, `proofs/measured/kona-client/src/**`
- `proofs/measured/nitro-enclave/src/{enclave.rs,protocol.rs,host.rs}`
- `proofs/backends/sp1/host/src/**`, `proofs/workers/{core,sp1,nitro}/src/**`
- `proofs/services/prover-service/src/**`
- `proofs/kona-host/src/{online.rs,lib.rs}`

**Halt at stake:** honest blocks can't be proven, because of an unsupported transaction or precompile,
cycle or memory limits, frame-size caps, witness-collection failures, or a vkey/PCR/config mismatch.
Honest games time out one after another, and settlement stalls (H4).

**Invariants / questions**
- Every block the node can produce can also be derived and proven by both lanes within the proof window, including at maximum gas, DA and blob usage.
- Witness and frame size limits exceed the worst-case block range.
- Job leasing and retry logic always makes progress: no job can be stuck forever, and no queue is starved.
- Config rotations (vkeys, PCRs, rollup config) can't leave in-flight games unprovable (see `finding.md` #6).

## C14 — Configuration, operator parameters & deployment

**Files**
- `crates/cli/src/{cli.rs,app.rs,config.rs,chainspec.rs,cli/builder.rs,cli/p2p.rs,cli/pbh.rs}`
- `crates/node/src/context.rs` (startup `expect`s: flashblocks args, authorizer key)
- `C/src/pbh/PBHEntryPointImplV1.sol` setters (`setPBHGasLimit`, `setNumPbhPerMonth`, `setWorldId`, builder add/remove)
- `C/scripts/devnet/{DeployProofSystem.s.sol,ActivateProofSystem.s.sol,DeployNitro.s.sol}`
- `C/scripts/mainnet/{Deploy.s.sol,DeployUpgrade.s.sol,DeployFeeVaults.s.sol}`
- `Dockerfile`, `Cargo.toml` profiles

**Halt at stake:** a documented configuration value, operator setter or deployment step leaves the
sequencer, followers or the settlement layer unable to make progress. For example, a startup `expect` on
an optional flag, a setter value that makes every build fail, or an activation step that makes every game
creation revert.

**Invariants / questions**
- Every documented flag combination either starts a working node or fails fast with a clear error. None fails later at runtime.
- Every admin setter validates its input against the constraints of the live chain.
- Deployment and activation scripts check the invariants C12 relies on before switching over.

---

## Cross-lens interactions (check after the individual lenses)

| Lenses | Interaction checked |
|---|---|
| C01 × C10 | Builder-side transaction failures vs payload completion |
| C02 × C03 × C11 | Builder-produced block vs validator, import and proof-program acceptance at fork boundaries |
| C04 × C05 × C07 | Engine responsiveness vs flashblocks coordinator locks and critical-task exits |
| C06 × C01 | Publishing clearance vs block production during failover |
| C08 × C07 | Any input-reachable panic on a critical task or the sequencer |
| C12 × C13 | Proof-generation failures vs game timeouts, retries and parent validity |
| C14 × all | Configuration and operator actions that move a lens into a halted state |
