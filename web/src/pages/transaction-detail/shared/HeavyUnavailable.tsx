import WarningAmberOutlinedIcon from '@mui/icons-material/WarningAmberOutlined';
import { EmptyState } from '@rumblefish/soroban-block-explorer-ui';

/**
 * Shown when transaction data fetched from the Stellar history archive is
 * missing. Signatures, events and raw XDR live only in that block, so
 * defaulting them to `[]` makes a section render a confident "0 / none" for
 * something nothing measured. "Could not load" and "there are none" are
 * different facts and must not share a rendering (task 0377 F1/F2).
 *
 * Callers must NOT gate this on `heavy == null` alone: the archive can answer
 * while an individual transaction's envelope is missing, in which case the
 * heavy block exists with empty fields.
 *
 * `description` is optional and deliberately omitted by most callers: several
 * sections can be missing at once from the SAME cause, and repeating one
 * sentence four times down a page reads as four failures rather than one. The
 * operations strip carries the explanation for the page.
 *
 * No retry affordance here. The obvious one — refetching the whole transaction
 * — belongs to the page, not to a section that cannot own the query, and a
 * per-section button would fire N identical refetches. If it is ever wanted,
 * pass `query.refetch` down from `index.tsx` rather than reintroducing a prop
 * nothing supplies.
 *
 * Rendered in the neutral tone on purpose. One archive miss can empty four
 * sections at once, and four amber blocks down a page read as four failures
 * rather than one degraded fetch. The page keeps exactly one alarm-coloured
 * element — the operations strip — and these state the same fact quietly. The
 * warning glyph is kept so the state still reads as "attention", not "nothing
 * here".
 */
export function HeavyUnavailable({
  what,
  description,
}: {
  what: string;
  description?: string;
}) {
  return (
    <EmptyState
      icon={<WarningAmberOutlinedIcon />}
      title={`${what} unavailable`}
      description={description}
      py={4}
    />
  );
}
