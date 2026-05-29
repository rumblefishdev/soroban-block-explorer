import { Stack } from '@mui/material';
import type { ContractDetailResponse } from '@rumblefish/api-types';
import {
  IdentifierDisplay,
  IdentifierWithCopy,
} from '@rumblefish/soroban-block-explorer-ui';

import { KpiCell } from '../detail/KpiCell.js';
import { SectionCard } from '../detail/SectionCard.js';
import { SummaryRow } from '../detail/SummaryRow.js';
import { formatAmount } from '../format.js';
import { Dash } from '../transactions/cells.js';

/**
 * Contract summary block — windowed stat tiles plus the metadata card
 * (contract ID, deployer, deployed-at ledger, WASM hash). Stats are scoped
 * to `stats_window` (the API exposes recent counts, not full-history
 * totals), so the tiles are labelled with the window.
 */
export function ContractSummary({
  contract,
}: {
  contract: ContractDetailResponse;
}) {
  const { stats } = contract;
  return (
    <Stack spacing={3}>
      <Stack direction={{ xs: 'column', sm: 'row' }} spacing={3}>
        <KpiCell
          label="Total invocations"
          value={formatAmount(stats.recent_invocations)}
        />
        <KpiCell
          label="Unique callers"
          value={formatAmount(stats.recent_unique_callers)}
        />
      </Stack>

      <SectionCard title="Summary">
        <SummaryRow
          cells={[
            {
              label: 'Contract ID',
              value: (
                <IdentifierWithCopy
                  value={contract.contract_id}
                  type="contract"
                  truncate={false}
                />
              ),
            },
          ]}
        />
        <SummaryRow
          cells={[
            {
              label: 'Deployer',
              value: contract.deployer ? (
                <IdentifierDisplay value={contract.deployer} type="account" />
              ) : (
                <Dash />
              ),
            },
            {
              label: 'Deployed at ledger',
              value:
                contract.deployed_at_ledger != null ? (
                  <IdentifierDisplay
                    value={String(contract.deployed_at_ledger)}
                    type="ledger"
                  />
                ) : (
                  <Dash />
                ),
            },
          ]}
        />
        <SummaryRow
          cells={[
            {
              label: 'WASM hash',
              value: contract.wasm_hash ? (
                <IdentifierWithCopy
                  value={contract.wasm_hash}
                  type="transaction"
                  linked={false}
                  truncate={false}
                />
              ) : (
                <Dash />
              ),
            },
          ]}
        />
      </SectionCard>
    </Stack>
  );
}
