# V — how the seed's output was audited against the chain

The method used on the 2026-08-24 dry-runs, recorded so the check can be
repeated and so its two dead ends are not re-discovered as findings. The
scripts themselves are not kept in the repo; this note is the specification
they implement.

## The rule that makes the numbers mean anything

RPC answers for **now**; the snapshot answers for the **checkpoint**. A bare
mismatch therefore proves nothing at all. Every comparison is gated on the
`lastModifiedLedgerSeq` the chain returns:

| chain's ledger vs the one we recorded | what it licenses                                                                                   |
| ------------------------------------- | -------------------------------------------------------------------------------------------------- |
| **equal**                             | the entry has not moved since we read it — the amount is a real claim and must agree to the stroop |
| **above**                             | the network churned afterwards; no amount claim is possible                                        |
| **below**                             | impossible; report as a defect                                                                     |

Without that gate the audit degenerates into counting ordinary churn, which is
exactly the mistake the verdict rule itself was making (see the README's
checkpoint-guard finding).

## Independence is the point

The primitives were re-implemented from the spec — StrKey (SEP-23 base32 +
CRC16), the `LedgerKey` XDR encoding, and the `AccountEntry` / `TrustLineEntry`
decoders — rather than reused from our Rust. Checking our decoder with our
decoder passes a shared misreading; a second implementation cannot.

The strongest single probe follows from that: build the trustline `LedgerKey`
from what the SNAPSHOT claims (code bytes + issuer) and ask `getLedgerEntries`.
One hit proves four things at once — the asset exists, the holder exists, the
holding exists, and the true balance — and the returned entry echoes the code
and issuer back for comparison.

## Two layers, different guarantees

- **Structural, 100% coverage, offline.** Identity/surrogate bijection over
  every stub, asset-code shape, CRC on every StrKey, signer and threshold
  ranges. Catches anything systematic in the decode without a network call.
- **Chain, sampled, per verdict bucket.** Present / absent / amount, gated as
  above. `missing_*` must be PRESENT, `closure_*` and `ghosts_*` ABSENT,
  `agree_*` PRESENT with a matching amount as the positive control.

## Two assertions that were wrong

Both flagged rows, both turned out to be the audit's mistake, both verified
against chain before being dropped:

- **"Every account must be able to sign."** A threshold above the total signer
  weight is the standard idiom for locking an account forever — a fixed-supply
  issuer proving it cannot issue more. Three samples decoded byte-identical to
  ours (`1/1/255/255` with no signers, `0/1/1/1` with no signers,
  `1/10/10/10` with one weight-1 signer). Now counted, not failed.
- **"A code starting `0x` is the hex fallback."** Real asset codes may start
  with `0x` (`0xmons`, `0x3`). Only an even-length all-hex tail is genuinely
  ambiguous, and `asset_code.rs` already documents that residual as accepted.

## What the method cannot tell you

- Sampling bounds a **systematic** defect, not a random one. A wrong derivation
  hits every row of its class, so a fixed sample per bucket finds it; fifty
  randomly corrupted rows in 45M it will not.
- **`ghosts_classic` is unreachable.** Those holders have no `accounts` row and
  no `AccountEntry` in the snapshot, so no source carries their StrKey and no
  ledger key can be built for them at all.
- The residual in the native-supply decomposition (claimable balances and
  contract-held XLM) is inferred, not enumerated. Tallying those two entry types
  from the snapshot would make it measured; that belongs with 0503/0504.
