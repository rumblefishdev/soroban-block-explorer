-- Local enrichment-backfill smoke seed (lore task 0231).
--
-- Seeds a curated ~50 sep1-asset + ~50 nft candidates into a LOCAL ClickHouse
-- so `backfill-enrichment-runner` can be driven end-to-end (chunks, cursor,
-- pagination, fan-out + every reproducible outcome category). NOT for prod.
--
-- Surrogate ids (`accounts.id`, `assets.issuer_id`, `soroban_contracts.id`,
-- `nfts.contract_id`) are arbitrary join keys — the backfill never re-derives
-- them from a StrKey, it only joins on them. "Real" is driven by `account_id`
-- (the real issuer StrKey, so the resolver matches CURRENCIES) + `home_domain`.
--
-- Outcome coverage (see docs/runbooks/0231_enrichment-backfill-local.md):
--   9 real across 5 issuers/domains (live TOML): USDC (centre.io),
--   AQUA + ICE/dICE/governICE/gdICE (aqua.network), EURC (mykobo.co),
--   PEN + ARS (anclap.com). A row may be real-partial (icon '' if its
--   CURRENCIES entry omits `image`) — the run reveals which.
--   900010 NOHOME                           → F no home_domain   → sentinel (no fetch)
--   900011 D404 (example.com)               → G domain 404       → sentinel (fetch)
--   900012 NOTREAL (centre.io, bogus code)  → H no CURRENCIES     → sentinel (fetch)
--   900013 TMOUT (localhost:1)              → unsafe hostname     → sentinel (SSRF guard)
--   E0000..E0042 (issuer absent)            → E missing issuer    → sentinel (no fetch)
--   asset_type 0 / 3                        → J excluded by candidate filter
-- NFT (nft_enrichment, 50 candidates):
--   900201 CDA5FGE4 (only verified mainnet Soroban NFT)  → real ("SorobanNFT")
--   900202 XLM SAC + 900203 game-contract CDL74RF5 (no token_uri) → RPC-fail → sentinel
--   920000.. (contract absent from soroban_contracts)    → lookup-miss sentinel (no RPC)
-- Not covered here (unit-tested only): transient (the SSRF guard rejects the
-- localhost trick → sentinel, not transient; a real timing-out host isn't
-- deterministic), B real-partial (AQUA/ICE turned out to carry images → full
-- real), C unsafe http:// image, D oversize, nft transient. See runbook
-- "Known gaps".
--
-- NOTE: ClickHouse VALUES does not allow trailing `--` comments on data rows.
-- Re-runnable: clears its own rows (id/issuer/contract in 900000..999999) first.

ALTER TABLE accounts            DELETE WHERE id BETWEEN 900000 AND 999999;
ALTER TABLE assets              DELETE WHERE issuer_id BETWEEN 900000 AND 999999;
ALTER TABLE asset_enrichment    DELETE WHERE issuer_id BETWEEN 900000 AND 999999;
ALTER TABLE soroban_contracts   DELETE WHERE id BETWEEN 900000 AND 999999;
ALTER TABLE nfts                DELETE WHERE contract_id BETWEEN 900000 AND 999999;
ALTER TABLE nft_enrichment      DELETE WHERE contract_id BETWEEN 900000 AND 999999;

INSERT INTO accounts (id, account_id, first_seen_ledger, last_seen_ledger, sequence_number, home_domain) VALUES
  (900001, 'GA5ZSEJYB37JRC5AVCIA5MOP4RHTM335X2KGX3IHOJAPP5RE34K4KZVN', 0, 0, 0, 'centre.io'),
  (900002, 'GBNZILSTVQZ4R7IKQDGHYGY2QXL5QOFJYQMXPKWRRM5PAV7Y4M67AQUA', 0, 0, 0, 'aqua.network'),
  (900003, 'GAXSGZ2JM3LNWOO4WRGADISNMWO4HQLG4QBGUZRKH5ZHL3EQBGX73ICE', 0, 0, 0, 'aqua.network'),
  (900004, 'GAQRF3UGHBT6JYQZ7YSUYCIYWAF4T2SAA5237Q5LIQYJOHHFAWDXZ7NM', 0, 0, 0, 'mykobo.co'),
  (900005, 'GA4TDPNUCZPTOHB3TKUYMDCRVATXKEADH7ZEYEBWJKQKE2UBFCYNBPEN', 0, 0, 0, 'anclap.com'),
  (900006, 'GCYE7C77EB5AWAA25R5XMWNI2EDOKTTFTTPZKM2SR5DI4B4WFD52DARS', 0, 0, 0, 'anclap.com'),
  (900010, 'GSEEDFNOHOMEDOMAIN00000000000000000000000000000000000001', 0, 0, 0, NULL),
  (900011, 'GSEEDGDOMAIN404000000000000000000000000000000000000000002', 0, 0, 0, 'example.com'),
  (900012, 'GSEEDHNOCURRENCY000000000000000000000000000000000000000003', 0, 0, 0, 'centre.io'),
  (900013, 'GSEEDITRANSIENT0000000000000000000000000000000000000000004', 0, 0, 0, 'localhost:1');

INSERT INTO assets (asset_type, asset_code, issuer_id, contract_id, name, total_supply, holder_count, icon_url) VALUES
  (1, 'USDC',      900001, 0, NULL, NULL, NULL, NULL),
  (1, 'AQUA',      900002, 0, NULL, NULL, NULL, NULL),
  (1, 'ICE',       900003, 0, NULL, NULL, NULL, NULL),
  (1, 'dICE',      900003, 0, NULL, NULL, NULL, NULL),
  (1, 'governICE', 900003, 0, NULL, NULL, NULL, NULL),
  (1, 'gdICE',     900003, 0, NULL, NULL, NULL, NULL),
  (1, 'EURC',      900004, 0, NULL, NULL, NULL, NULL),
  (1, 'PEN',       900005, 0, NULL, NULL, NULL, NULL),
  (1, 'ARS',       900006, 0, NULL, NULL, NULL, NULL),
  (1, 'NOHOME',  900010, 0, NULL, NULL, NULL, NULL),
  (1, 'D404',    900011, 0, NULL, NULL, NULL, NULL),
  (1, 'NOTREAL', 900012, 0, NULL, NULL, NULL, NULL),
  (1, 'TMOUT',   900013, 0, NULL, NULL, NULL, NULL),
  (0, 'XLM',     900098, 0, NULL, NULL, NULL, NULL),
  (3, 'SORO',    900099, 900099, NULL, NULL, NULL, NULL);

INSERT INTO assets (asset_type, asset_code, issuer_id, contract_id, name, total_supply, holder_count, icon_url)
SELECT 1, concat('E', leftPad(toString(number), 4, '0')), 910000 + number, 0, NULL, NULL, NULL, NULL
FROM numbers(37);

INSERT INTO soroban_contracts (id, contract_id, wasm_uploaded_at_ledger, is_sac) VALUES
  (900201, 'CDA5FGE4LZP4S45LP6AJLWMLKWHVWMKFSIKVYEBSIYOB25NWLKCLL7RY', 0, false),
  (900202, 'CAS3J7GYLGXMF6TDJBBYYSE3HQ6BBSMLNUQ34T6TZMYMW2EVH34XOWMA', 0, true),
  (900203, 'CDL74RF5BLYR2YBLCCI7F5FB6TPSCLKEJUBSD2RSVWZ4YHF3VMFAIGWA', 0, false);

INSERT INTO nfts (contract_id, token_id) VALUES
  (900201, '1'),
  (900202, '1'),
  (900202, '2'),
  (900203, '1');

INSERT INTO nfts (contract_id, token_id)
SELECT 920000 + number, '1' FROM numbers(46);
