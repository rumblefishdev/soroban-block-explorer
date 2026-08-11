import { Stack } from '@mui/material';
import type { ContractDetailResponse } from '@rumblefish/api-types';
import {
  Dash,
  formatAmount,
  IdentifierDisplay,
  IdentifierWithCopy,
} from '@rumblefish/soroban-block-explorer-ui';

import { routes } from '../../router/routes.js';
import { KpiCell } from '../detail/KpiCell.js';
import { SectionCard } from '../detail/SectionCard.js';
import { SummaryRow } from '../detail/SummaryRow.js';

import { sacAssetCode, sacAssetId } from './sacAsset.js';

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
          label={`Invocations (last ${stats.stats_window})`}
          value={formatAmount(stats.recent_invocations)}
        />
        <KpiCell
          label={`Unique callers (last ${stats.stats_window})`}
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
                  linked={false}
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
        {/* Task 0441: which classic asset this SAC mirrors, linked to its
            asset page. Row only renders for a SAC; an unresolvable facet
            (2 of ~3.9k on prod) falls back to a dash. The issuer rides
            along because an asset code alone is ambiguous — prod carries
            many issuers of e.g. "USDC". */}
        {contract.is_sac && (
          <SummaryRow
            cells={[
              {
                label: 'Mirrors asset',
                value: contract.sac_asset ? (
                  <Stack direction="row" spacing={1} alignItems="center">
                    <IdentifierDisplay
                      value={sacAssetCode(contract.sac_asset)}
                      type="asset"
                      truncate={false}
                      href={routes.asset(sacAssetId(contract.sac_asset))}
                    />
                    {contract.sac_asset.issuer && (
                      <IdentifierDisplay
                        value={contract.sac_asset.issuer}
                        type="account"
                        href={routes.account(contract.sac_asset.issuer)}
                      />
                    )}
                  </Stack>
                ) : (
                  <Dash />
                ),
              },
            ]}
          />
        )}
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
