import type { PaletteMode, PaletteOptions } from '@mui/material/styles';

import { colorsDark, colorsLight } from './colors.js';

import './types.js';

export function createPalette(mode: PaletteMode): PaletteOptions {
  const c = mode === 'light' ? colorsLight : colorsDark;

  return {
    mode,

    primary: {
      main: c.gray[900],
      light: c.gray[800],
      dark: c.gray[950],
      contrastText: c.base.white,
    },

    secondary: {
      main: c.text.accent,
      light: c.surface.primaryLight,
      dark: c.surface.primaryHover,
      contrastText: c.base.black,
    },

    success: {
      main: c.stroke.success,
      light: c.surface.success,
      dark: c.text.success,
      contrastText: c.base.white,
    },

    error: {
      main: c.stroke.error,
      light: c.surface.error,
      dark: c.text.error,
      contrastText: c.base.white,
    },

    warning: {
      main: c.stroke.warning,
      light: c.surface.warning,
      dark: c.text.warning,
      contrastText: c.base.white,
    },

    info: {
      main: c.yellow[500],
      light: c.surface.information,
      dark: c.yellow[700],
      contrastText: c.text.primary,
    },

    text: {
      primary: c.text.primary,
      secondary: c.text.secondary,
      disabled: c.gray[400],
      tertiary: c.text.tertiary,
      inverted: c.text.inverted,
      accent: c.text.accent,
      success: c.text.success,
      warning: c.text.warning,
      error: c.text.error,
    },

    background: {
      default: c.surface.background,
      paper: mode === 'light' ? c.base.white : c.gray[800],
    },

    divider: c.stroke.default,

    common: {
      black: c.base.black,
      white: c.base.white,
    },

    grey: {
      50: c.gray[50],
      100: c.gray[100],
      200: c.gray[200],
      300: c.gray[300],
      400: c.gray[400],
      500: c.gray[500],
      600: c.gray[600],
      700: c.gray[700],
      800: c.gray[800],
      900: c.gray[900],
    },

    surface: { ...c.surface },
    stroke: { ...c.stroke },
  };
}
