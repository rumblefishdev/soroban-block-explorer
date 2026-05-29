import ErrorOutlineIcon from '@mui/icons-material/ErrorOutline';
import { Button } from '@mui/material';

import { EmptyState } from '../empty/EmptyState.js';

interface TransientErrorStateProps {
  retryable?: boolean;
  onRetry?: () => void;
  description?: string;
  meta?: string;
  py?: number;
}

export function TransientErrorState({
  retryable = true,
  onRetry,
  description,
  meta,
  py,
}: TransientErrorStateProps) {
  const copy = retryable
    ? description ??
      "We couldn't load this data. This is likely a temporary issue — please try again."
    : description ??
      'The identifier looks invalid. Please check the value and try again.';

  return (
    <EmptyState
      icon={<ErrorOutlineIcon />}
      variant="error"
      title={retryable ? 'Something went wrong' : 'Invalid input'}
      description={copy}
      meta={meta}
      py={py}
      action={
        retryable && onRetry ? (
          <Button variant="contained" onClick={onRetry}>
            Try again
          </Button>
        ) : undefined
      }
    />
  );
}
