import Box from '@mui/material/Box';
import Typography from '@mui/material/Typography';
import type { ReactNode } from 'react';

import { formatInteger, formatTps } from '../format/index.js';
import { grid } from '../theme/grid.js';
import { SearchInput } from './SearchInput.js';

/**
 * Slim subset of `GET /v1/network/stats` that TopNav needs to render
 * its counters. Field names mirror the API exactly so callers can pass
 * the raw query response through without a mapping layer (structural
 * typing accepts the wider shape).
 */
export interface NetworkStats {
  tps_60s: number;
  latest_ledger_sequence: number;
  total_accounts: number;
  total_contracts: number;
}

export interface TopNavProps {
  /** Live network counters. Pass `undefined` to render dashes while the
   *  underlying query is loading or errored — TopNav handles the
   *  fallback so callers don't ship visually-misleading hard-coded
   *  zeros. */
  stats?: NetworkStats;
  searchValue: string;
  onSearchChange: (value: string) => void;
  onSearchSubmit?: () => void;
  onSearchClear?: () => void;
  searchOverlaySlot?: ReactNode;
}

function StatDivider() {
  return (
    <Box
      sx={(theme) => ({
        width: '1px',
        height: '20px',
        backgroundColor: theme.palette.stroke.default,
        flexShrink: 0,
      })}
    />
  );
}

function Stat({
  label,
  value,
  valueColor = 'text.primary',
}: {
  label: string;
  value: string;
  valueColor?: string;
}) {
  return (
    <Box display="flex" alignItems="baseline" gap={1} flexShrink={0}>
      <Typography variant="bodySmMedium" color="text.tertiary" noWrap>
        {label}
      </Typography>

      <Typography variant="bodyMonoSmMedium" noWrap sx={{ color: valueColor }}>
        {value}
      </Typography>
    </Box>
  );
}

function formatNumber(n: number): string {
  if (n >= 1_000_000) {
    const value = n / 1_000_000;
    return Number.isInteger(value) ? `${value}M` : `${value.toFixed(1)}M`;
  }
  return formatInteger(n);
}

export function TopNav({
  stats,
  searchValue,
  onSearchChange,
  onSearchSubmit,
  onSearchClear,
  searchOverlaySlot,
}: TopNavProps) {
  return (
    <Box
      component="header"
      sx={(theme) => ({
        width: '100%',
        borderBottom: `1px solid ${theme.palette.stroke.default}`,
        backgroundColor: theme.palette.surface.backgroundAlt,

        position: 'relative',
        zIndex: 2,
      })}
    >
      <Box
        sx={{
          display: 'flex',
          flexWrap: { xs: 'wrap', lg: 'nowrap' },
          alignItems: 'center',
          justifyContent: 'space-between',
          width: '100%',
          maxWidth: grid.desktop.maxWidth,
          mx: 'auto',
          px: {
            xs: `${grid.mobile.margin}px`,
            md: `${grid.desktop.margin}px`,
          },
          py: 1,
          gap: 1,
        }}
      >
        <Box
          display="flex"
          alignItems="center"
          gap={2}
          flex={1}
          minWidth={0}
          overflow="hidden"
        >
          <Box
            sx={{
              display: 'flex',
              alignItems: 'center',
              gap: 1.5,
              minWidth: 0,
              overflowX: 'auto',
              scrollbarWidth: 'none',
              '&::-webkit-scrollbar': { display: 'none' },
            }}
          >
            <Stat
              label="TPS"
              value={stats ? formatTps(stats.tps_60s) : '—'}
              valueColor="text.success"
            />
            <StatDivider />
            <Stat
              label="Ledger"
              value={stats ? formatNumber(stats.latest_ledger_sequence) : '—'}
            />
            <StatDivider />
            <Stat
              label="Accounts"
              value={stats ? formatNumber(stats.total_accounts) : '—'}
            />
            <StatDivider />
            <Stat
              label="Contracts"
              value={stats ? formatNumber(stats.total_contracts) : '—'}
            />
          </Box>
        </Box>

        <Box
          sx={{
            position: 'relative',
            flexShrink: 0,
            minWidth: 0,
            width: { xs: '100%', lg: 'auto' },
          }}
        >
          <SearchInput
            value={searchValue}
            onChange={onSearchChange}
            onSubmit={onSearchSubmit}
            onClear={onSearchClear}
          />
          {searchOverlaySlot && (
            <Box
              sx={(theme) => ({
                position: 'absolute',
                top: '100%',
                right: 0,
                mt: 0.5,
                width: { xs: 'calc(100vw - 32px)', md: 628 },
                maxWidth: 628,
                zIndex: theme.zIndex.modal,
              })}
            >
              {searchOverlaySlot}
            </Box>
          )}
        </Box>
      </Box>
    </Box>
  );
}
