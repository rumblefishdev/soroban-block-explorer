import { useCallback, useMemo } from 'react';
import { useSearchParams } from 'react-router-dom';

export interface UseTabUrlStateOptions {
  /** Query param name holding the active tab. Defaults to 'tab'. */
  param?: string;
  /** Tab key used when the param is absent or invalid. */
  defaultKey: string;
  /** Allowed tab keys; an out-of-range param falls back to defaultKey. */
  validKeys?: readonly string[];
}

export interface UseTabUrlStateResult {
  activeKey: string;
  setActiveKey: (key: string) => void;
}

/**
 * Syncs the active tab to a URL query param via React Router — mirrors
 * `useTableUrlState`. Tab changes update the param (replace, no history
 * entry) so there is no hard reload and the tab is shareable/back-safe.
 */
export function useTabUrlState({
  param = 'tab',
  defaultKey,
  validKeys,
}: UseTabUrlStateOptions): UseTabUrlStateResult {
  const [params, setParams] = useSearchParams();

  const activeKey = useMemo(() => {
    const value = params.get(param);
    if (value && (!validKeys || validKeys.includes(value))) return value;
    return defaultKey;
  }, [params, param, defaultKey, validKeys]);

  const setActiveKey = useCallback(
    (key: string) => {
      setParams(
        (prev) => {
          const next = new URLSearchParams(prev);
          // The default tab is the bare URL — no redundant ?tab= param.
          if (key === defaultKey) next.delete(param);
          else next.set(param, key);
          return next;
        },
        { replace: true }
      );
    },
    [setParams, param, defaultKey]
  );

  return { activeKey, setActiveKey };
}
