-- Task 0217 — `nfts_pending` + `nft_ownership_pending` quarantine tables.
--
-- Routes NFT-candidate rows whose contract has not yet been definitively
-- classified (verdict `Other` or NULL — i.e. WASM not observed in the
-- current backfill window) to a dedicated quarantine instead of
-- permissive-inserting into the API-facing hot tables `nfts` /
-- `nft_ownership`. Hot tables stay clean by design — production
-- `/v1/nfts*` endpoints never read `_pending`.
--
-- Promotion is wired through the existing `reclassify_contracts_from_wasm`
-- UPDATE path (task 0118 Phase 2). The reclassify path only flips
-- contract_type via WASM observation, so the only verdict transitions
-- it handles are `Other → Nft` (promotes pending rows to hot) and
-- `Other → Fungible` (drops pending rows). `Token` (SAC) is classified
-- at deploy time and never re-evaluated via WASM, so it never reaches
-- the promotion hook; `Token`-classified contracts are dropped at
-- persist-filter time before they ever enter pending in the first place.
--
-- Design choices documented in the task notes
-- (`lore/1-tasks/active/0217_FEATURE_nfts-quarantine-table.md`):
--
-- * **No FK to `soroban_contracts`** — rows arrive before classification;
--   the lookup churn is wasted work on a by-design transient row.
-- * **No FK to `nfts.id` on the ownership side** — pending ownership rows
--   carry their own natural identity (contract_id, token_id, …) and only
--   adopt the hot `nft_id` on promotion. Avoids a SERIAL gap pile-up.
-- * **No partitioning** — pending is transient; the by-created_at range
--   pattern that drives `nft_ownership`'s partitioning has no
--   read-side payoff here (the only read is the per-contract_id
--   promotion lookup).
-- * **Minimal indexing** — single `(contract_id)` index; the table is
--   write-heavy and only read at promotion / drain time.

CREATE TABLE nfts_pending (
    contract_id           BIGINT       NOT NULL,  -- no FK: see header
    token_id              VARCHAR(256) NOT NULL,
    collection_name       VARCHAR(256),
    name                  VARCHAR(256),
    media_url             TEXT,
    minted_at_ledger      BIGINT,
    current_owner_id      BIGINT,                 -- no FK to accounts (transient)
    current_owner_ledger  BIGINT,
    PRIMARY KEY (contract_id, token_id)
);
CREATE INDEX idx_nfts_pending_contract ON nfts_pending(contract_id);

CREATE TABLE nft_ownership_pending (
    contract_id      BIGINT       NOT NULL,  -- no FK: see header
    token_id         VARCHAR(256) NOT NULL,
    transaction_id   BIGINT       NOT NULL,  -- no FK to transactions (transient)
    owner_id         BIGINT,                  -- no FK to accounts (transient)
    event_type       SMALLINT     NOT NULL,  -- domain::NftEventType
    ledger_sequence  BIGINT       NOT NULL,
    event_order      SMALLINT     NOT NULL,
    created_at       TIMESTAMPTZ  NOT NULL,
    PRIMARY KEY (contract_id, token_id, created_at, ledger_sequence, event_order),
    CONSTRAINT ck_nft_own_pending_event_type_range CHECK (event_type BETWEEN 0 AND 15)
);
CREATE INDEX idx_nft_ownership_pending_contract ON nft_ownership_pending(contract_id);
