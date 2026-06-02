import { useCallback, useRef, useState } from 'react';

export interface UseIntersectionObserverOptions {
  /** Margin around the root — lets content load just before it scrolls in. */
  rootMargin?: string;
  /** Visibility ratio that triggers intersection. */
  threshold?: number | number[];
  /** Stop observing once the element first becomes visible. Defaults to true. */
  once?: boolean;
}

export interface UseIntersectionObserverResult<T extends Element> {
  /** Callback ref to attach to the element being observed. */
  ref: (node: T | null) => void;
  /** True once the element has intersected the viewport. */
  isIntersecting: boolean;
}

/**
 * Observe a single element's viewport visibility via IntersectionObserver.
 * Falls back to eager visibility when the API is unavailable (e.g. SSR).
 */
export function useIntersectionObserver<T extends Element = HTMLDivElement>(
  options: UseIntersectionObserverOptions = {}
): UseIntersectionObserverResult<T> {
  const { rootMargin = '200px', threshold = 0, once = true } = options;
  const [isIntersecting, setIsIntersecting] = useState(false);
  const observerRef = useRef<IntersectionObserver | null>(null);

  const ref = useCallback(
    (node: T | null) => {
      observerRef.current?.disconnect();
      observerRef.current = null;
      if (!node) return;

      if (typeof IntersectionObserver === 'undefined') {
        // No IO support: render eagerly rather than hiding content forever.
        setIsIntersecting(true);
        return;
      }

      const observer = new IntersectionObserver(
        (entries) => {
          const entry = entries[0];
          if (!entry) return;
          if (entry.isIntersecting) {
            setIsIntersecting(true);
            if (once) observer.disconnect();
          } else if (!once) {
            setIsIntersecting(false);
          }
        },
        { rootMargin, threshold }
      );
      observer.observe(node);
      observerRef.current = observer;
    },
    [rootMargin, threshold, once]
  );

  return { ref, isIntersecting };
}
