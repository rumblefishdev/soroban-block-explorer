import { useCallback } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';

const OP_HASH = /^#op-(\d+)$/;

/** Selected operation index, deep-linkable as `#op-N` (1-based) so a link can
 *  point at one operation of a transaction. Invalid or absent hash → 0. */
export function useSelectedOp(): [number, (index: number) => void] {
  const location = useLocation();
  const navigate = useNavigate();

  const match = OP_HASH.exec(location.hash);
  const selected = match != null ? Math.max(0, Number(match[1]) - 1) : 0;

  const setSelected = useCallback(
    (index: number) => {
      navigate(
        {
          pathname: location.pathname,
          search: location.search,
          hash: `#op-${index + 1}`,
        },
        { replace: true }
      );
    },
    [navigate, location.pathname, location.search]
  );

  return [selected, setSelected];
}
