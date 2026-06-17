import { Box, Divider, Stack } from '@mui/material';
import { alpha } from '@mui/material/styles';
import {
  AnimatedNumber,
  QueryErrorState,
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

  // Error state only when there is nothing to show — a transient failed
  // poll keeps the last good stats on screen instead of collapsing the
  // panel (the LiveIndicator flips to OFFLINE to signal the condition).
  let content: ReactNode;
  if (isError && !data) {
    content = (
      <QueryErrorState error={error} onRetry={() => void refetch()} py={4} />
    );
  } else {
    const cells = [
      <KpiCell
        key="ledger"
        card={false}
        align="center"
        valueVariant="heading4SemiBold"
        labelVariant="bodyMedium"
        label={<LiveIndicator />}
        value={
          data ? (
            <AnimatedNumber value={data.latest_ledger_sequence} />
          ) : undefined
        }
        caption="Current ledger"
        loading={isLoading}
      />,
      <KpiCell
        key="tps"
        card={false}
        align="center"
        valueVariant="heading4SemiBold"
        labelVariant="bodyMedium"
        label="TPS"
        value={
          data ? (
            <AnimatedNumber
              value={data.tps_60s}
              format={{ minimumFractionDigits: 1, maximumFractionDigits: 1 }}
            />
          ) : undefined
        }
        caption="Last 60s"
        valueColor={(theme) => theme.palette.text.success}
        loading={isLoading}
      />,
      <KpiCell
        key="accounts"
        card={false}
        align="center"
        valueVariant="heading4SemiBold"
        labelVariant="bodyMedium"
        label="Accounts"
        value={
          data ? <AnimatedNumber value={data.total_accounts} /> : undefined
        }
        caption="Total"
        loading={isLoading}
      />,
      <KpiCell
        key="contracts"
        card={false}
        align="center"
        valueVariant="heading4SemiBold"
        labelVariant="bodyMedium"
        label="Contracts"
        value={
          data ? <AnimatedNumber value={data.total_contracts} /> : undefined
        }
        caption="Soroban"
        loading={isLoading}
      />,
    ];
    content = (
      <>
        {/* Mobile/tablet: 2×2 grid — a 4-tall vertical stack reads as dead
            space. 1px gap over a stroke-coloured bg paints the divider lines. */}
        <Box
          sx={(theme) => ({
            display: { xs: 'grid', md: 'none' },
            gridTemplateColumns: '1fr 1fr',
            gap: '1px',
            backgroundColor: theme.palette.stroke.default,
            '& > *': { backgroundColor: theme.palette.surface.grayMainAlt },
          })}
        >
          {cells}
        </Box>
        {/* Desktop: single row with vertical dividers. */}
        <Stack
          direction="row"
          alignItems="stretch"
          divider={<Divider orientation="vertical" flexItem />}
          sx={{ width: '100%', display: { xs: 'none', md: 'flex' } }}
        >
          {cells}
        </Stack>
      </>
    );
  }

  return (
    <Box sx={{ display: 'flex', justifyContent: 'center' }}>
      <Box
        sx={(theme) => ({
          width: '100%',
          maxWidth: 1064,
          borderRadius: `${theme.shape.radius.lg}px`,
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
