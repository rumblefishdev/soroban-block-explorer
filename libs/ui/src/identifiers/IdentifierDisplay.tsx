import { useMemo } from 'react';

import Box from '@mui/material/Box';
import type { SxProps, Theme } from '@mui/material/styles';

import { monoFontFamily } from '../theme/typography.js';

import { getIdentifierHref } from './routes.js';
import { getDefaultTruncation, truncateMiddle } from './truncate.js';
import type { EntityType, TruncationConfig } from './types.js';

function makeMonoSx(
  linked: boolean,
  fullWidth: boolean,
  tone: 'default' | 'inherit',
  fontSize: number | string
): SxProps<Theme> {
  const inheritColor = tone === 'inherit';
  return {
    fontFamily: monoFontFamily,
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
  className?: string;
  'aria-label'?: string;
}

function formatForDisplay(type: EntityType, value: string): string {
  if (type === 'ledger' && /^\d+$/.test(value)) {
    return Number(value).toLocaleString('en-US');
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
  className,
  'aria-label': ariaLabel,
}: IdentifierDisplayProps) {
  const cfg = truncation ?? getDefaultTruncation(type);
  const formatted = formatForDisplay(type, value);
  const displayText = truncate ? truncateMiddle(formatted, cfg) : formatted;
  const sx = useMemo(
    () => makeMonoSx(linked, !truncate, tone, fontSize),
    [linked, truncate, tone, fontSize]
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
