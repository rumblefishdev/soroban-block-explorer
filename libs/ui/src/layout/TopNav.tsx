import Box from '@mui/material/Box';
import Typography from '@mui/material/Typography';
import type { ReactNode } from 'react';

import { grid } from '../theme/grid.js';
import { monoFontFamily } from '../theme/typography.js';
import { NetworkSwitcher, type Network } from './NetworkSwitcher.js';
import { SearchInput } from './SearchInput.js';

export interface NetworkStats {
  tps: number;
  ledger: number;
  accounts: number;
  contracts: number;
}

export interface TopNavProps {
  network: Network;
  onNetworkChange?: (network: Network) => void;
  stats: NetworkStats;
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

export function TopNav({
  network,
  onNetworkChange,
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
      })}
    >
      <Box
        sx={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          width: '100%',
          maxWidth: grid.desktop.maxWidth,
          mx: 'auto',
          px: `${grid.desktop.margin}px`,
          py: 1,
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
          <Box flexShrink={0}>
            <NetworkSwitcher
              network={network}
              onNetworkChange={onNetworkChange}
            />
          </Box>

          <Box
            display="flex"
            alignItems="center"
            gap={1.5}
            minWidth={0}
            overflow="hidden"
          >
            <Stat
              label="TPS"
              value={stats.tps.toFixed(1)}
              valueColor="text.success"
            />
            <StatDivider />
            <Stat label="Ledger" value={formatNumber(stats.ledger)} />
            <StatDivider />
            <Stat label="Accounts" value={formatNumber(stats.accounts)} />
            <StatDivider />
            <Stat label="Contracts" value={formatNumber(stats.contracts)} />
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
    </Box>
  );
}
