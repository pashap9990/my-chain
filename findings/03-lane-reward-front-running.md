# Proof-lane rewards can be front-run, or burned to `address(0)`

**Severity:** Low
**Component:** `pkg/contracts/src/dispute/MultiProofGame.sol` (`submitProofLane`), `ERC20StakingVault.sol` (`settle`)
**Status:** Found by code review. Not yet confirmed with a PoC.

---

## Summary

When someone challenges a proposal and the proposer successfully defends it, the challenger's bond is
forfeited. Part of that bond goes to whoever submitted the proofs that won the defense. The idea is sound:
proving costs real money, so the people who do it should be paid.

The problem is that the contract doesn't really know who did the work. The reward address travels in the
same calldata as the proof, but nothing checks it. The SP1 proof, the TEE signature and the council
signature all vouch for the state transition. None of them vouch for the payout address.

So anyone who sees a proof submission waiting in the public mempool can copy the proof, put their own
address in the recipient field, and outbid the original transaction. The proof is still perfectly valid,
so the copy lands, the game counts the lane, and the copier collects the reward. The honest prover's
transaction then reverts as a duplicate.

A related problem: nothing stops the recipient from being `address(0)`. If it is, the reward is credited
to an account nobody can ever withdraw from, and those tokens are locked in the vault permanently.

The code already acknowledges the front-running risk in a comment (`MultiProofGame.sol:495-496`), which
recommends submitting through a private relay. That comment is the only protection today.

---

## Vulnerability Path

**1. The payload mixes the proof with an unverified recipient.**

`submitProofLane(bytes proof)` takes one packed blob (`LibProof.sol:49-50`, decoded at `:76-91`):

```
byte  0        lane id
bytes 1..20    reward recipient   <- never verified
bytes 21..     proof bytes        <- the only part the verifier checks
```

**2. The verifiers only check the proof against the game's statement.**

The game passes `compact.proof` to the lane's verifier together with the public values the game computes
itself (`MultiProofGame.sol:528-532`, `:841-853`):

- **Validity lane:** the SP1 proof is checked against `abi.encode(transition, rangeVKeyCommitment)`.
- **TEE lane:** the enclave signature is checked over `keccak256(abi.encode(transition))` (`NitroProofVerifier.sol:30`).
- **Council lane:** the Safe signature is checked over `rootId` (`SecurityCouncilVerifier.sol:34-36`).

The recipient appears in none of these.

**3. The game stores whatever recipient the caller wrote.**

```solidity
claimData.proofBitmap = claimData.proofBitmap.set(lane);
laneRecipient[compact.laneId] = compact.recipient;   // MultiProofGame.sol:535
```

**4. How the theft plays out on a public mempool:**

1. A proposal is challenged, and the proposer starts gathering proofs to defend it.
2. An honest prover pays for an SP1 proof and broadcasts `submitProofLane(lane | proverAddress | proof)`.
3. A front-running bot sees the pending transaction, swaps bytes 1..20 for its own address, and resubmits with a higher priority fee.
4. The bot's transaction is mined first. The proof verifies, the lane is marked, and `laneRecipient` becomes the bot.
5. The honest transaction reverts with `DuplicateProofLane`.
6. The game resolves `DEFENDER_WINS`. `_creditDefenderWins` credits the bot's share (`MultiProofGame.sol:627-643`).
7. After the finality delay, `closeGame()` settles the pot into the vault, and the bot withdraws once the vault's withdrawal delay has passed.

The bot can repeat this for every lane in the same game.

**5. The `address(0)` case.**

If the recipient bytes are all zero, because of a buggy submission tool or a griefer who only wants the
honest prover to go unpaid, the game credits `normalModeCredit[address(0)]`. `settle` then adds it to
`availableBalance[address(0)]` without complaint (`ERC20StakingVault.sol:209-211`).

Withdrawals are keyed on `msg.sender` (`ERC20StakingVault.sol:115-140`), and `address(0)` can never be
the sender. The vault also has no admin path for moving balances, by design. Those tokens are stuck for good.

---

## Impact

**Who loses:** the prover who actually generated and submitted the proof. For the validity lane, that
prover has usually paid for SP1 proving, so they are out both the proving cost and the reward.

**Who gains:** whoever front-runs the submission. It costs them only gas.

**How much:** the reward exists only when a game is challenged and then resolves `DEFENDER_WINS` with
`NORMAL` bond distribution. The challenger's bond is split evenly between each accepted lane and the
proposer:

```
per-lane reward = challengerBond / (acceptedLanes + 1)
```

With the typical two lanes (SP1 + TEE), each lane is worth one third of the challenger's bond. A bot that
front-runs both lanes takes two thirds. Unchallenged games, games the challenger wins, and games closed in
`REFUND` mode pay lane recipients nothing, so there is nothing to steal in those cases.

**What is not affected:**

- The copied proof counts exactly like the original, so the game still resolves correctly.
- The proposer's bond, the challenger's outcome, and bridge withdrawals are all untouched.
- No other participant's balance in the vault is affected.

**Longer-term effect:** if third-party provers learn that their rewards get sniped, fewer of them will
bother defending challenged games. The proposer still has its own bond at stake and will keep defending,
so this weakens the incentive design rather than the safety of the system.

---

## Mitigation

These are listed from cheapest to most thorough. They can be combined.

**1. Reject a zero recipient (do this regardless).**

This is a one-line change, and it removes the permanent-burn case entirely:

```solidity
CompactProof memory compact = proof.decodeCompact();
if (compact.recipient == address(0)) revert InvalidRecipient(); // new error
```

**2. Submit through a private relay (current guidance).**

This is what the code comment recommends. It works as long as the relay doesn't leak or reorder the
transaction, but it's an operational habit rather than a guarantee. Anyone who submits through the public
mempool is still exposed.

**3. Bind the recipient to the proof.**

This is the real fix: make the recipient part of what each verifier checks, so changing it invalidates
the proof.

- **Council lane:** add the recipient to the signed digest, for example
  `keccak256(abi.encode(ATTESTATION_TYPEHASH, block.chainid, address(this), rootId, recipient))`, and
  have the game pass it in the public values. This is the easiest of the three.
- **TEE lane:** have the enclave sign `(transition, recipient)` instead of `transition` alone, and have
  `NitroProofVerifier` check against the same message. This needs an enclave change, a new image (PCR0)
  and a verifier update.
- **SP1 lane:** add the recipient as a committed public value in the aggregation program. This needs a
  new aggregation vkey.

**4. Commit-reveal.**

The prover first posts `hash(proof, recipient, salt)`, then reveals the proof and recipient at least one
block later. The game accepts the reveal only if it matches an earlier commitment. This works for every
lane without touching the proof systems, but it adds latency inside the proof window and needs care to
avoid griefing with junk commitments.

**What won't work:** setting the recipient to `msg.sender`. The front-runner simply becomes
`msg.sender`, so the theft still succeeds.
