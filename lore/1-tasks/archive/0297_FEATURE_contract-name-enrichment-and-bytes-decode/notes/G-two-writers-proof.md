---
prefix: G
title: 'Proof: identity vs metadata are two independent writes → two tables (ADR 0049)'
status: mature
spawned_from: ['0297']
date: 2026-06-18
who: karolkow
---

# Proof: why `soroban_contract_metadata` is a separate table (chain-verified)

Reproducible evidence for the ADR 0049 decision. Read-only mainnet.

## The claim (the question, precisely)

A contract's row is built from **two independent write events**, which can land
in **different ledgers**:

1. **Identity** — the deploy: `wasm_hash`, `deployer`, `deployed_at_ledger`
   (→ `soroban_contracts`). `deployer`/`deployed_at_ledger` come from the deploy
   **transaction context**.
2. **Metadata** — `name` / `symbol` / `decimals`, written into the contract's
   **instance storage** under `Symbol("METADATA")` (→ `soroban_contract_metadata`).
   Set in the constructor (same ledger as deploy) OR a later `init()` / rename
   (a different ledger).

**The combined-table problem:** the metadata write (the instance ledger entry)
carries `executable` (= `wasm_hash`) + the whole storage map, **but NOT**
`deployer` / `deploy-ledger` — those are not in the ledger entry, they are the
deploy TX context. So:

- a **later** metadata write cannot reconstruct identity (no `deployer`);
- a deploy **without a constructor** carries no metadata yet;
- identity and metadata update on **different clocks** (RMT version).

On ClickHouse (`ReplacingMergeTree`, whole-row replace) a single combined row
therefore cannot hold both without one write clobbering the other's columns
(the "G5" bug class). → two tables, composed at read. This is the same shape as
the already-shipped `asset_enrichment` / `nft_enrichment` side tables (ADR 0048).

> Nuance: a constructor deploy _can_ carry identity + metadata together. The
> decision still holds because metadata may also arrive **separately/later**,
> and that write lacks the identity fields + has its own clock.

## Chain evidence (mainnet, 2026-06-18)

```
### liquidFi bridge (WASM token)  CDKRSOVB5KEBN37MN3RB2O75NUR4RLR6HIX5Q73MMRTZS7CSTAG3B4D2
  instance entry : executable=wasm:df437853c584…  storage_keys=[METADATA, ["Admin"]]
  METADATA (IN the instance entry): {"decimal":7,"name":"liquidFi bridge token","symbol":"lUSDC"}
  deployer (from the deploy TX, NOT in the instance entry): GCFVKIDAZE36XSVS2KWGKWFOAVVY6BUTFULTX54R4NCEQZ6MD7GWZ3V4
  instance lastModifiedLedger=55893610   created(unix)=1740478389

### USDC SAC  CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75
  instance entry : executable=stellar_asset  storage_keys=[METADATA, ["Admin"], ["AssetInfo"]]
  METADATA (IN the instance entry): {"decimal":7,"name":"USDC:GA5ZSEJYB37…","symbol":"USDC"}
  deployer (from the deploy TX, NOT in the instance entry): GDMTVHLWJTHSUDMZVVMXXH6VJHA2ZV3HNG5LYNAZ6RTWB7GISM6PGTUV
  instance lastModifiedLedger=62313360   created(unix)=1708482513
```

Reading: `METADATA` (name/symbol/decimals) is **inside** the instance entry;
`deployer` is **not** (it is the deploy TX context). The instance's
`lastModifiedLedger` is independent of creation — metadata can change on its own
clock. (Earlier in this task: an instance `updated` change carries the full
instance incl `executable` but never the TX-context deployer — see
[[S-onchain-metadata-location-chain-verified]] §Option B; the USDC SAC instance
was last modified ~796 days after creation.)

## How to reproduce

`@stellar/stellar-sdk@16`, endpoint `https://mainnet.sorobanrpc.com` +
`api.stellar.expert`. **Input** = the two contract IDs above. **Output** = the
block above. Script (`prove_two_writers.mjs`):

```js
import * as SDK from '@stellar/stellar-sdk';
const { Contract, scValToNative } = SDK;
const server = new SDK.rpc.Server('https://mainnet.sorobanrpc.com');
const safe = (_k, v) => (typeof v === 'bigint' ? v.toString() : v);

async function instanceEntry(cid) {
  const r = (await server.getLedgerEntries(new Contract(cid).getFootprint()))
    .entries?.[0];
  if (!r) return null;
  const inst = r.val.contractData().val().instance();
  const ex = inst.executable();
  const exec =
    ex.switch().name === 'contractExecutableWasm'
      ? 'wasm:' + Buffer.from(ex.wasmHash()).toString('hex').slice(0, 12) + '…'
      : ex.switch().name;
  const fields = [];
  let meta = null;
  for (const e of inst.storage() || []) {
    const k = scValToNative(e.key());
    fields.push(typeof k === 'string' ? k : JSON.stringify(k));
    if (k === 'METADATA') meta = scValToNative(e.val());
  }
  return { exec, fields, meta, lastModified: r.lastModifiedLedgerSeq };
}
async function deployInfo(cid) {
  const j = await (
    await fetch(`https://api.stellar.expert/explorer/public/contract/${cid}`)
  ).json();
  return { deployer: j.creator, createdUnix: j.created };
}
for (const [label, cid] of [
  [
    'liquidFi bridge (WASM token)',
    'CDKRSOVB5KEBN37MN3RB2O75NUR4RLR6HIX5Q73MMRTZS7CSTAG3B4D2',
  ],
  ['USDC SAC', 'CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75'],
]) {
  const i = await instanceEntry(cid);
  const d = await deployInfo(cid);
  console.log(`\n### ${label}\n${cid}`);
  console.log(
    `  instance entry : executable=${i.exec}  storage_keys=[${i.fields.join(
      ', '
    )}]`
  );
  console.log(
    `  METADATA (IN the instance entry): ${JSON.stringify(i.meta, safe)}`
  );
  console.log(
    `  deployer (from the deploy TX, NOT in the instance entry): ${d.deployer}`
  );
  console.log(
    `  instance lastModifiedLedger=${i.lastModified}   created(unix)=${d.createdUnix}`
  );
}
```

Run: `npm i @stellar/stellar-sdk@^16 && node prove_two_writers.mjs`.
