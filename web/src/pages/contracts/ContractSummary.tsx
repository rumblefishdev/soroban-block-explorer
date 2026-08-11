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
            (2 of ~3.9k on prod) falls back to a dash. Asset and issuer are
            SEPARATE labelled cells (0472) — an asset code alone is ambiguous
            (prod carries many issuers of e.g. "USDC"), but two bare links
            side by side in one cell read as two anonymous buttons. Native
            XLM has no issuer, so it renders the asset cell only. */}
        {contract.is_sac && (
          <SummaryRow
            cells={[
              {
                label: 'Asset',
                value: contract.sac_asset ? (
                  <IdentifierDisplay
                    value={sacAssetCode(contract.sac_asset)}
                    type="asset"
                    truncate={false}
                    href={routes.asset(sacAssetId(contract.sac_asset))}
                  />
                ) : (
                  <Dash />
                ),
              },
              ...(contract.sac_asset?.issuer
                ? [
                    {
                      label: 'Issuer',
                      value: (
                        <IdentifierDisplay
                          value={contract.sac_asset.issuer}
                          type="account"
                          href={routes.account(contract.sac_asset.issuer)}
                        />
                      ),
                    },
                  ]
                : []),
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
