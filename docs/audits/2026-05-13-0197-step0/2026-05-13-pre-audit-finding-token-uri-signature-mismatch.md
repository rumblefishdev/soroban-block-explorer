# Pre-audit finding: NFT enrichment worker hardcodes `token_uri(token_id)` signature, breaks on SEP-39 collection-wide URI contracts

**Date:** 2026-05-13
**Status:** **fix verified locally on the audit DB (2026-05-13)** — pending commit + PR + production rollout
**Source:** Step 0 mini-spike of task 0197, fixture test against real Soroban NFT
**Severity:** medium — worker silently fails for any NFT contract using collection-wide `token_uri()` convention.

## TL;DR

The NFT token-URI fetcher in
`crates/enrichment-shared/src/nft_token_uri/client.rs` always invokes
`token_uri(token_id)` with one argument. Real Soroban NFT contracts on
mainnet implement **two distinct conventions**:

1. **SEP-50 / OpenZeppelin** (modern, per-token): `token_uri(token_id) → String`
2. **SEP-39 / ERC-721 collection style** (older, collection-wide): `token_uri() → String`
   plus `token_image() → String`

Worker handles only #1. Any contract implementing #2 returns
`Error(WasmVm, UnexpectedSize): "VM call failed:
Func(MismatchingParameterLen)", token_uri` on every token — i.e.
every token in the collection fails forever.

## Repro

Live Soroban NFT contract on pubnet (James Bachini's "SorobanNFT", SEP-39
style, from <https://github.com/jamesbachini/Soroban-NFT>):

```
contract:   CDA5FGE4LZP4S45LP6AJLWMLKWHVWMKFSIKVYEBSIYOB25NWLKCLL7RY
name():     "SorobanNFT"
symbol():   "SBN"
token_uri():    "https://ipfs.io/ipfs/QmegWR31kiQcD9S2katTXKxracbAgLs2QLBRGruFW3NhXC"
token_image():  "https://ipfs.io/ipfs/QmeRHSYkR4aGRLQXaLmZiccwHw7cvctrB211DzxzuRiqW6"
owner_of(token_id: i128) → Address
```

CLI inspection of the deployed interface:

```bash
$ stellar contract invoke --id CDA5FGE4... --rpc-url https://mainnet.sorobanrpc.com \
    --network-passphrase "Public Global Stellar Network ; September 2015" \
    --source-account GDMTVHLW... --send=no -- --help

Commands:
  owner_of
  name
  symbol
  token_uri        # <-- takes NO arguments
  token_image
  is_approved
  transfer
  mint
  approve
  transfer_from
```

`token_uri` takes no arguments. The interface is real, the contract is
live (RPC responds, name/symbol return values).

Worker test against this contract:

```text
# DB seed: INSERT soroban_contracts (contract_id, contract_type=2, is_sac=false)
#          INSERT nfts (contract_id=..., token_id='1')
# Run: cargo run -p backfill-enrichment-runner -- nft-metadata --id <nft_id>

**Processed:** 1
**Succeeded:** 0
**Unreachable (transient):** 1
**Duration:** 184 ms

| id | error |
| --- | --- |
| 3146763 | soroban rpc: HostError: Error(WasmVm, UnexpectedSize)
| | Event log: contract:CDA5FGE4…, topics:[error, Error(WasmVm, UnexpectedSize)],
| | data:["VM call failed: Func(MismatchingParameterLen)", token_uri]
| | 1: topics:[fn_call, CDA5FGE4…, token_uri], data:1
```

The `fn_call` event log shows the worker passed `data:1` (the token_id)
to `token_uri`. The Soroban VM rejected the call because the function
expects 0 parameters and got 1.

## Root cause

File: `crates/enrichment-shared/src/nft_token_uri/client.rs`

The fetcher's `resolve()` invokes a `simulateTransaction` against the
contract, always with one argument (the `nfts.token_id` value). There is
no fallback to invoking with zero arguments and no inspection of the
contract's WASM spec to discriminate which convention applies.

Specifically, the SEP-50 spec (which the worker codifies) and SEP-39
spec (which Bachini's contract codifies) are not interchangeable:

| Spec                   | `token_uri` signature          | Returns                                                                         |
| ---------------------- | ------------------------------ | ------------------------------------------------------------------------------- |
| SEP-50                 | `token_uri(token_id) → String` | Per-token URI (typically points to per-token JSON)                              |
| SEP-39 / ERC-721-style | `token_uri() → String`         | Collection-wide base URI (per-token data derived elsewhere or in `token_image`) |

Worker treats every NFT as SEP-50.

## Impact

- Every SEP-39-style NFT in `nfts` table: 100% transient errors, 0%
  success, retry budget burnt forever.
- Production live worker: each affected row will retry through SQS
  until reaching the DLQ max-receive count, generate a DLQ alarm,
  consume operator attention — only to surface "contract function
  signature mismatch" which is fundamentally unfixable on the worker
  side without a code change.
- Compounds with Bug #3 (`looks_like_token_id` false positives): the
  587k fake-NFT rows would mostly land on `"symbol not found in slice
of strs"` (no `token_uri` at all), but any real SEP-39 NFT among
  them would land on this `MismatchingParameterLen` and still go
  permanent-classified-as-transient → DLQ.

## Secondary issue — `is_transient` misclassification

Same bug as the `"symbol not found"` case (already noted in
the NFT-false-positives finding). `Func(MismatchingParameterLen)` is
fundamentally **permanent** — the contract's function signature is
fixed. Worker should classify this as permanent → sentinel write,
not transient → retry.

## Proposed fix

Two options, ordered by quality:

### Option A — WASM-spec discrimination (clean fix)

Worker reads the contract's WASM interface spec once on first
encounter (cache by `wasm_hash`). For each token:

- If `token_uri` exported with 1 arg of type `u32`/`i128`/etc → call
  with `token_id` (SEP-50 path).
- If `token_uri` exported with 0 args → call without args, treat
  the response as collection-wide URI; also call `token_image()` for
  per-token image if exported.
- Otherwise → sentinel write.

This pairs cleanly with the WASM-classifier fix proposed in Bug #3.

### Option B — try / fallback (quick fix)

Worker tries `token_uri(token_id)` first. On
`Func(MismatchingParameterLen)` → retry with zero args. Cache the
result per contract so we don't probe twice.

Cheaper to implement, more brittle. Suggest Option A as primary.

### Common across both

Add `Func(MismatchingParameterLen)` (and any related
`InvalidInput` / `WasmVm UnexpectedSize` patterns) to permanent error
classification in `errors::is_transient`. They are fundamentally
permanent — function signature won't change.

## Verification of fix

After the fix, the same fixture above should:

1. Detect signature: 0 args.
2. Call `token_uri()`, get `"https://ipfs.io/ipfs/QmegWR31ki…"`.
3. (Optionally) call `token_image()`, get the image URI.
4. Fetch the IPFS metadata, extract `name` / `image` / `collection`.
5. Write `nfts.name`, `nfts.media_url`, `nfts.collection_name` for
   every token row pointing at this contract.

### Local verification on the audit DB (2026-05-13)

Implemented **Option B** (try / fallback) in
`crates/enrichment-shared/src/nft_token_uri/client.rs::fetch_uncached`
and `build_simulate_envelope` (now takes `Option<u32>` for the
`token_id` arg). Plus the secondary fix in
`crates/enrichment-shared/src/nft_token_uri/errors.rs::is_transient`
to classify `MismatchingParameterLen` / `"symbol not found in slice of
strs"` / `Error(WasmVm, UnexpectedSize)` patterns as **permanent**.

Re-ran `backfill-enrichment-runner -- nft-metadata --id 3146763`
against the same Bachini fixture row:

```text
**Processed:** 1
**Succeeded (incl. sentinel writes):** 1
**Unreachable (transient, retry candidate):** 0
**DB failures:** 0
**Duration:** 897 ms

✓ All processed rows reached a terminal outcome.
```

Post-drain row in DB:

```sql
SELECT id, contract_id, token_id, name, media_url, collection_name FROM nfts WHERE id = 3146763;
--    id   | contract_id | token_id |    name    | media_url | collection_name
-- 3146763 |     1706144 | 1        | SorobanNFT |           |
```

`nfts.name = "SorobanNFT"` — **real metadata extracted end-to-end**.
`media_url` / `collection_name` are empty because the Bachini IPFS
JSON uses non-standard field names (`url` instead of `image`, no
`collection` field — see the JSON dump below). That is the contract's
choice, not a worker bug.

Raw IPFS JSON fetched during the run:

```json
{
  "name": "SorobanNFT",
  "description": "A prototype Soroban NFT contract",
  "url": "ipfs://QmeRHSYkR4aGRLQXaLmZiccwHw7cvctrB211DzxzuRiqW6",
  "issuer": "GB2QDUX7OJZ64BBG2PIFIY3WKUCOSFQSP6QJ7MZ32NOYAJJJ3FBOXA36",
  "code": "SBN"
}
```

Status command diff:

| column                 | PRE (broken) NULL / sentinel | POST (fix) NULL / sentinel | populated (derived)              |
| ---------------------- | ---------------------------- | -------------------------- | -------------------------------- |
| `nfts.name`            | 1 087 653 / 931              | 1 087 652 / 931            | **0 → 1** ✅                     |
| `nfts.media_url`       | 1 087 653 / 931              | 1 087 652 / 932            | 0 → 0 (JSON has no `image`)      |
| `nfts.collection_name` | 1 087 653 / 931              | 1 087 652 / 932            | 0 → 0 (JSON has no `collection`) |

Two unit tests added for the secondary fix
(`is_transient` permanent classification of
`MismatchingParameterLen` and `"symbol not found in slice of strs"`
patterns). Run `cargo test -p enrichment-shared --tests` to exercise.

### Out of scope for this fix

- The non-standard Bachini JSON shape (`url` vs `image`) is **not**
  a worker bug. Worker correctly extracts `name` and leaves the
  others as sentinel. If we want to support that variant, it would
  be a separate `extract_columns` fallback (try `image` first then
  `url`) — captured here for visibility, not implemented.
- The clean fix (Option A — WASM-spec discrimination) is still the
  recommended long-term path. Option B (this fix) is fast,
  correct on the verification fixture, and unblocks any SEP-39
  contract on pubnet immediately.

## Future work — WASM-spec-driven dispatch (optimisation)

The current fix (Option B, try/fallback) is correct but spends one
extra RPC round-trip per SEP-39 token. A SEP-39 collection of N tokens
currently makes 2 × N `simulateTransaction` calls (try SEP-50 → fail
→ retry SEP-39); with WASM-spec dispatch it would make N calls.

To replace try/fallback with deterministic dispatch:

1. **Indexer must populate `wasm_interface_metadata.metadata` reliably.**
   At audit time the table is empty for ~100% of rows — a separate
   gap not yet tracked as its own finding. The classifier
   (`xdr-parser/src/classification.rs`) already inspects WASM specs
   when present, so the missing piece is the writer side, not the
   parser side.

2. **`xdr-parser::classification` must expose function arity**, not
   just presence-by-name. Today `classify_contract_from_wasm_spec`
   returns `ContractClassification::{Nft, Fungible, Other}` from a
   name-only match (any of `owner_of` / `token_uri` /
   `approve_for_all` / `get_approved` / `is_approved_for_all`). The
   selector for SEP-50 vs SEP-39 needs the count of `token_uri`'s
   declared parameters.

3. **Keep the try/fallback as a safety net** for contracts whose WASM
   bytecode is no longer reachable via RPC (state-pruning past the
   retention window — common for older contracts on pubnet).

The TODO is also written in-source above the
`simulate_token_uri_with_fallback` function, tagged
`TODO(audit-0197 follow-up)`. Priority: **low** — fallback is
functional, this is optimisation only.

If the team picks this up, suggested task title:

> **FEATURE: WASM-spec-driven `token_uri` signature dispatch (remove try/fallback hot path)**

Effort: M; depends on prerequisite (1) being shipped first.

## Audit context

This is **finding #5** in 0197 Step 0 mini-spike. Together with:

- #1 (classic-credit assets row missing)
- #2 (home_domain not backfilled)
- #3 (`looks_like_token_id` false positives — task 0118)
- #4 (`is_sac` not set for pre-existing SAC)

…it forms the pattern "real production data has more diversity than the
indexer / enricher assume". Fix proposed alongside #3 (WASM-spec
discrimination) since both want the same classifier infrastructure.

## Sample queries (reproducible)

```bash
# Inspect interface
stellar contract invoke --id CDA5FGE4LZP4S45LP6AJLWMLKWHVWMKFSIKVYEBSIYOB25NWLKCLL7RY \
    --rpc-url https://mainnet.sorobanrpc.com \
    --network-passphrase "Public Global Stellar Network ; September 2015" \
    --source-account GDMTVHLWJTHSUDMZVVMXXH6VJHA2ZV3HNG5LYNAZ6RTWB7GISM6PGTUV \
    --send=no -- --help

# Call zero-arg variant successfully
stellar contract invoke --id CDA5FGE4... ... -- token_uri
# → "https://ipfs.io/ipfs/QmegWR31kiQcD9S2katTXKxracbAgLs2QLBRGruFW3NhXC"

# Call one-arg variant fails
stellar contract invoke --id CDA5FGE4... ... -- token_uri --token_id 1
# → error: unexpected argument '--token_id' found
```
