import { Avatar } from '@mui/material';
import type { Theme } from '@mui/material/styles';

import { safeHttpUrl } from '../url.js';

export type AssetIconKind = 'native' | 'classic' | 'sac' | 'default';

interface AssetIconProps {
  /** Asset code — its first letter is the fallback when no icon is available. */
  code?: string | null;
  /** Asset icon URL from metadata; falls back to a letter avatar when absent. */
  iconUrl?: string | null;
  size?: number;

  kind?: AssetIconKind;
}

function kindColors(theme: Theme, kind: AssetIconKind) {
  switch (kind) {
    case 'native':
      return { bg: theme.palette.blue[100], fg: theme.palette.blue[600] };
    case 'classic':
      return {
        bg: theme.palette.emerald[100],
        fg: theme.palette.emerald[600],
      };
    case 'sac':
      return { bg: '#724311', fg: '#fffcc2' };
    case 'default':
    default:
      return {
        bg: theme.palette.surface.grayMain,
        fg: theme.palette.text.secondary,
      };
  }
}

/**
 * Round token avatar used everywhere an asset (or pool leg — a pool is two
 * assets) is shown: assets table, account balances, asset detail header,
 * and liquidity-pool legs. Renders the metadata `iconUrl` when present
 * (sanitised via `safeHttpUrl`), otherwise falls back to a letter avatar
 * derived from the code — the fallback is MUI `Avatar`'s native behaviour,
 * so a broken/absent image needs no manual handling.
 */
export function AssetIcon({
  code,
  iconUrl,
  size = 32,
  kind = 'default',
}: AssetIconProps) {
  const letter = (code ?? '?').trim().charAt(0).toUpperCase() || '?';
  return (
    <Avatar
      src={safeHttpUrl(iconUrl) ?? undefined}
      alt=""
      sx={(theme) => {
        const { bg, fg } = kindColors(theme, kind);
        return {
          width: size,
          height: size,
          fontSize: size * 0.42,
          fontWeight: 600,
          bgcolor: bg,
          color: fg,
          flexShrink: 0,
        };
      }}
    >
      {letter}
    </Avatar>
  );
}
