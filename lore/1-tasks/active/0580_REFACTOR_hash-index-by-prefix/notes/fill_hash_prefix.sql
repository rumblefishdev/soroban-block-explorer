INSERT INTO transaction_hash_index_staging_prefix (hash_prefix, ledger_sequence)
SELECT reinterpretAsUInt64(substring(hash, 1, 8)), ledger_sequence
FROM transaction_hash_index
WHERE ledger_sequence >= {A} AND ledger_sequence < {B}
