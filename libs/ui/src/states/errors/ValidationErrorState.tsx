import FilterAltOutlinedIcon from '@mui/icons-material/FilterAltOutlined';

import { EmptyState } from '../empty/EmptyState.js';

interface ValidationErrorStateProps {
  /** The API's own message — `ErrorEnvelope.message`, already surfaced as
   *  `Error.message` by the client interceptor. */
  message?: string;
  py?: number;
}

/**
 * A 400/422: the REQUEST was wrong, not the service. Distinct from the
 * generic error on both counts that matter — it says what to change, using
 * the API's own wording ("asset pair must be `A/B`, each side at least 2
 * characters"), and it offers NO retry, because resending identical input
 * fails identically. "Something went wrong · Try again" was both wrong about
 * the cause and useless as an action.
 */
export function ValidationErrorState({
  message,
  py,
}: ValidationErrorStateProps) {
  return (
    <EmptyState
      icon={<FilterAltOutlinedIcon />}
      title="That filter isn't valid"
      description={message ?? 'Adjust the filter and try again.'}
      py={py}
    />
  );
}
