import { Avatar } from '@mui/material';

import { safeHttpUrl } from '../url.js';

import { assetColor } from './assetColor.js';

interface AssetIconProps {
  /** Asset code — its first letter is the letter fallback, and the value
   *  hashed for the fallback colour (same code → same colour everywhere). */
  code?: string | null;
  /** Asset icon URL from metadata; falls back to a letter avatar when absent. */
  iconUrl?: string | null;
  size?: number;
}

/**
 * Round token avatar used everywhere an asset (or pool leg — a pool is two
 * assets) is shown: assets table, account balances, asset detail header,
 * and liquidity-pool legs. Renders the metadata `iconUrl` when present
 * (sanitised via `safeHttpUrl`), otherwise falls back to a letter avatar
 * coloured per asset identity via `assetColor` — so the same asset reads
 * as the same colour across every view. The image/letter switch is MUI
 * `Avatar`'s native behaviour, so a broken/absent image needs no handling.
 */
export function AssetIcon({ code, iconUrl, size = 32 }: AssetIconProps) {
  const letter = (code ?? '?').trim().charAt(0).toUpperCase() || '?';
  const { bg, fg } = assetColor(code);
  return (
    <Avatar
      src={safeHttpUrl(iconUrl) ?? undefined}
      alt=""
      sx={{
        width: size,
        height: size,
        fontSize: size * 0.42,
        fontWeight: 600,
        bgcolor: bg,
        color: fg,
        flexShrink: 0,
      }}
    >
      {letter}
    </Avatar>
  );
}
