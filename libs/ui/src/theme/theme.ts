import {
  createTheme,
  type PaletteMode,
  type Theme,
} from '@mui/material/styles';

import { overrides } from './overrides.js';
import { createPalette } from './palette.js';
import { radius } from './radius.js';
import { muiShadows } from './shadows.js';
import { typography } from './typography.js';

export function createExplorerTheme(mode: PaletteMode): Theme {
  return createTheme({
    palette: createPalette(mode),
    typography,
    shape: {
      borderRadius: radius.md,
      radius,
    },
    zIndex: {
      pageGlow: 0,
      gridBackdrop: 0,
      contentMain: 1,
      secondaryNav: 2,
      footer: 2,
      topNav: 3,
    },
    shadows: muiShadows,
    components: overrides,
  });
}
