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
  formatAmount,
  formatCompactAmount,
  STROOPS_PER_XLM_BIGINT,
  stroopsToXlmString,
  formatFee,
  formatStroops,
  formatInteger,
  formatTps,
  formatPercent,
} from './format/index.js';

export { useDebouncedDraft } from './hooks/index.js';

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
  isMissingResource,
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
  usePageHandlers,
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
  PageInfoLike,
  UsePageHandlersResult,
} from './table/index.js';

export {
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
  type FooterSystemStatus,
  PageGridBackdrop,
} from './layout/index.js';

export {
  CopyButton,
  type CopyButtonProps,
  IdentifierDisplay,
  type IdentifierDisplayProps,
  IdentifierWithCopy,
  type IdentifierWithCopyProps,
  entityRoutes,
  getIdentifierHref,
  routeForHit,
  ELLIPSIS_CHAR,
  getDefaultTruncation,
  truncateMiddle,
  type EntityType,
  type TruncationConfig,
  isAccountId,
  isContractId,
  isLedgerSequence,
  isPoolId,
  isTransactionHash,
  isValidIdentifier,
} from './identifiers/index.js';

export {
  Tabs,
  useTabUrlState,
  TimeSeriesChart,
  DEFAULT_TIME_SERIES_INTERVALS,
  OperationFlowTree,
  LazySection,
  useIntersectionObserver,
} from './visualization/index.js';
export type {
  TabsProps,
  TabDefinition,
  UseTabUrlStateOptions,
  UseTabUrlStateResult,
  TimeSeriesChartProps,
  TimeSeriesPoint,
  TimeSeriesInterval,
  OperationFlowTreeProps,
  FlowNode,
  FlowNodeKind,
  FlowNodeIdentifier,
  LazySectionProps,
  UseIntersectionObserverOptions,
  UseIntersectionObserverResult,
} from './visualization/index.js';
