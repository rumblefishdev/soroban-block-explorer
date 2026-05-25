import { useCallback } from 'react';
import { useSearchParams } from 'react-router-dom';

export type DetailMode = 'normal' | 'advanced';

const MODE_PARAM = 'mode';

export interface UseDetailModeResult {
  mode: DetailMode;
  setMode: (next: DetailMode) => void;
}

export function useDetailMode(): UseDetailModeResult {
  const [searchParams, setSearchParams] = useSearchParams();
  const raw = searchParams.get(MODE_PARAM);
  const mode: DetailMode = raw === 'advanced' ? 'advanced' : 'normal';

  const setMode = useCallback(
    (next: DetailMode) => {
      setSearchParams(
        (prev) => {
          const updated = new URLSearchParams(prev);
          if (next === 'normal') updated.delete(MODE_PARAM);
          else updated.set(MODE_PARAM, next);
          return updated;
        },
        { replace: true }
      );
    },
    [setSearchParams]
  );

  return { mode, setMode };
}
