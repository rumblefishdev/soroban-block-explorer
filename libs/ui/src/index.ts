export interface NavigationItem {
  href: string;
  label: string;
}

export {
  colorsLight,
  colorsDark,
  type ColorScheme,
  grid,
  createPalette,
  radius,
  shadows,
  createExplorerTheme,
  ExplorerThemeProvider,
  useColorMode,
} from './theme/index.js';
