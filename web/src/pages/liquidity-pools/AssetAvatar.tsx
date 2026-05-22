import { Box } from '@mui/material';
import type { PoolAssetLeg } from '@rumblefish/api-types';

import { assetLegLabel } from '../pool-detail/helpers.js';

import { assetLegColor } from './assetColor.js';

/**
 * Pool list asset avatar — 32px circle, deterministic `{ bg, fg }`
 * from `assetLegColor`, with the leg's first letter drawn inside as a
 * placeholder until the backend exposes `icon_url` on `PoolAssetLeg`.
 * Matches the Figma list-page glyphs (node `266:36052` — XLM → `X`,
 * USDC → `U`, etc.).
 *
 * `overlap` adds the negative left-margin used by the second avatar
 * to sit half-tucked behind the first.
 */
export function AssetAvatar({
  leg,
  overlap = false,
}: {
  leg: PoolAssetLeg;
  overlap?: boolean;
}) {
  const { bg, fg } = assetLegColor(leg);
  const letter = assetLegLabel(leg).charAt(0).toUpperCase();
  return (
    <Box
      aria-hidden
      sx={(theme) => ({
        display: 'inline-flex',
        alignItems: 'center',
        justifyContent: 'center',
        width: 32,
        height: 32,
        borderRadius: '50%',
        backgroundColor: bg,
        color: fg,
        fontSize: 12,
        fontWeight: 700,
        lineHeight: 1,
        // 2px ring against the row surface so overlapping pairs read as
        // two distinct discs (Figma uses the table cell background).
        border: `2px solid ${theme.palette.surface.grayMain}`,
        marginLeft: overlap ? '-8px' : 0,
        flexShrink: 0,
      })}
    >
      {letter}
    </Box>
  );
}
