import Box from '@mui/material/Box';
import Typography from '@mui/material/Typography';
import type { ReactNode } from 'react';

import type { Format } from '@number-flow/react';

import { AnimatedNumber } from '../format/index.js';
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
  onSearchFocus?: () => void;
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
  value: ReactNode;
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

/** One fixed decimal, matching `formatTps` ("12.3"). */
const TPS_FORMAT: Format = {
  minimumFractionDigits: 1,
  maximumFractionDigits: 1,
};

/** ≥1M compacts to a one-decimal compact suffix (the slim header has no
 *  room for nine grouped digits); below that, plain grouped integer.
 *  Expressed as Intl options so NumberFlow can animate the digits. Close to
 *  the old string `formatNumber` but not identical: whole millions keep one
 *  decimal here ("5.0M" vs the old "5M"), and ≥1e9 uses Intl's `B` unit
 *  ("2.0B" vs the old M-only "2000M"). Both are intentional — the chain
 *  counters are well under 1e9 and the forced decimal reads consistently. */
function countFormat(n: number): Format {
  return n >= 1_000_000
    ? {
        notation: 'compact',
        compactDisplay: 'short',
        minimumFractionDigits: 1,
        maximumFractionDigits: 1,
      }
    : { useGrouping: true };
}

export function TopNav({
  stats,
  searchValue,
  onSearchChange,
  onSearchSubmit,
  onSearchClear,
  onSearchFocus,
  searchOverlaySlot,
}: TopNavProps) {
  return (
    <Box
      component="header"
      sx={(theme) => ({
        width: '100%',
        borderBottom: `1px solid ${theme.palette.stroke.default}`,
        backgroundColor: theme.palette.surface.backgroundAlt,

        // Above SecondaryNav (zIndex 2) so the search-results dropdown —
        // which lives in this header's stacking context — paints over the
        // nav row below instead of being covered by it.
        position: 'relative',
        zIndex: theme.zIndex.topNav,
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
              value={
                <AnimatedNumber
                  value={stats?.tps_60s}
                  format={TPS_FORMAT}
                  animate={false}
                />
              }
              valueColor="text.success"
            />
            <StatDivider />
            <Stat
              label="Ledger"
              value={
                <AnimatedNumber
                  value={stats?.latest_ledger_sequence}
                  format={countFormat}
                  animate={false}
                />
              }
            />
            <StatDivider />
            <Stat
              label="Accounts"
              value={
                <AnimatedNumber
                  value={stats?.total_accounts}
                  format={countFormat}
                  animate={false}
                />
              }
            />
            <StatDivider />
            <Stat
              label="Contracts"
              value={
                <AnimatedNumber
                  value={stats?.total_contracts}
                  format={countFormat}
                  animate={false}
                />
              }
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
            onFocus={onSearchFocus}
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
