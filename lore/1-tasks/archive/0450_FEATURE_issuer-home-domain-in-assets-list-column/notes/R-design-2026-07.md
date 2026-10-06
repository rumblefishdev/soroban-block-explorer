# Issuer home domain on the assets list — design analysis (2026-07)

Moved out of the task README when the task was archived.

### Why it is nearly free

The list resolves its page's issuers through a bloom-pruned `accounts.id`
key-seek rather than a join (0319, 0334), and that seek **already selects
`home_domain`**:

- `crates/api/src/assets/queries.rs:290-304` — `seek_latest_account` selects
  `id, account_id, home_domain`, taking the newest row by `last_seen_ledger`
  because `home_domain` is mutable via `SET_OPTIONS`.
- `crates/api/src/assets/queries.rs:777-790` — the list page resolves all its
  issuers via `resolve_page_issuers`, run concurrently with hydration under one
  `tokio::join!` (0364).
- `crates/api/src/assets/queries.rs:254-277` — `list_row_to_asset_row` maps the
  result onto `AssetRow.issuer_home_domain`, for list rows and detail alike.

Then it stops. `issuer_home_domain` appears nowhere in
`crates/api/src/assets/dto.rs` and nowhere in `libs/api-types/src/openapi.json`;
its only consumer is `crates/api/src/assets/handlers.rs:262`, which uses it
internally to drive the SEP-1 lookup behind the detail page's `description`. The
frontend has never seen the field.

### We already show a domain in two places — and from two different sources

This is not a new UI idea, and that is the complication.

| Surface          | Shows a domain?                                                                             | Source                                                                       |
| ---------------- | ------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| Accounts list    | yes, linked chip beside the address (`web/src/pages/accounts/AccountsTable.tsx:30-48`)      | `accounts.home_domain` — **on-chain**                                        |
| Asset **detail** | yes, a `Domain` row plus a `Homepage` link (`web/src/pages/assets/AssetMetadata.tsx:38-41`) | hostname of `home_page`, `www.` stripped — **SEP-1 `DOCUMENTATION.ORG_URL`** |
| Asset **list**   | no                                                                                          | —                                                                            |

**The two sources can disagree.** They usually match, because the toml is served
from the home domain — but nothing forces an org to put its own home domain in
`ORG_URL`. Shipping the list against one source while the detail page shows the
other, both labelled "Domain", would be an inconsistency we introduced.

**The list has no choice of source.** The detail page's value comes from
fetching the issuer's `stellar.toml` during the request
(`crates/api/src/assets/handlers.rs:262-277`). Twenty list rows would mean
twenty third-party HTTP fetches per page load — not an option. The list must use
`home_domain`, which is already read per row (see below).

So decide, and record the decision here before implementing:

- **Preferred:** the list shows `home_domain`, and the detail page gains it too
  as the primary `Domain`, demoting the `ORG_URL` hostname to part of the
  `Homepage` link it came from. One meaning of "Domain" across the product: what
  the issuer declared on-chain.
- **Alternative:** keep them distinct and label them differently, which needs
  wording a user can act on — harder than it sounds, and probably not worth it.

Whichever wins, the presentation comes from the accounts-list cell — see
"Staying consistent" below for how.

### The target column does not always show an issuer

Bigger than the missing chip, and it has to be settled first. The
"Issuer / Contract ID" cell is polymorphic
(`web/src/pages/assets/AssetsTable.tsx:75-88`), and its branches are ordered:

```
contract_id      → contract  (Soroban-native asset — the contract IS the asset)
sac_contract_id  → contract  (SAC facet of a CLASSIC asset)   ← before the issuer
issuer           → issuer G…
—                → dash
```

The SAC branch wins over the issuer branch. So **for any classic asset with an
observed SAC, the column shows a `C…` contract address and never shows the
issuer at all** — and USDC, the reporter's own example, has one. The request
described `G… - Centre.io`; that row showed neither half.

Calling this a bug overstates it. The branch arrived with 0339, which collapsed
the classic↔SAC two-row split into one row — before that a SAC was its own asset
whose identity _was_ the contract, so keeping the contract visible preserved
what the old row showed. The comment written at the time justifies the _linking_
rule, not the _precedence_. It is undocumented precedence with a consequence
nobody weighed, not a mistake.

So the column has to be decided before the chip means anything:

- **Both.** The cell shows the issuer (with its domain chip) _and_ the SAC
  contract. Most informative, and it stops the SAC facet hiding the issuer — but
  it is three things in one cell and the width question below gets harder.
- **Issuer first, contract second.** Reorder so `issuer` wins for classic assets
  and the SAC contract moves to its own column or a secondary line. Matches what
  the column header promises ("Issuer / Contract ID") for classic assets.
- **Split the column.** Honest but the widest change.

Whatever wins, Soroban-native assets genuinely have no issuer and must keep
showing the contract with no chip.

### What a "reserved SAC" actually is (investigated 2026-07-28)

The issuer column was not the only surface reading the wrong field, and the
reason it looked chaotic is worth writing down.

**Every classic asset has a SAC address.** It is derived from
`(code, issuer, network)` and needs nobody's permission to exist. Nothing is
"reserved" by anyone.

**We only learn about one when the asset emits a CAP-67 unified asset event** —
`transfer` / `mint` / `burn` / `clawback` / `set_authorized`
(`crates/xdr-parser/src/sac.rs:189-190`). Since CAP-67, ordinary classic
transfers emit these under the asset's derived SAC address whether or not a SAC
was ever deployed. `detect_undeployed_sac_overrides` proves
`emitter == derive_sac(asset)` and records the handle with `sac_deployed = false`
so the activity has somewhere to live (task 0323).

So **`sac_contract_id != 0` means "this asset has moved", not "this asset has a
contract"** — which is why two otherwise-identical classic assets disagreed on
screen with no visible reason: one had activity, the other did not. Verified on
a live example: `CC774ZITP2FCKQ3RACDQPZKCQXXFNJBSNG4VJ6PDNEI4REO6EZCEUP67` is
absent from the ledger per both a hand-built `getLedgerEntries` RPC call
(`entries: []`) and stellar.expert (404).

### Where the handle leaked, and where it did not

| Surface                         | Keyed on                                      | Saw reserved | Outcome                                                                     |
| ------------------------------- | --------------------------------------------- | ------------ | --------------------------------------------------------------------------- |
| Assets-list issuer column       | `sac_contract_id`                             | yes          | showed an unlinked `C…` — fixed by this task                                |
| Asset-detail "SAC contract" row | `sac_contract_id`                             | yes          | showed the address + "Reserved address — not deployed" — fixed by this task |
| `Has SAC` filter                | `max(sac_deployed)`                           | no           | already correct                                                             |
| `SAC` chip                      | `sac_deployed`                                | no           | already correct                                                             |
| Search                          | `soroban_contracts` only                      | no           | a reserved address is unfindable, correctly                                 |
| `soroban_contracts`             | overrides suppressed (`persist/stage.rs:384`) | no           | contract counts not inflated, by design                                     |
| Liquidity-pool legs             | `LEFT JOIN soroban_contracts`                 | no           | misses to null — **safe by accident**                                       |

That last row is the one to watch: the LP path is protected only because it
resolves through `soroban_contracts`, which reserved handles never enter. Anyone
repointing it at `asset_sac` directly — as the assets path does — reintroduces
this.

All four user-facing surfaces now mean one thing by "SAC": a deployed contract.

### Scope

1. Add `issuer_home_domain: Option<String>` to the assets **list** item DTO
   (the detail DTO may want it too — check before assuming).
2. Regenerate API types.
3. `web/src/pages/assets/AssetsTable.tsx:69-89` — resolve the column question
   above, then render the domain beside the issuer. The StrKey stays the
   copyable canonical value.

### Staying consistent with the accounts list

The accounts list is the reference implementation
(`web/src/pages/accounts/AccountsTable.tsx:14-52`). Four details it settled that
this task must not re-litigate or silently diverge from:

- **Column width.** Accounts uses `width: 240` with the reason in a comment:
  wider than a plain identifier _because_ the cell carries the chip next to the
  address and copy button. The assets issuer column is `width: 160`, sized for
  the identifier alone — **it has to grow**. There is room: the assets table has
  only four columns (240 / 160 / 150 / 110). Match 240 rather than inventing a
  third number.
- **Extract, don't copy.** Two tables rendering "identifier + domain chip" by
  copy-paste will drift on the next change to either. Lift the accounts cell
  into a shared component and have both use it — the same move already made for
  the liquidity-pool components under `web/src/pages/pool-shared/`.
- **Same column, different tables — accept it.** Accounts reads `home_domain`
  from `accounts_recent`, the refreshable MV that is already deduped, so its
  freshness is the MV refresh interval (`crates/api/src/accounts/queries.rs:195-205`).
  Assets seek the raw `accounts` RMT with `ORDER BY last_seen_ledger DESC LIMIT 1`
  because the issuer is a _different_ row from the one being listed
  (`crates/api/src/assets/queries.rs:290-304`). Same source column, two access
  paths with different costs — do not try to unify the queries. The visible
  consequence is a divergence window after a `SET_OPTIONS` domain change, during
  which the accounts list shows the old value and the assets list the new one.
  Acceptable: a home domain changes about never.
- **Field name.** Accounts calls it `home_domain` on its own DTO
  (`crates/api/src/accounts/dto.rs:46`). On an asset the value belongs to the
  _issuer_, not the asset, so `issuer_home_domain` is the clearer name and
  matches the internal `AssetRow` field it comes from. Deliberate difference,
  not an oversight.
- **The link, verbatim.** On-chain domains carry no scheme, so accounts prefixes
  `https://` only when the stored value lacks one, and opens with
  `target="_blank" rel="noopener noreferrer"`. Keep exactly that — a bare
  `href={domain}` would resolve as a relative path.

**Out of scope:** an assets equivalent of the accounts `filter[with_domain]`
toggle. Nobody asked for it; note it here so its absence reads as a decision.

### Constraints

- **Not an identity claim.** `home_domain` is set by the account holder and is
  unverified on its own — an issuer can set any domain. Render it as a claim,
  never as a badge implying we checked it. (SEP-1 `TOML` reachability would be
  weak corroboration at best; out of scope here.)
- **Sparse.** Most issuers set no `home_domain`; the cell must look deliberate
  when the value is absent, not broken.
- The branch that renders a contract StrKey (soroban / SAC facet) has no issuer
  and must be left alone.
