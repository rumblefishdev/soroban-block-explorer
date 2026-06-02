import { Box } from '@mui/material';
import type { ReactNode } from 'react';
import { useEffect, useRef } from 'react';

import { CardSkeleton } from '../states/skeletons/CardSkeleton.js';

import { useIntersectionObserver } from './useIntersectionObserver.js';

export interface LazySectionProps {
  children: ReactNode;
  /** Rendered until the section scrolls into view. Defaults to a CardSkeleton. */
  placeholder?: ReactNode;
  /** Viewport margin that triggers an early render. Defaults to '200px'. */
  rootMargin?: string;
  /** Reserved height for the placeholder, to avoid layout jump. */
  minHeight?: number | string;
  /** Fired once, the first time the section becomes visible. */
  onVisible?: () => void;
}

/**
 * Defers rendering of heavy visualizations (charts, trees) until the section
 * enters the viewport. Children mount only once visible — so any data fetching
 * inside them is deferred too. Presentational only; it does not fetch.
 */
export function LazySection({
  children,
  placeholder,
  rootMargin = '200px',
  minHeight = 240,
  onVisible,
}: LazySectionProps) {
  const { ref, isIntersecting } = useIntersectionObserver<HTMLDivElement>({
    rootMargin,
    once: true,
  });
  const notified = useRef(false);

  useEffect(() => {
    if (isIntersecting && !notified.current) {
      notified.current = true;
      onVisible?.();
    }
  }, [isIntersecting, onVisible]);

  return (
    <Box ref={ref} sx={{ minHeight: isIntersecting ? undefined : minHeight }}>
      {isIntersecting ? children : placeholder ?? <CardSkeleton lines={4} />}
    </Box>
  );
}
