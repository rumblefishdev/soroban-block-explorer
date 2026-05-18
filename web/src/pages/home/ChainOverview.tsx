import { Box, Divider } from '@mui/material';
import { GenericErrorState } from '@rumblefish/soroban-block-explorer-ui';

import { useNetworkStats } from '../../api/index.js';
import { formatAmount } from '../format.js';

import { ChainOverviewCard } from './ChainOverviewCard.js';
import { LiveIndicator } from './LiveIndicator.js';

/**
 * Chain overview — four compact stat cards (current ledger, TPS, accounts,
 * contracts) backed by `GET /network/stats`. Polls on the home cadence;
 * a failed query shows an inline retry without collapsing the page.
 */
export function ChainOverview() {
  const { data, isLoading, isError, refetch } = useNetworkStats();

  return (
    <Box sx={{ px: 10 }}>
      <Box
        sx={{
          maxWidth: 1064,
          mx: 'auto',
          borderRadius: 4,
          border: (theme) => `1px solid ${theme.palette.stroke.default}`,
          backgroundColor: 'surface.grayMainAlt',
          overflow: 'hidden',
        }}
      >
        {isError ? (
          <Box sx={{ display: 'flex', justifyContent: 'center', py: 4 }}>
            <GenericErrorState onRetry={() => void refetch()} />
          </Box>
        ) : (
          <Box sx={{ display: 'flex', alignItems: 'stretch' }}>
            <ChainOverviewCard
              label={<LiveIndicator />}
              value={
                data ? formatAmount(data.latest_ledger_sequence) : undefined
              }
              caption="Current ledger"
              loading={isLoading}
            />
            <Divider orientation="vertical" flexItem />
            <ChainOverviewCard
              label="TPS"
              value={data ? data.tps_60s.toFixed(1) : undefined}
              caption="Last 60s"
              accentValue
              loading={isLoading}
            />
            <Divider orientation="vertical" flexItem />
            <ChainOverviewCard
              label="Accounts"
              value={data ? formatAmount(data.total_accounts) : undefined}
              caption="Total"
              loading={isLoading}
            />
            <Divider orientation="vertical" flexItem />
            <ChainOverviewCard
              label="Contracts"
              value={data ? formatAmount(data.total_contracts) : undefined}
              caption="Soroban"
              loading={isLoading}
            />
          </Box>
        )}
      </Box>
    </Box>
  );
}
