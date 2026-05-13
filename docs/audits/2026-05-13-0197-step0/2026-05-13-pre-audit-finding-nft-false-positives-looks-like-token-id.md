# Pre-audit finding: `looks_like_token_id` accepts i128 amounts → all SAC transfers become fake NFTs

**Date:** 2026-05-13
**Status:** open
**Source:** Step 0 mini-spike of task 0197 (DB completeness audit), NFT enrichment drain
**Severity:** **critical** — produces ~99% false positives in `nfts` table at production scale.
**Related task:** 0118 (BUG: NFT false positives from fungible token transfers) — currently `blocked`, this finding is its concrete root cause.

## TL;DR

The NFT-event detector in `crates/xdr-parser/src/nft.rs` uses a
heuristic `looks_like_token_id(data)` that **accepts** any ScVal type
except `void` / `map` / `vec` / `error`. That includes `i128`, which is
the canonical fungible-token transfer amount type. Result: every SAC /
fungible-token `transfer` event in the Soroban ledger is classified as
an NFT transfer, and a row gets inserted into `nfts` with the
`amount` value masquerading as `token_id`.

In our 11k-ledger backfill: **587 067 rows in `nfts` are SAC
amount-transfers, not real NFTs**. The two top "NFT contracts" by row
count are `CAS3J7GY…` (SAC XLM wrapper, 136 376 rows) and `CCW67TSZ…`
(SAC USDC wrapper, 47 215 rows). Every numeric value in `nfts.token_id`
in our DB is a stroop amount, not an NFT identifier.

## Root cause

File: `crates/xdr-parser/src/nft.rs`

```rust
fn looks_like_token_id(data: &Value) -> bool {
    let type_str = data.get("type").and_then(|v| v.as_str()).unwrap_or("");
    !matches!(type_str, "void" | "map" | "vec" | "error")
}
```

The function comment in the same file explicitly admits this is a
placeholder:

> Definitive NFT vs fungible classification should use WASM spec
> analysis from `contract.rs` (task 0027 responsibility).

So the developer documented "this is wrong, use the WASM classifier
instead" but the integration was never completed.

The classifier in `crates/xdr-parser/src/classification.rs` already
distinguishes the two interfaces correctly via WASM spec analysis
(`ContractClassification::Nft` if `owner_of` / `token_uri` /
`approve_for_all` / `get_approved` / `is_approved_for_all` is exported).
SAC contracts (XLM wrapper, classic-credit wrappers) do not expose any
of these and would correctly classify as Fungible / Other — they just
never reach that gate because the event-stage `looks_like_token_id`
check fires first and accepts everything.

## Verification

```sql
-- Top "NFT" contracts in our DB:
SELECT c.contract_id, COUNT(n.id) AS nft_rows
FROM nfts n
JOIN soroban_contracts c ON c.id = n.contract_id
GROUP BY 1
ORDER BY 2 DESC
LIMIT 5;

-- Result:
--  CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA | 136376  -- SAC XLM
--  CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75 |  47215  -- SAC USDC
--  CBZVSNVB55ANF24QVJL2K5QCLOAB6XITGTGXYEAF6NPTXYKEJUYQOHFC |  39420  -- (also SAC by stellar.expert)
--  CCKCKCPHYVXQD4NECBFJTFSCU2AMSJGCNG4O6K4JVRE2BLPR7WNDBQIQ |  17751  -- (also SAC by stellar.expert)
--  CAUIKL3IYGMERDRUN6YSCLWVAKIFG5Q4YJHUKM4S4NJZQIA3BAS6OJPK |  17683  -- (also SAC by stellar.expert)
```

Cross-checked the top two against stellar.expert API:

```bash
curl https://api.stellar.expert/explorer/public/contract/CAS3J7GY...
# {"contract":"CAS3J7GY…","asset":"XLM",...}

curl https://api.stellar.expert/explorer/public/contract/CCW67TSZ...
# {"contract":"CCW67TSZ…","asset":"USDC-GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN-1",...}
```

`asset: "XLM"` / `asset: "USDC-..."` is stellar.expert's marker for SAC
wrappers — not NFTs.

## Downstream impact

When `backfill-enrichment-runner nft-metadata --limit 200` ran on the
seeded DB:

| Outcome                                                                                                | Count | Cause                                                                   |
| ------------------------------------------------------------------------------------------------------ | ----- | ----------------------------------------------------------------------- |
| Transient errors (mostly `Error(Value, InvalidInput): "symbol not found in slice of strs", token_uri`) | 68    | SAC contracts don't export `token_uri()` — Soroban RPC simulation fails |
| Sentinel writes (`name=''`, `media_url=''`, `collection_name=''`)                                      | 132   | Other failure modes (4xx from a faked URL, malformed JSON, etc.)        |
| Real metadata extracted                                                                                | **0** | All 200 sampled rows are SAC amount-transfers, not real NFTs            |

So the worker's "0 real NFT enrichments" result is purely a downstream
symptom of the false-positive flood.

## Secondary issue — `is_transient` classification (Bug #6)

In `crates/enrichment-shared/src/nft_token_uri/errors.rs::is_transient`,
**every** `NftTokenUriError::SorobanRpc` variant was classified as
transient, including `Error(Value, InvalidInput): "symbol not found in
slice of strs", token_uri`. That specific error is fundamentally
permanent — the contract will never export a function it doesn't have.
Classifying it as transient causes the production live worker to retry
the SQS message until DLQ, wasting retry budget.

### Fix verified locally on the audit DB (2026-05-13)

Patched `errors.rs::is_transient` to discriminate
`NftTokenUriError::SorobanRpc(msg)` by message content:

- `"MismatchingParameterLen"` → permanent
- `"symbol not found in slice of strs"` → permanent
- `"Error(WasmVm, UnexpectedSize)"` → permanent
- `"Error(Storage, MissingValue)"` → permanent (added in iteration 2,
  see measurement below — pruned-state contract instance won't return
  without redeployment)
- Otherwise → transient (default unchanged)

Three unit tests added to lock in the classification:

- `soroban_rpc_mismatching_parameter_len_is_permanent`
- `soroban_rpc_symbol_not_found_is_permanent`
- `soroban_rpc_storage_missing_value_is_permanent`

Combined with the Bug #5 fix (zero-arg `token_uri()` fallback), the
worker now:

1. Tries SEP-50 path with `token_id` argument first.
2. On `MismatchingParameterLen` → falls back to SEP-39 zero-arg path.
3. On `"symbol not found"` (false-positive NFTs from this finding) →
   classifies as permanent → writes sentinel → no SQS retry.
4. On `Error(Storage, MissingValue)` (pruned contract instance) →
   permanent → sentinel.

### Measurement on a 1000-row sample (`--force-retry`)

Same 1000-row sample re-processed across three fix iterations:

| Metric                      | PRE-fix | After #5 + #6 (initial patterns) | After + `Storage, MissingValue` pattern |
| --------------------------- | ------- | -------------------------------- | --------------------------------------- |
| Processed                   | 1000    | 1000                             | 1000                                    |
| Succeeded (sentinel + real) | 931     | 954                              | **1000** ✅                             |
| Transient (SQS retry)       | 69      | 46 (−33%)                        | **0** (−100%) ✅                        |
| DB failures                 | 0       | 0                                | 0                                       |
| Duration                    | 2348 ms | 2256 ms                          | 2204 ms                                 |

Every row reaches a terminal outcome. Zero SQS retry budget burnt
on this sample (which is dominated by Bug #3 false-positive SAC
contracts). At full pubnet scale (~1M fake-NFT rows) this prevents
roughly 70 000 false-retry cycles per drain pass.

Production impact when rolled out:

- SQS retry budget no longer burnt on false-positive NFTs.
- DLQ noise drops sharply (most permanent errors now never reach
  the retry path at all).
- Real SEP-39 collections (e.g. James Bachini SorobanNFT) now resolve
  end-to-end on first call.

## Proposed fix

Two-step:

1. **Replace `looks_like_token_id` with WASM classifier consultation.**
   When a `transfer` / `mint` / `burn` event arrives, look up the
   emitting contract's classification (cache the verdict per
   `wasm_hash`). Only proceed if `ContractClassification::Nft`.
   Drop everything else.

2. **Cleanup pass on existing `nfts` rows.** Add a
   `backfill-enrichment-runner nft-purge-false-positives` subcommand
   (or one-shot SQL via a new migration) that deletes `nfts` rows whose
   `contract_id` resolves to a non-NFT contract type. With the fix
   above in place, future ingests will not re-introduce them.

3. **Secondary:** add a `permanent` arm to `is_transient` for the
   "symbol not found in slice of strs" pattern.

## Audit context

This is **finding #3** during 0197 Step 0 mini-spike. Together with
finding #1 (classic-credit assets row missing) and finding #2
(home_domain not backfilled), it explains why the local audit produces
no meaningful POST-enrichment numbers for the NFT path: the entire 587k
population is fake.

Task 0118 is already in the lore as `blocked` — this finding
documents the **concrete root cause** (one line of code) and makes it
unblockable.
