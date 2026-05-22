import Box from '@mui/material/Box';
import Typography from '@mui/material/Typography';
import type { ReactNode } from 'react';

import { monoFontFamily } from '../theme/typography.js';
import { SearchInput } from './SearchInput.js';

export interface NetworkStats {
  tps: number;
  ledger: number;
  accounts: number;
  contracts: number;
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
    <Box display="flex" alignItems="center" gap={1} flexShrink={0}>
      <Typography variant="bodySmMedium" color="text.tertiary" noWrap>
        {label}
      </Typography>
      <Typography
        noWrap
        sx={{
          fontFamily: monoFontFamily,
          fontSize: 14,
          fontWeight: 500,
          lineHeight: 1.4,
          color: valueColor,
        }}
      >
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
  return n.toLocaleString('en-US');
}

/** Renders the TPS counter — dash when stats unavailable OR genuinely
 *  zero. Historical backfill data has no recent 60s TPS, so `"0.0"`
 *  reads as a dead network rather than "no data". */
function formatTps(stats: NetworkStats | undefined): string {
  if (!stats || stats.tps === 0) return '—';
  return stats.tps.toFixed(1);
}

/** Renders a counter — dash when stats unavailable. */
function formatStat(
  stats: NetworkStats | undefined,
  pick: (s: NetworkStats) => number
): string {
  if (!stats) return '—';
  return formatNumber(pick(stats));
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
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        px: 10,
        py: 1,
        borderBottom: `1px solid ${theme.palette.stroke.default}`,
        backgroundColor: theme.palette.surface.backgroundAlt,
        width: '100%',
      })}
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
          display="flex"
          alignItems="center"
          gap={1.5}
          minWidth={0}
          overflow="hidden"
        >
          <Stat
            label="TPS"
            value={formatTps(stats)}
            valueColor="text.success"
          />
          <StatDivider />
          <Stat label="Ledger" value={formatStat(stats, (s) => s.ledger)} />
          <StatDivider />
          <Stat label="Accounts" value={formatStat(stats, (s) => s.accounts)} />
          <StatDivider />
          <Stat
            label="Contracts"
            value={formatStat(stats, (s) => s.contracts)}
          />
        </Box>
      </Box>

      <Box sx={{ position: 'relative', flexShrink: 0 }}>
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
              width: 628,
              zIndex: theme.zIndex.modal,
            })}
          >
            {searchOverlaySlot}
          </Box>
        )}
      </Box>
    </Box>
  );
}
