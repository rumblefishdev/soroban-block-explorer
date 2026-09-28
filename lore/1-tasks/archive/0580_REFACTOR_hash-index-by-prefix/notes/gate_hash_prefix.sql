SELECT uniqExact({K}, ledger_sequence) FROM {TBL}
WHERE ledger_sequence >= {LO} AND ledger_sequence < {HI}
FORMAT TSV
