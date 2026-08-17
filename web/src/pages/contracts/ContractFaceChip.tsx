import { Tooltip } from '@mui/material';
import { Chip } from '@rumblefish/soroban-block-explorer-ui';
import { Link as RouterLink } from 'react-router-dom';

import type { ContractFace } from './contractFace.js';

/**
 * Renders a contract's face (task 0472) as a chip, linked when it has a
 * target. ONE component for the detail header and the list row — the two
 * surfaces had drifted (accent vs brown, hand-rolled aria) when each built
 * the same chip by hand.
 *
 * A11y contract (review, 2026-08-13): the anchor is the ONLY interactive
 * element — the chip must not be `clickable` (MUI would nest a keyboard-dead
 * `role="button"` inside the link) — and the tooltip uses `describeChild`, so
 * the visible label stays the accessible NAME (WCAG 2.5.3) with the issuer as
 * a description.
 */
export function ContractFaceChip({
  face,
  size,
}: {
  face: ContractFace;
  size: 'sm' | 'md';
}) {
  const chip = <Chip size={size} color={face.meta.color} label={face.label} />;
  if (!face.href) return chip;
  return (
    <Tooltip title={face.title ?? ''} describeChild>
      <RouterLink to={face.href} style={{ textDecoration: 'none' }}>
        {chip}
      </RouterLink>
    </Tooltip>
  );
}
