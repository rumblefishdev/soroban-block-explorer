import { Box, Divider, Stack } from '@mui/material';
import { alpha } from '@mui/material/styles';
import {
  classifyError,
  formatAmount,
  formatTps,
  GenericErrorState,
  RateLimitState,
  TransientErrorState,
} from '@rumblefish/soroban-block-explorer-ui';
import type { ReactNode } from 'react';

import { useNetworkStats } from '../../api/index.js';
import { KpiCell } from '../detail/KpiCell.js';

import { LiveIndicator } from './LiveIndicator.js';

/**
 * Chain overview — four compact stat cards (current ledger, TPS, accounts,
 * contracts) backed by `GET /network/stats`. Polls on the home cadence;
 * a failed query shows an inline, classified retry without collapsing the
 * page.
 */
export function ChainOverview() {
  const { data, isLoading, isError, error, refetch } = useNetworkStats();

  let content: ReactNode;
  if (isError) {
    const kind = classifyError(error);
    const retry = () => void refetch();
    content =
      kind === 'rate-limit' ? (
        <RateLimitState onRetry={retry} py={4} />
      ) : kind === 'transient' ? (
        <TransientErrorState onRetry={retry} py={4} />
      ) : (
        <GenericErrorState onRetry={retry} py={4} />
      );
  } else {
    content = (
      <Stack
        direction={{ xs: 'column', md: 'row' }}
        alignItems="stretch"
        divider={
          <Divider
            orientation="vertical"
            flexItem
            sx={{ display: { xs: 'none', md: 'block' } }}
          />
        }
        sx={{ width: '100%' }}
      >
        <KpiCell
          card={false}
          align="center"
          valueVariant="heading4SemiBold"
          labelVariant="bodyMedium"
          label={<LiveIndicator />}
          value={data ? formatAmount(data.latest_ledger_sequence) : undefined}
          caption="Current ledger"
          loading={isLoading}
        />
        <KpiCell
          card={false}
          align="center"
          valueVariant="heading4SemiBold"
          labelVariant="bodyMedium"
          label="TPS"
          value={data ? formatTps(data.tps_60s) : undefined}
          caption="Last 60s"
          valueColor={(theme) => theme.palette.text.success}
          loading={isLoading}
        />
        <KpiCell
          card={false}
          align="center"
          valueVariant="heading4SemiBold"
          labelVariant="bodyMedium"
          label="Accounts"
          value={data ? formatAmount(data.total_accounts) : undefined}
          caption="Total"
          loading={isLoading}
        />
        <KpiCell
          card={false}
          align="center"
          valueVariant="heading4SemiBold"
          labelVariant="bodyMedium"
          label="Contracts"
          value={data ? formatAmount(data.total_contracts) : undefined}
          caption="Soroban"
          loading={isLoading}
        />
      </Stack>
    );
  }

  return (
    <Box sx={{ display: 'flex', justifyContent: 'center' }}>
      <Box
        sx={(theme) => ({
          width: '100%',
          maxWidth: 1064,
          borderRadius: '16px',
          border: `1px solid ${theme.palette.stroke.default}`,
          backgroundColor: alpha(theme.palette.surface.grayMainAlt, 0.8),
          backdropFilter: 'blur(6px)',
          overflow: 'hidden',
        })}
      >
        {content}
      </Box>
    </Box>
  );
}
