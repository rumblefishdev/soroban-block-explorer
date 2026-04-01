CREATE TABLE soroban_invocations (
    id               BIGSERIAL,
    transaction_id   BIGINT NOT NULL,
    contract_id      VARCHAR(56),
    caller_account   VARCHAR(56),
    function_name    VARCHAR(100) NOT NULL,
    function_args    JSONB,
    return_value     JSONB,
    successful       BOOLEAN NOT NULL,
    ledger_sequence  BIGINT NOT NULL,
    created_at       TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (id, created_at),
    FOREIGN KEY (transaction_id) REFERENCES transactions(id) ON DELETE CASCADE,
    FOREIGN KEY (contract_id) REFERENCES soroban_contracts(contract_id)
) PARTITION BY RANGE (created_at);

CREATE INDEX idx_invocations_contract ON soroban_invocations (contract_id, created_at DESC);
CREATE INDEX idx_invocations_function ON soroban_invocations (contract_id, function_name);
CREATE INDEX idx_invocations_tx ON soroban_invocations (transaction_id);

CREATE TABLE soroban_events (
    id               BIGSERIAL,
    transaction_id   BIGINT NOT NULL,
    contract_id      VARCHAR(56),
    event_type       VARCHAR(20) NOT NULL,
    topics           JSONB NOT NULL,
    data             JSONB NOT NULL,
    ledger_sequence  BIGINT NOT NULL,
    created_at       TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (id, created_at),
    FOREIGN KEY (transaction_id) REFERENCES transactions(id) ON DELETE CASCADE,
    FOREIGN KEY (contract_id) REFERENCES soroban_contracts(contract_id)
) PARTITION BY RANGE (created_at);

CREATE INDEX idx_events_contract ON soroban_events (contract_id, created_at DESC);
CREATE INDEX idx_events_topics ON soroban_events USING GIN (topics);
CREATE INDEX idx_events_tx ON soroban_events (transaction_id);

CREATE TABLE event_interpretations (
    id                   BIGSERIAL PRIMARY KEY,
    event_id             BIGINT NOT NULL,
    event_created_at     TIMESTAMPTZ NOT NULL,
    interpretation_type  VARCHAR(50) NOT NULL,
    human_readable       TEXT NOT NULL,
    structured_data      JSONB NOT NULL,
    FOREIGN KEY (event_id, event_created_at)
        REFERENCES soroban_events(id, created_at) ON DELETE CASCADE
);

CREATE INDEX idx_interpretations_type ON event_interpretations (interpretation_type);
CREATE INDEX idx_interpretations_event ON event_interpretations (event_id, event_created_at);

-- Initial monthly partitions for soroban_invocations (Apr-Jun 2026)
CREATE TABLE soroban_invocations_y2026m04 PARTITION OF soroban_invocations
    FOR VALUES FROM ('2026-04-01 00:00:00+00') TO ('2026-05-01 00:00:00+00');
CREATE TABLE soroban_invocations_y2026m05 PARTITION OF soroban_invocations
    FOR VALUES FROM ('2026-05-01 00:00:00+00') TO ('2026-06-01 00:00:00+00');
CREATE TABLE soroban_invocations_y2026m06 PARTITION OF soroban_invocations
    FOR VALUES FROM ('2026-06-01 00:00:00+00') TO ('2026-07-01 00:00:00+00');

CREATE TABLE soroban_invocations_default PARTITION OF soroban_invocations
    DEFAULT;

-- Initial monthly partitions for soroban_events (Apr-Jun 2026)
CREATE TABLE soroban_events_y2026m04 PARTITION OF soroban_events
    FOR VALUES FROM ('2026-04-01 00:00:00+00') TO ('2026-05-01 00:00:00+00');
CREATE TABLE soroban_events_y2026m05 PARTITION OF soroban_events
    FOR VALUES FROM ('2026-05-01 00:00:00+00') TO ('2026-06-01 00:00:00+00');
CREATE TABLE soroban_events_y2026m06 PARTITION OF soroban_events
    FOR VALUES FROM ('2026-06-01 00:00:00+00') TO ('2026-07-01 00:00:00+00');

CREATE TABLE soroban_events_default PARTITION OF soroban_events
    DEFAULT;
