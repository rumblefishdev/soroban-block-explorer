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
  monoFontFamily,
} from './theme/index.js';

export {
  CopyButton,
  type CopyButtonProps,
  IdentifierDisplay,
  type IdentifierDisplayProps,
  IdentifierWithCopy,
  type IdentifierWithCopyProps,
  getIdentifierHref,
  getDefaultTruncation,
  truncateMiddle,
  type EntityType,
  type TruncationConfig,
  isAccountId,
  isContractId,
  isLedgerSequence,
  isTransactionHash,
  isValidIdentifier,
} from './identifiers/index.js';
