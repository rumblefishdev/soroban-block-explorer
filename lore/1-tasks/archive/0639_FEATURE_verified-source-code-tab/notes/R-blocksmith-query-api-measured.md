# R: BlockSmith Query API — measured on its staging host (2026-10-14)

All reads, unauthenticated, against `https://staging.sv.blocksmith.co`.

## Verified population

`GET /api/query/v1/wasms?limit=200&order=desc` → one page, 33 records, no
cursor. `agreement`: 17 `single`, 16 `unanimous_verified`. Statuses over
49 verification records: 46 `verified`, 3 `mismatched`. Two verifiers are
listed by `GET /api/query/v1/verifiers`: `scvs-staging` and `scvs-dev`.

Per Wasm hash:

| Class                         | Hashes |
| ----------------------------- | ------ |
| verified, public source       | 13     |
| verified, source not public   | 17     |
| only `mismatched` (with code) | 3      |

All 33 hashes are deployed in our `testnet` database
(`testnet.soroban_contracts FINAL WHERE wasm_hash IN (…)`); 6 of them also
in `default`. Testnet holds 49,455 distinct Wasm programs
(`testnet.wasm_programs`), mainnet 5,420.

## Example contracts (testnet), checked on `/contracts/testnet/{id}`

| Case                       | Contract                                                   | Answer                               |
| -------------------------- | ---------------------------------------------------------- | ------------------------------------ |
| verified, public source    | `CD37GB5NIHBI5EGUNIJXEHDNVDDG23HJAVSJTAKSMDQQVKJMBSK6E5EW` | `unanimous_verified`, `.tar.gz`      |
| verified, public source    | `CBQYEADFBVU6ANYZXCPYFID5PBXV7FCDWWPIZPZLSAIFJRLF74S3YGFP` | `single`, `.tar.gz`                  |
| verified, no public source | `CCZOTIJCHZLWCOBZMSYNENXTBH6TVTP3LBEAXYVIIL5UCBGARLYM2XMH` | `unanimous_verified`, no source URIs |
| mismatched                 | `CDG3OR5ORUWA52GJFHH2XNMVXHEXOYUBFTEKMA44T3FFNYZAO73QRIEP` | `single`, `mismatched`               |
| Wasm, never verified       | `CDRNFRL3NPOKXYE3QIV3UW43LTF45VZ2B24KNHFTEZLJRT2XBPCDEVEI` | 200, `no_data`, empty list           |
| SAC (no Wasm)              | `CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC` | 404 `contract_not_found`             |

## Public vs not public

Each record links a report (`results_urls`, `location:
verifier_self_storage`). In all 49 reports `source.is_public` is `true`
exactly when the aggregate record carries `verified_source_code_uris`.
The verifier policy (`/policy.json`) accepts two source modes,
`private_uploaded_tarball` and `source_uri`, with `publish_source_bytes:
true` for uploads. A record may still carry `source_uri` while
`is_public: false`; the explorer does not treat that as published source.

## Archives

21 source links, 7 distinct archives, every one HTTP 200,
`application/octet-stream`. Formats: `.tar.gz` and `.zip`. The file name
is the sha256 of the archive and equals the record's `source_sha256`
(`shasum -a 256`, 7 of 7). Largest: 376,238 B packed, 297 files (274
`.rs`), 1,463,534 B unpacked; the others 5–34 files. Every file is text
(`.rs`, `.toml`, `.lock`, `.json`, `.md`, `Makefile`, `.gitignore`).

Crates already in `Cargo.lock`: `zip` 3.0.0, `flate2` 1.1.9, `sha2`
0.10.9, `reqwest` 0.12.28; `tar` is not.

## Limits and headers

- `/wasms/{hash}`: `x-ratelimit-limit: 6000`, reset 60 s.
- `/contracts/{network}/{id}`: `x-ratelimit-limit: 600`, reset 60 s.
- Both: weak `etag`, `access-control-allow-origin: *`.
- No `logo_url` in any record. No production host answers yet
  (`sv.blocksmith.co` does not resolve); staging `/health` reports
  version `0.1.0`.

## Standard

SEP-58 (draft 0.6.0, updated 2026-07-15) defines the build fields
(`bldimg`, `bldopt`, `bldarg`, `source_sha256`, `source_uri`) and says
consumers SHOULD weigh results by verifier reputation and aggregate across
independent verifiers. It defines no verified / mismatched / disputed
states and no API — those are the Query API's.

## Summary carried over from the task file (2026-10-16)

- `verified_source_code_uris` is present exactly when `is_public` (49/49 records).
- Coverage: 33 of 49,455 testnet Wasm programs (0.07%).
