import Box from '@mui/material/Box';
import ButtonBase from '@mui/material/ButtonBase';
import type { Theme } from '@mui/material/styles';

export enum Network {
  MAINNET = 'mainnet',
  TESTNET = 'testnet',
}

export interface NetworkToggleProps {
  network: Network;
  onNetworkChange?: (network: Network) => void;
}

const ACTIVE_PALETTE: Record<
  Network,
  {
    bg: (t: Theme) => string;
    border: (t: Theme) => string;
    fg: (t: Theme) => string;
  }
> = {
  [Network.MAINNET]: {
    bg: (t) => t.palette.blue[100],
    border: (t) => t.palette.blue[600],
    fg: (t) => t.palette.blue[600],
  },
  [Network.TESTNET]: {
    bg: (t) => t.palette.surface.warning,
    border: (t) => t.palette.stroke.warning,
    fg: (t) => t.palette.text.warning,
  },
};

export function NetworkToggle({
  network,
  onNetworkChange,
}: NetworkToggleProps) {
  return (
    <Box
      role="group"
      aria-label="Network selector"
      sx={(theme) => ({
        display: 'inline-flex',
        alignItems: 'center',
        p: 0.5,
        borderRadius: '8px',
        border: `1px solid ${theme.palette.stroke.default}`,
        backgroundColor: theme.palette.surface.grayMain,
      })}
    >
      <NetworkOption
        label="Mainnet"
        variant={Network.MAINNET}
        active={network === Network.MAINNET}
        onSelect={onNetworkChange}
      />
      <NetworkOption
        label="Testnet"
        variant={Network.TESTNET}
        active={network === Network.TESTNET}
        onSelect={onNetworkChange}
      />
    </Box>
  );
}

interface NetworkOptionProps {
  label: string;
  variant: Network;
  active: boolean;
  onSelect?: (network: Network) => void;
}

function NetworkOption({
  label,
  variant,
  active,
  onSelect,
}: NetworkOptionProps) {
  const interactive = Boolean(onSelect);
  return (
    <ButtonBase
      component={interactive ? 'button' : 'span'}
      disabled={!interactive}
      onClick={interactive ? () => onSelect?.(variant) : undefined}
      aria-pressed={interactive ? active : undefined}
      aria-label={interactive ? `Switch to ${label}` : undefined}
      sx={(theme) => {
        const palette = ACTIVE_PALETTE[variant];
        return {
          display: 'inline-flex',
          alignItems: 'center',
          px: 1.5,
          py: 0.25,
          borderRadius: '8px',
          ...theme.typography.bodySmMedium,
          cursor: interactive ? 'pointer' : 'default',
          transition: 'background-color 0.15s, border-color 0.15s, color 0.15s',
          ...(active
            ? {
                backgroundColor: palette.bg(theme),
                border: `1px solid ${palette.border(theme)}`,
                color: palette.fg(theme),
              }
            : {
                backgroundColor: 'transparent',
                border: '1px solid transparent',
                color: theme.palette.text.secondary,
                '&:hover': interactive
                  ? { backgroundColor: theme.palette.surface.background }
                  : undefined,
              }),
          '&:focus-visible': {
            outline: `2px solid ${theme.palette.stroke.action}`,
            outlineOffset: 1,
          },
        };
      }}
    >
      {label}
    </ButtonBase>
  );
}
