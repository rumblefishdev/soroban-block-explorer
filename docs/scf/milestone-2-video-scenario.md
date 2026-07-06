# Milestone 2 - Deliverable Verification Video Script

Purpose: record a short SCF deliverable verification video for Milestone 2:
Complete API + Frontend.

Target length: 4-5 minutes.

## Before recording

Prepare these values and keep the browser tabs open:

| Item                                                       | Value                                                                              |
| ---------------------------------------------------------- | ---------------------------------------------------------------------------------- |
| Frontend                                                   | `https://sorobanscan.rumblefish.dev`                                               |
| API                                                        | `https://api-sorobanscan.rumblefishdev.com/v1`                                     |
| Swagger UI                                                 | `https://api-sorobanscan.rumblefishdev.com/api-docs`                               |
| API key (on-screen demo; provided to reviewers on request) | `<API_KEY>`                                                                        |
| Ledger                                                     | `63300000`                                                                         |
| Transaction — decoded events (CAP-67)                      | `e584ff2d5c548b6302fba03354ca9a9f2fa13e3e133ea79db20c7eb2687b65c9`                 |
| Contract — invocations (KALE farm, ~17M invocations)       | `CDL74RF5BLYR2YBLCCI7F5FB6TPSCLKEJUBSD2RSVWZ4YHF3VMFAIGWA`                         |
| Invocation tx 1 (fn `plant`)                               | `e584ff2d5c548b6302fba03354ca9a9f2fa13e3e133ea79db20c7eb2687b65c9`                 |
| Invocation tx 2 (fn `plant`)                               | `97073c3121861dfc457bdee1353f69b864d265aef8a6e01026f64fb12f6ab84e`                 |
| Invocation tx 3 (fn `plant`)                               | `e5f56501b97cad1c350648ab3867f1fd19e78d4f6c649ff64388acc23f039f4d`                 |
| Account (for search)                                       | `GA6G524YPEHXE7M5ICIDKBHPXM4HO7TKNAZYMLCRLB33CUNLQKSIAJ3O`                         |
| Asset                                                      | `EURC-GDHU6WRG4IEQXM5NZ4BMPKOXHW76MZM4Y2IEMFDVXBSDP6SJY4ITNPP2`                    |
| NFT — enriched (name + image)                              | `CCAOCYALAGXC2NQMSB5TMNE56KMONZHR2U7DLOOXE7J4WJNPA7B2YPKS` / token `8` (PIKO PASS) |
| Liquidity pool (XLM / USDC)                                | `LCSGRVA5R2NY6PDSBFSRMCFXJN63PLEZKLOK4DG7ESDR2HM4PMAIQQUG`                         |

## Scene 1 - Intro and scope

SHOW: frontend home page, then Swagger UI.

SAY:

> Hi, I am <DEV_NAME> from Rumble Fish. This video verifies Milestone 2 of the
> Soroban Block Explorer: Complete API and Frontend. I will show the live React
> frontend, the public REST API, decoded Soroban contract data, CAP-67 events,
> and global search. The application reads from our own indexed mainnet data in
> ClickHouse; it does not use a third-party chain API on the read path.

## Scene 2 - Public frontend with live mainnet data

SHOW: home page, then top-level pages: Transactions, Ledgers, Accounts,
Contracts, Assets, NFTs, Liquidity Pools. Then a few detail pages with live data:
NFT `CCAOCYAL…/8` (PIKO PASS - decoded name + image), liquidity pool
`LCSGRVA5…` (XLM / USDC, with reserves), contract `CDL74RF5…` (KALE).

SAY:

> The frontend is publicly accessible at `sorobanscan.rumblefish.dev`. These are
> the main explorer pages required for Milestone 2. Each page is loading live
> mainnet data from the API: transactions, ledgers, accounts, contracts, assets,
> NFTs, and liquidity pools. I will open a few detail pages as well, so the
> reviewer can see that list and detail views are both connected to live data.

## Scene 3 - API surface and schema-valid responses

SHOW: Swagger UI, then a terminal with short curl calls.

Run or show:

```bash
API=https://api-sorobanscan.rumblefishdev.com/v1
KEY=<API_KEY>   # provided to reviewers on request

curl -sS -H "x-api-key: $KEY" "$API/ledgers/63300000" | jq .
curl -sS -H "x-api-key: $KEY" "$API/transactions/4f6993de613664af15f0a17c0fade885da931db3c57a326c5144cccc4178b984" | jq .
curl -sS -H "x-api-key: $KEY" "$API/accounts/GA6G524YPEHXE7M5ICIDKBHPXM4HO7TKNAZYMLCRLB33CUNLQKSIAJ3O" | jq .
curl -sS -H "x-api-key: $KEY" "$API/contracts/CDL74RF5BLYR2YBLCCI7F5FB6TPSCLKEJUBSD2RSVWZ4YHF3VMFAIGWA" | jq .
curl -sS -H "x-api-key: $KEY" "$API/assets/EURC-GDHU6WRG4IEQXM5NZ4BMPKOXHW76MZM4Y2IEMFDVXBSDP6SJY4ITNPP2" | jq .
curl -sS -H "x-api-key: $KEY" "$API/nfts/CCAOCYALAGXC2NQMSB5TMNE56KMONZHR2U7DLOOXE7J4WJNPA7B2YPKS/8" | jq .
curl -sS -H "x-api-key: $KEY" "$API/liquidity-pools/LCSGRVA5R2NY6PDSBFSRMCFXJN63PLEZKLOK4DG7ESDR2HM4PMAIQQUG" | jq .
```

SAY:

> This is the OpenAPI documentation for the deployed API. The API exposes the
> Milestone 2 entities: transactions, ledgers, accounts, contracts, assets,
> NFTs, liquidity pools, and search. Here I am calling representative endpoints
> with our access-controlled API key. The responses return HTTP 200 and match
> the published schema. We can run the same calls against any mainnet entity IDs
> the reviewer provides; an API key is available to reviewers on request.

## Scene 4 - Decoded Soroban invocations

SHOW: Contract Detail page for `CDL74RF5BLYR2YBLCCI7F5FB6TPSCLKEJUBSD2RSVWZ4YHF3VMFAIGWA`
(KALE farm) -> Invocations tab (the appearance index; ~17M invocations, 6,500
callers over 7 days). Click one invocation's transaction -> Transaction Detail ->
Advanced view, showing the decoded operation. Also show the matching API response
for `GET /v1/transactions/e584ff2d5c548b6302fba03354ca9a9f2fa13e3e133ea79db20c7eb2687b65c9`
(the `heavy.diagnostic_events` carry the decoded `fn_call` / `fn_return`).

SAY:

> Each contract's Invocations tab lists the calls made to it and links each one
> to its transaction. Opening that transaction and switching to the Advanced
> view shows the Soroban call decoded for humans: the function name - here
> `plant` - the decoded arguments, and the return value, not raw XDR. The
> transaction-detail API returns the same decoded operation data. For the
> submission evidence we use three known contract transactions:
> `bc67f27f…ae7b687b`, `97073c31…12f6ab84e`, and `e5f56501…3f039f4d`.

## Scene 5 - CAP-67 events on transaction detail

SHOW: transaction detail page for
`e584ff2d5c548b6302fba03354ca9a9f2fa13e3e133ea79db20c7eb2687b65c9`, Events tab
(the count badge shows the _recent_ window; open the tab to see the decoded
events), then matching API response for
`GET /v1/transactions/e584ff2d5c548b6302fba03354ca9a9f2fa13e3e133ea79db20c7eb2687b65c9`.

SAY:

> This transaction contains Soroban events. In the Events tab, CAP-67 topics and
> data are decoded into typed, readable fields - symbols, addresses, integers -
> rather than shown as raw XDR. The same decoded event data is available from the
> transaction detail API response (`heavy.diagnostic_events`), so the frontend and
> API evidence match.

## Scene 6 - Global search and wrap-up

SHOW: global search for exact transaction hash
(`4f6993de613664af15f0a17c0fade885da931db3c57a326c5144cccc4178b984`), account ID
(`GA6G524YPEHXE7M5ICIDKBHPXM4HO7TKNAZYMLCRLB33CUNLQKSIAJ3O`), and contract ID
(`CDL74RF5BLYR2YBLCCI7F5FB6TPSCLKEJUBSD2RSVWZ4YHF3VMFAIGWA`) - each routes to the
matching detail page.

SAY:

> Finally, global search routes exact identifiers to the correct detail pages.
> A transaction hash opens the transaction page, an account ID opens the account
> page, and a contract ID opens the contract page. That completes the Milestone
> 2 acceptance criteria: the REST API is live, the React frontend is public,
> pages render live mainnet data, Soroban invocations and events are decoded,
> and exact search works across the required entity types. Thanks for reviewing.
