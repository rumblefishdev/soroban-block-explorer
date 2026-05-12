import {
  createTheme,
  type PaletteMode,
  type Theme,
} from '@mui/material/styles';

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
    shadows: muiShadows,
  });
}
