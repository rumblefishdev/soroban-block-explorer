import Box from '@mui/material/Box';

import type { Network } from '../network.js';
import { SWITCHER_FROM, tabColors } from './NetworkSwitcher.js';

/**
 * On phones the switcher lives in the menu drawer, so the header would no
 * longer say which network this is. A testnet page must never pass for
 * mainnet (task 0553), so testnet keeps a badge beside the logo, in the
 * switcher's testnet colours. Mainnet, the default, shows none.
 */
export function TestnetBadge({ current }: { current: Network }) {
  if (current !== 'testnet') return null;

  return (
    <Box
      sx={(theme) => {
        const colors = tabColors(theme, 'testnet');
        return {
          display: { xs: 'inline-flex', [SWITCHER_FROM]: 'none' },
          alignItems: 'center',
          flexShrink: 0,
          px: 0.75,
          py: 0.25,
          borderRadius: `${theme.shape.radius.s}px`,
          border: `1px solid ${colors.border}`,
          backgroundColor: colors.fill,
          color: colors.text,
          ...theme.typography.bodyXsMedium,
          letterSpacing: '0.04em',
        };
      }}
    >
      TESTNET
    </Box>
  );
}
