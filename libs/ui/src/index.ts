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
