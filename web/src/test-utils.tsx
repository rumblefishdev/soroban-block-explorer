import { ExplorerThemeProvider } from '@rumblefish/soroban-block-explorer-ui';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import {
  render,
  type RenderOptions,
  type RenderResult,
} from '@testing-library/react';
import type { ReactElement, ReactNode } from 'react';
import { MemoryRouter, Route, Routes } from 'react-router-dom';

interface RenderWithProvidersOptions extends Omit<RenderOptions, 'wrapper'> {
  initialEntries?: string[];
  routePath?: string;
  queryClient?: QueryClient;
}

/**
 * Renders a UI tree inside the providers every page expects:
 * `QueryClientProvider`, `ExplorerThemeProvider`, and `MemoryRouter`.
 * Pass `routePath` (e.g. `/accounts/:accountId`) together with
 * `initialEntries` to drive `useParams` in page-component tests.
 */
export function renderWithProviders(
  ui: ReactElement,
  options: RenderWithProvidersOptions = {}
): RenderResult & { queryClient: QueryClient } {
  const {
    initialEntries = ['/'],
    routePath,
    queryClient = makeTestQueryClient(),
    ...rest
  } = options;

  function Wrapper({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={queryClient}>
        <ExplorerThemeProvider>
          <MemoryRouter initialEntries={initialEntries}>
            {routePath != null ? (
              <Routes>
                <Route path={routePath} element={children} />
              </Routes>
            ) : (
              children
            )}
          </MemoryRouter>
        </ExplorerThemeProvider>
      </QueryClientProvider>
    );
  }

  const result = render(ui, { wrapper: Wrapper, ...rest });
  return { ...result, queryClient };
}

/**
 * QueryClient tuned for tests — no retries, no caching, no stale window
 * so each test gets deterministic fetch behaviour.
 */
export function makeTestQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false, gcTime: 0, staleTime: 0 },
      mutations: { retry: false },
    },
  });
}
