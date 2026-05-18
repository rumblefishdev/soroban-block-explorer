import { Avatar } from '@mui/material';

import { safeHttpUrl } from '../url.js';

interface AssetIconProps {
  /** Asset code — its first letter is the fallback when no icon is available. */
  code?: string | null;
  /** Asset icon URL from metadata; falls back to a letter avatar when absent. */
  iconUrl?: string | null;
  size?: number;
}

/**
 * Round asset icon used in the assets table, account balances, and the asset
 * detail header. Renders the metadata icon when present, otherwise a letter
 * avatar derived from the asset code.
 */
export function AssetIcon({ code, iconUrl, size = 32 }: AssetIconProps) {
  const letter = (code ?? '?').trim().charAt(0).toUpperCase() || '?';
  return (
    <Avatar
      src={safeHttpUrl(iconUrl) ?? undefined}
      alt=""
      sx={{
        width: size,
        height: size,
        fontSize: size * 0.42,
        fontWeight: 600,
        bgcolor: 'surface.grayMain',
        color: 'text.secondary',
        flexShrink: 0,
      }}
    >
      {letter}
    </Avatar>
  );
}
