import WarningAmberOutlinedIcon from '@mui/icons-material/WarningAmberOutlined';
import { Typography } from '@mui/material';
import { EmptyState } from '@rumblefish/soroban-block-explorer-ui';

/**
 * The two shapes of "we could not read this", kept in one file because the
 * choice between them is a choice of SCALE, not of meaning: a whole section is
 * missing, or a single value inside a row is.
 *
 * Both exist because signatures, events, raw XDR, the memo and the fee source
 * live only in the data fetched from the Stellar history archive. Defaulting
 * them to `[]` or to a dash makes the page state something nothing measured —
 * for signatures, something that essentially cannot be true. "Could not read"
 * and "there are none" are different facts and must not share a rendering
 * (task 0377 F1/F2).
 *
 * Callers must NOT gate these on "the archive block is absent" alone: the
 * archive can answer while an individual transaction's envelope is missing, in
 * which case the block exists carrying empty fields.
 */

/**
 * A whole section could not be read — signatures, events, raw XDR.
 *
 * Neutral tone on purpose. One archive miss can empty four sections at once,
 * and four alarm-coloured blocks down a page read as four failures rather than
 * one degraded fetch. The page keeps exactly one alarm-coloured element — the
 * operations strip — and these state the same fact quietly. The warning glyph
 * stays, so each still reads as "attention" rather than "nothing here".
 *
 * `description` is opt-in for the same reason: repeating one sentence four
 * times down a page is noise. Pass it only where a section can be the page's
 * ONLY sign of trouble.
 *
 * No retry affordance. Refetching the transaction belongs to the page, not to a
 * section that cannot own the query, and a per-section button would fire N
 * identical refetches.
 */
export function UnavailableSection({
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

/**
 * A single value in a summary row could not be read — the memo, the fee source.
 *
 * Deliberately NOT `<Dash />`: a dash already means "this genuinely has none"
 * in these tables, so reusing it would leave the two facts indistinguishable,
 * which is the defect itself. Same type scale as a real value — `text.tertiary`
 * alone carries "not a value", exactly as `Dash` does.
 */
export function UnavailableValue() {
  return (
    <Typography
      variant="bodySmMedium"
      sx={(theme) => ({ color: theme.palette.text.tertiary })}
    >
      Not available
    </Typography>
  );
}
