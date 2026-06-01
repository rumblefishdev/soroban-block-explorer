import { Box } from '@mui/material';
import type { PoolAssetLeg } from '@rumblefish/api-types';

import { AssetIcon } from '../assets/AssetIcon.js';
import { iconKindFor } from '../assets/assetType.js';
import { assetLegLabel } from '../pool-detail/helpers.js';

/**
 * A liquidity pool is two assets. Render each leg with the same
 * `AssetIcon` used everywhere else (icon-or-letter fallback + `kind`
 * colour, via `iconKindFor` like the assets list / balances), laid out as
 * an overlapping coin pair. The 2px ring + negative margin live here, in
 * the pair layout, so `AssetIcon` stays a plain single-asset avatar.
 */
export function PoolAssetPair({
  a,
  b,
  size = 32,
}: {
  a: PoolAssetLeg;
  b: PoolAssetLeg;
  size?: number;
}) {
  return (
    <Box
      sx={(theme) => ({
        display: 'flex',
        alignItems: 'center',
        '& .MuiAvatar-root': {
          border: `2px solid ${theme.palette.surface.grayMain}`,
        },
        '& .MuiAvatar-root:not(:first-of-type)': { marginLeft: '-8px' },
      })}
    >
      <AssetIcon
        code={assetLegLabel(a)}
        iconUrl={a.icon_url}
        kind={iconKindFor(a.asset_type_name)}
        size={size}
      />
      <AssetIcon
        code={assetLegLabel(b)}
        iconUrl={b.icon_url}
        kind={iconKindFor(b.asset_type_name)}
        size={size}
      />
    </Box>
  );
}
