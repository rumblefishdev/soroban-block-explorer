import { useCallback } from 'react';
import { useLocation, useNavigate } from 'react-router-dom';

const OP_HASH = /^#op-(\d+)$/;

/**
 * Resolve the `#op-N` fragment (1-based) into an index that addresses an
 * operation this transaction actually has.
 *
 * `count` of 0 does not mean "no such operation" — it is also "still loading"
 * and "the archive fetch failed", and the section renders its unavailable
 * state for those. So an unjudgeable fragment behaves exactly like an absent
 * one rather than asserting anything (0377).
 *
 * Out of range resolves to the first operation WITHOUT announcing it, and that
 * is deliberate: the card labels itself from `application_order`, not from the
 * fragment, so the reader sees a correctly numbered operation. Nothing is
 * hidden and nothing is mislabelled — the only way here is a hand-edited URL,
 * and a notice for that case was cut as unearned (task 0482).
 */
export function resolveOp(hash: string, count: number): number {
  const match = OP_HASH.exec(hash);
  if (match == null || count <= 0) return 0;

  const requested = Number(match[1]);
  return requested >= 1 && requested <= count ? requested - 1 : 0;
}

/**
 * Selected operation index, deep-linkable as `#op-N` (1-based) so a link can
 * point at one operation of a transaction.
 *
 * The hook owns the index's validity, the way `useTableUrlState` owns
 * `sort`/`dir`: user-supplied URL state is normalised where it is read, so
 * nothing downstream defends against a value that cannot happen. It used to
 * clamp only the lower bound and let anything above the list through, which
 * left the card showing operation 1 while the picker beside it highlighted
 * nothing.
 */
export function useSelectedOp(
  count: number
): [number, (index: number) => void] {
  const location = useLocation();
  const navigate = useNavigate();

  const selected = resolveOp(location.hash, count);

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
