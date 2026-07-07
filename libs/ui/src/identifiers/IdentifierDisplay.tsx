import { useMemo } from 'react';

import Box from '@mui/material/Box';
import type { SxProps, Theme } from '@mui/material/styles';

import { formatInteger } from '../format/index.js';
import { monoFontFamily, secondaryFontFamily } from '../theme/typography.js';

import { getIdentifierHref } from './routes.js';
import { getDefaultTruncation, truncateMiddle } from './truncate.js';
import type { EntityType, TruncationConfig } from './types.js';

function makeIdentifierSx(
  linked: boolean,
  fullWidth: boolean,
  tone: 'default' | 'inherit',
  fontSize: number | string,
  mono: boolean
): SxProps<Theme> {
  const inheritColor = tone === 'inherit';
  return {
    fontFamily: mono ? monoFontFamily : secondaryFontFamily,
    fontSize,
    fontWeight: 500,
    lineHeight: 1.4,
    color: inheritColor
      ? 'inherit'
      : (theme: Theme) => theme.palette.text.primary,
    '&:visited': {
      color: inheritColor
        ? 'inherit'
        : (theme: Theme) => theme.palette.text.primary,
    },
    textDecoration: 'none',
    display: 'inline-flex',
    alignItems: 'center',
    maxWidth: fullWidth ? '100%' : undefined,
    overflow: fullWidth ? 'hidden' : 'visible',
    textOverflow: fullWidth ? 'ellipsis' : 'clip',
    whiteSpace: 'nowrap',
    cursor: linked ? 'pointer' : 'inherit',
    '&:hover': linked
      ? inheritColor
        ? { textDecoration: 'underline' }
        : { color: (theme: Theme) => theme.palette.surface.primaryMainAlt }
      : undefined,
    '&:focus-visible': {
      outline: inheritColor
        ? '2px solid currentColor'
        : (theme: Theme) => `2px solid ${theme.palette.stroke.action}`,
      outlineOffset: 2,
      borderRadius: 2,
    },
  };
}

export interface IdentifierDisplayProps {
  value: string;
  type: EntityType;
  truncate?: boolean;
  truncation?: TruncationConfig;
  linked?: boolean;
  href?: string;
  /**
   * 'inherit' makes the link adopt the surrounding text colour — needed when
   * rendered on coloured backgrounds (e.g. flow-tree node cards). Defaults to
   * 'default' (theme text colour).
   */
  tone?: 'default' | 'inherit';
  /**
   * Font size of the rendered identifier. Defaults to 14. Pass `'inherit'`
   * when rendered inline inside smaller surrounding text (e.g. a reserves
   * cell) so the identifier matches the adjacent value rather than forcing
   * its own size.
   */
  fontSize?: number | string;
  /**
   * Force the monospace font on/off, overriding the type-driven default. Needed
   * when an entity's `type` doesn't match the value shape — e.g. a ledger's
   * HASH cell uses `type="ledger"` (no link/route) but must render mono like
   * every other hash so the truncated `XXXX…XXXX` is fixed-width (copy buttons
   * line up). Defaults to the per-type heuristic.
   */
  mono?: boolean;
  className?: string;
  'aria-label'?: string;
}

function formatForDisplay(type: EntityType, value: string): string {
  if (type === 'ledger' && /^\d+$/.test(value)) {
    return formatInteger(Number(value));
  }
  return value;
}

export function IdentifierDisplay({
  value,
  type,
  truncate = true,
  truncation,
  linked = true,
  href,
  tone = 'default',
  fontSize = 14,
  mono,
  className,
  'aria-label': ariaLabel,
}: IdentifierDisplayProps) {
  // Type-driven font: opaque identifiers (hashes, addresses, contract / tx /
  // pool ids) read in the mono font where fixed width aids scanning. Asset
  // "ids" are human ticker codes (USDC, AQUA) and ledger ids are plain
  // sequence numbers — both read in the body font like the value beside
  // them (matches Figma). Ledger stays a link, just not mono — but a ledger
  // HASH cell overrides via `mono` (see prop doc).
  const isMono = mono ?? (type !== 'asset' && type !== 'ledger');
  const cfg = truncation ?? getDefaultTruncation(type);
  const formatted = formatForDisplay(type, value);
  const displayText = truncate ? truncateMiddle(formatted, cfg) : formatted;
  const sx = useMemo(
    () => makeIdentifierSx(linked, !truncate, tone, fontSize, isMono),
    [linked, truncate, tone, fontSize, isMono]
  );

  // NFT identity is composite `(contract_id, token_id)`; pass `href`
  // explicitly when `type='nft'` (no production callsite does today).
  // `getIdentifierHref` throws on `'nft'` if reached without an
  // override — safety net, not hot path.
  return (
    <Box
      component={linked ? 'a' : 'span'}
      {...(linked && { href: href ?? getIdentifierHref(type, value) })}
      className={className}
      aria-label={ariaLabel ?? value}
      sx={sx}
    >
      {displayText}
    </Box>
  );
}
