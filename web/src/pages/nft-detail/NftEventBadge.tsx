import { Box } from '@mui/material';
import { colorsDark } from '@rumblefish/soroban-block-explorer-ui';

interface EventStyle {
  bg: string;
  fg: string;
}

// NFT lifecycle events use fixed brand-scale pills (theme-independent),
// matching the Figma "NFT Transfer history badge" component.
const EVENT_STYLES: Record<string, EventStyle> = {
  mint: { bg: colorsDark.violet[100], fg: colorsDark.violet[600] },
  transfer: { bg: colorsDark.blue[100], fg: colorsDark.blue[600] },
  burn: { bg: colorsDark.red[100], fg: colorsDark.red[700] },
};

const FALLBACK_STYLE: EventStyle = {
  bg: colorsDark.gray[900],
  fg: colorsDark.gray[300],
};

interface NftEventBadgeProps {
  /** `mint | transfer | burn`, or null when the API could not decode it. */
  name?: string | null;
}

/** Coloured pill for an NFT transfer event — violet/blue/red per kind. */
export function NftEventBadge({ name }: NftEventBadgeProps) {
  const key = name?.toLowerCase() ?? '';
  const style = EVENT_STYLES[key] ?? FALLBACK_STYLE;
  const label = key ? key.charAt(0).toUpperCase() + key.slice(1) : 'Event';

  return (
    <Box
      component="span"
      sx={(theme) => ({
        display: 'inline-flex',
        alignItems: 'center',
        px: 1,
        py: 0.25,
        borderRadius: `${theme.shape.radius.s}px`,
        backgroundColor: style.bg,
        color: style.fg,
        fontSize: 14,
        fontWeight: 500,
        lineHeight: 1.4,
        whiteSpace: 'nowrap',
      })}
    >
      {label}
    </Box>
  );
}
