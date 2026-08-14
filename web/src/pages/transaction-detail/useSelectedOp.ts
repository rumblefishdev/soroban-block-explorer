import { useCallback } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';

const OP_HASH = /^#op-(\d+)$/;

export interface OpSelection {
  /** Always addresses an existing operation — 0 when there is no list to
   *  address. Consumers index with it directly and carry no range guard. */
  index: number;
  /** The 1-based number the URL asked for, when it names an operation the
   *  transaction does not have. `null` when the URL is silent, names a real
   *  operation, or when there is no list to judge it against yet. */
  missing: number | null;
}

/**
 * Resolve the `#op-N` fragment (1-based) against the operations actually on
 * the page.
 *
 * `count` of 0 is NOT "this transaction has no operation N" — it is also
 * "still loading" and "the archive fetch failed". Answering it would assert a
 * count nobody measured (0377), so an unjudgeable fragment reports nothing and
 * selects the first operation, exactly as an absent fragment does.
 */
export function resolveOp(hash: string, count: number): OpSelection {
  const match = OP_HASH.exec(hash);
  if (match == null || count <= 0) return { index: 0, missing: null };

  const requested = Number(match[1]);
  if (requested >= 1 && requested <= count) {
    return { index: requested - 1, missing: null };
  }
  return { index: 0, missing: requested };
}

/**
 * Selected operation index, deep-linkable as `#op-N` (1-based) so a link can
 * point at one operation of a transaction.
 *
 * This hook OWNS the validity of the index, the way `useTableUrlState` owns
 * the validity of `sort`/`dir`: user-supplied URL state is normalised where it
 * is read, so nothing downstream has to defend against a value that cannot
 * happen. It previously clamped only the lower bound (`Math.max(0, …)`) and
 * let anything above the list escape, which pushed the decision onto the
 * section — where it turned into hiding the operation the reader came for.
 *
 * A fragment naming a missing operation is reported, not rewritten: the
 * address bar keeps what the reader actually asked for, and the section says
 * plainly that it does not exist. Silently swapping in a different operation
 * (the behaviour before this) reads as an answer rather than a miss.
 */
export function useSelectedOp(
  count: number
): OpSelection & { select: (index: number) => void } {
  const location = useLocation();
  const navigate = useNavigate();

  const selection = resolveOp(location.hash, count);

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

  return { ...selection, select: setSelected };
}
