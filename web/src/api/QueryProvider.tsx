import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { ReactQueryDevtools } from '@tanstack/react-query-devtools';
import type { PropsWithChildren } from 'react';

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 60_000,
      gcTime: 5 * 60_000,
      refetchOnWindowFocus: true,
      // 1 retry on network/5xx, 0 retries on 4xx - status is stamped by the
      // client error interceptor in `./client.ts`.
      retry: (failureCount, error) => {
        const status = (error as { status?: number })?.status;
        if (typeof status === 'number' && status >= 400 && status < 500) {
          return false;
        }
        return failureCount < 1;
      },
    },
  },
});

export function QueryProvider({ children }: PropsWithChildren) {
  return (
    <QueryClientProvider client={queryClient}>
      {children}
      {import.meta.env.DEV ? (
        <ReactQueryDevtools initialIsOpen={false} />
      ) : null}
    </QueryClientProvider>
  );
}
