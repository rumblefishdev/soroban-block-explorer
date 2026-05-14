import { useMemo } from 'react';

import Box from '@mui/material/Box';
import type { SxProps, Theme } from '@mui/material/styles';

import { monoFontFamily } from '../theme/typography.js';

import { getIdentifierHref } from './routes.js';
import { getDefaultTruncation, truncateMiddle } from './truncate.js';
import type { EntityType, TruncationConfig } from './types.js';

function makeMonoSx(linked: boolean, fullWidth: boolean): SxProps<Theme> {
  return {
    fontFamily: monoFontFamily,
    fontSize: 14,
    fontWeight: 500,
    lineHeight: 1.4,
    color: (theme) => theme.palette.text.primary,
    '&:visited': {
      color: (theme: Theme) => theme.palette.text.primary,
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
      ? { color: (theme: Theme) => theme.palette.surface.primaryMainAlt }
      : undefined,
    '&:focus-visible': {
      outline: (theme) => `2px solid ${theme.palette.stroke.action}`,
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
  className,
  'aria-label': ariaLabel,
}: IdentifierDisplayProps) {
  const cfg = truncation ?? getDefaultTruncation(type);
  const formatted = formatForDisplay(type, value);
  const displayText = truncate ? truncateMiddle(formatted, cfg) : formatted;
  const sx = useMemo(() => makeMonoSx(linked, !truncate), [linked, truncate]);

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
