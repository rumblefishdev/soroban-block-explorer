import { Box, Stack, Typography } from '@mui/material';
import type { AssetDetailResponse } from '@rumblefish/api-types';
import {
  IdentifierDisplay,
  IdentifierWithCopy,
} from '@rumblefish/soroban-block-explorer-ui';

import { SectionCard } from '../detail/SectionCard.js';
import { SummaryRow } from '../detail/SummaryRow.js';
import { formatAmount } from '../format.js';

function SupplyValue({
  supply,
  code,
}: {
  supply?: string | null;
  code?: string | null;
}) {
  return (
    <Stack>
      <Typography variant="bodySmRegular" sx={{ color: 'text.primary' }}>
        {formatAmount(supply)}
      </Typography>
      {code && (
        <Typography variant="bodyXsRegular" sx={{ color: 'text.tertiary' }}>
          {code}
        </Typography>
      )}
    </Stack>
  );
}

/**
 * Asset summary card — issuer (classic) or contract ID (Soroban/SAC), total
 * supply, holder count, and the deploy ledger for contract-based assets.
 */
export function AssetSummary({ asset }: { asset: AssetDetailResponse }) {
  return (
    <SectionCard title="Summary">
      {asset.issuer && (
        <SummaryRow
          cells={[
            {
              label: 'Issuer',
              value: (
                <Box
                  sx={{
                    '& a': { whiteSpace: 'normal', wordBreak: 'break-all' },
                  }}
                >
                  <IdentifierWithCopy
                    value={asset.issuer}
                    type="account"
                    truncate={false}
                  />
                </Box>
              ),
            },
          ]}
        />
      )}
      {asset.contract_id && (
        <SummaryRow
          cells={[
            {
              label: 'Contract ID',
              value: (
                <Box
                  sx={{
                    '& a': { whiteSpace: 'normal', wordBreak: 'break-all' },
                  }}
                >
                  <IdentifierWithCopy
                    value={asset.contract_id}
                    type="contract"
                    truncate={false}
                  />
                </Box>
              ),
            },
          ]}
        />
      )}
      <SummaryRow
        cells={[
          {
            label: 'Total supply',
            value: (
              <SupplyValue
                supply={asset.total_supply}
                code={asset.asset_code}
              />
            ),
          },
          { label: 'Holders', value: formatAmount(asset.holder_count) },
        ]}
      />
      {asset.deployed_at_ledger != null && (
        <SummaryRow
          cells={[
            {
              label: 'Deployed at ledger',
              value: (
                <IdentifierDisplay
                  value={String(asset.deployed_at_ledger)}
                  type="ledger"
                />
              ),
            },
          ]}
        />
      )}
    </SectionCard>
  );
}
