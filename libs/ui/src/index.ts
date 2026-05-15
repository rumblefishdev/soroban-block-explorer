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

export { Chip, type ChipProps } from './components/Chip.js';

export {
  TableSkeleton,
  CardSkeleton,
  DetailSkeleton,
  SearchSpinner,
  NotFoundState,
  type NotFoundEntity,
  TransientErrorState,
  RateLimitState,
  GenericErrorState,
  EmptyState,
  type EmptyStateVariant,
  SectionErrorBoundary,
  classifyError,
  type ErrorKind,
} from './states/index.js';

export {
  RelativeTimestamp,
  PollingIndicator,
  formatRelative,
  useNow,
} from './timestamps/index.js';

export {
  ExplorerTable,
  PaginationControls,
  TableSectionHeader,
  TableEmptyState,
  useTableUrlState,
  useCursorPagination,
} from './table/index.js';
export type {
  ExplorerTableColumn,
  ExplorerTableProps,
  SortDirection,
  PaginationControlsProps,
  TableSectionHeaderProps,
  TableEmptyStateProps,
  TableEmptyKind,
  TableUrlState,
  UseTableUrlStateOptions,
  UseTableUrlStateResult,
  UseCursorPaginationResult,
} from './table/index.js';

export {
  NetworkSwitcher,
  type Network,
  type NetworkSwitcherProps,
  NavButton,
  type NavButtonProps,
  type NavButtonSize,
  SearchInput,
  type SearchInputProps,
  type SearchInputSize,
  TopNav,
  type TopNavProps,
  type NetworkStats,
  SecondaryNav,
  type SecondaryNavProps,
  type NavItem,
  Footer,
  type FooterProps,
  type FooterNavItem,
} from './layout/index.js';

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
