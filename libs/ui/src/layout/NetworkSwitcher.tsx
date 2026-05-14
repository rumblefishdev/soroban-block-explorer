import Box from '@mui/material/Box';
import Typography from '@mui/material/Typography';

import { scales } from '../theme/colors.js';

export type Network = 'mainnet' | 'testnet';

export interface NetworkSwitcherProps {
  network: Network;
  onNetworkChange?: (network: Network) => void;
}

interface NetworkTabProps {
  label: string;
  active: boolean;
  variant: 'mainnet' | 'testnet';
  onClick?: () => void;
}

function NetworkTab({ label, active, variant, onClick }: NetworkTabProps) {
  const isMainnet = variant === 'mainnet';

  const activeSx = isMainnet
    ? {
        backgroundColor: scales.blue[100],
        border: `1px solid ${scales.blue[600]}`,
        color: scales.blue[600],
      }
    : {
        backgroundColor: (theme: import('@mui/material').Theme) =>
          theme.palette.surface.warning,
        border: (theme: import('@mui/material').Theme) =>
          `1px solid ${theme.palette.stroke.warning}`,
        color: (theme: import('@mui/material').Theme) =>
          theme.palette.text.warning,
      };

  const inactiveSx = {
    backgroundColor: 'transparent',
    border: '1px solid transparent',
    color: (theme: import('@mui/material').Theme) =>
      theme.palette.text.secondary,
  };

  return (
    <Box
      component={onClick ? 'button' : 'span'}
      {...(onClick ? { type: 'button' as const } : {})}
      role="tab"
      aria-selected={active}
      onClick={onClick}
      sx={(theme) => ({
        display: 'inline-flex',
        alignItems: 'center',
        gap: 1,
        px: 1.5,
        py: 0.25,
        borderRadius: '8px',
        cursor: onClick ? 'pointer' : 'default',
        background: 'none',
        fontFamily: 'inherit',
        ...(active ? activeSx : inactiveSx),
        transition: 'background-color 0.15s, border-color 0.15s',
        ...(onClick && !active
          ? {
              '&:hover': {
                backgroundColor: theme.palette.surface.background,
              },
            }
          : {}),
        ...(onClick
          ? {
              '&:focus-visible': {
                outline: `2px solid ${theme.palette.stroke.action}`,
                outlineOffset: 1,
              },
            }
          : {}),
      })}
    >
      <Typography variant="bodySmMedium" color="inherit" noWrap>
        {label}
      </Typography>
    </Box>
  );
}

export function NetworkSwitcher({
  network,
  onNetworkChange,
}: NetworkSwitcherProps) {
  return (
    <Box
      role="tablist"
      aria-label="Network"
      sx={(theme) => ({
        display: 'inline-flex',
        alignItems: 'center',
        p: 0.5,
        borderRadius: '8px',
        border: `1px solid ${theme.palette.stroke.default}`,
        backgroundColor: theme.palette.surface.grayMain,
        gap: 0,
      })}
    >
      <NetworkTab
        label="Mainnet"
        active={network === 'mainnet'}
        variant="mainnet"
        onClick={onNetworkChange ? () => onNetworkChange('mainnet') : undefined}
      />
      <NetworkTab
        label="Testnet"
        active={network === 'testnet'}
        variant="testnet"
        onClick={onNetworkChange ? () => onNetworkChange('testnet') : undefined}
      />
    </Box>
  );
}
