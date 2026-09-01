import { Box } from '@mui/material';

import { AssetIcon } from '../assets/AssetIcon.js';
import type { PoolLegView } from './helpers.js';

/**
 * A pool's legs as overlapping coin avatars — 2 for every classic pool,
 * 2–4 for soroban AMM pools (task 0374). Each leg colours itself per asset
 * identity (`assetColor`), so the same asset reads the same colour here, on
 * its detail page, and in the reserve dots. The 2px ring + negative margin
 * live here, in the pair layout, so `AssetIcon` stays a plain single-asset
 * avatar.
 */
export function PoolAssetPair({
  legs,
  size = 32,
}: {
  legs: readonly PoolLegView[];
  size?: number;
}) {
  return (
    <Box
      sx={(theme) => ({
        display: 'flex',
        alignItems: 'center',
        '& .MuiAvatar-root': {
          // The ring separates the overlapping avatars from the surface
          // behind them. Dark matches the card fill, so it reads as negative
          // space; light sits on white, where that trick is invisible and it
          // needs a real hairline instead.
          border:
            theme.palette.mode === 'light'
              ? `1px solid ${theme.palette.stroke.default}`
              : `2px solid ${theme.palette.surface.grayMain}`,
        },
        '& .MuiAvatar-root:not(:first-of-type)': { marginLeft: '-8px' },
      })}
    >
      {legs.map((leg, i) => (
        <AssetIcon
          key={`${leg.label}-${i}`}
          code={leg.label}
          iconUrl={leg.iconUrl ?? undefined}
          size={size}
        />
      ))}
    </Box>
  );
}
