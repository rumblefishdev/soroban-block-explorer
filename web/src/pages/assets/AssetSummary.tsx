import { Box, Stack, Typography } from '@mui/material';
import type { AssetDetailResponse } from '@rumblefish/api-types';
import {
  formatAmount,
  IdentifierDisplay,
  IdentifierWithCopy,
  scaleByDecimals,
} from '@rumblefish/soroban-block-explorer-ui';

import { SectionCard } from '../detail/SectionCard.js';
import { SummaryRow } from '../detail/SummaryRow.js';

function SupplyValue({
  supply,
  decimals,
  code,
}: {
  supply?: string | null;
  decimals: number;
  code?: string | null;
}) {
  return (
    <Stack>
      <Typography
        variant="bodySmBold"
        sx={(theme) => ({
          color: theme.palette.text.primary,
          // A long supply is one unbroken token — let it wrap instead of
          // overflowing into the adjacent "Holders" cell (F4).
          overflowWrap: 'anywhere',
        })}
      >
        {formatAmount(scaleByDecimals(supply, decimals))}
      </Typography>
      {code && (
        <Typography
          variant="bodyXsRegular"
          sx={(theme) => ({ color: theme.palette.text.secondary })}
        >
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
              // libs/ui IdentifierDisplay has no wrap mode; in this narrow
              // (half-width) card a 56-char id would clip. Override the
              // anchor to wrap instead of ellipsis-clipping.
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
              // See the Issuer note above — same wrap override.
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
      {/* ADR 0051: the classic/native asset's Stellar Asset Contract.
       *
       * Gated on `sac_deployed`, not merely on the address existing (task
       * 0450). Every classic asset HAS a SAC address — it is derived from
       * (code, issuer, network) and needs nobody's permission to exist. We
       * only learn one when the asset emits a CAP-67 unified asset event
       * (`transfer`/`mint`/`burn`/…), which classic transfers now do whether
       * or not a SAC was ever deployed. So an address with `sac_deployed =
       * false` says "this asset has moved", not "this asset has a contract" —
       * showing it under a "SAC contract" label answered a question nobody
       * asked, and made the row appear or vanish based on whether the asset
       * happened to have activity. Two otherwise-identical classic assets
       * disagreed with no visible reason.
       *
       * With this gate all four surfaces mean one thing by "SAC": the list's
       * `SAC` chip, the `Has SAC` filter (`HAVING max(sac_deployed)`), the
       * assets-list issuer column, and this row. */}
      {asset.sac_contract_id && asset.sac_deployed && (
        <SummaryRow
          cells={[
            {
              label: 'SAC contract',
              value: (
                <Box
                  sx={{
                    '& a': { whiteSpace: 'normal', wordBreak: 'break-all' },
                  }}
                >
                  <IdentifierWithCopy
                    value={asset.sac_contract_id}
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
                decimals={asset.decimals}
                // Soroban-native tokens have no classic `asset_code`; fall back
                // to the on-chain SEP-41 `symbol` so supply reads e.g. "1.5 USDC"
                // instead of an unlabelled number (task 0304).
                code={asset.asset_code ?? asset.symbol}
              />
            ),
          },
          {
            label: 'Holders',
            value: formatAmount(asset.holder_count),
            labelMinWidth: 70,
          },
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
