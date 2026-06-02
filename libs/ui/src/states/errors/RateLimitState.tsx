import HourglassEmptyIcon from '@mui/icons-material/HourglassEmpty';
import { Button } from '@mui/material';
import { useEffect, useState } from 'react';

import { EmptyState } from '../empty/EmptyState.js';

interface RateLimitStateProps {
  retryAfterSeconds?: number;
  onRetry?: () => void;
  py?: number;
}

export function RateLimitState({
  retryAfterSeconds,
  onRetry,
  py,
}: RateLimitStateProps) {
  const [remaining, setRemaining] = useState(retryAfterSeconds ?? 0);

  useEffect(() => {
    if (!retryAfterSeconds || !onRetry) return;
    setRemaining(retryAfterSeconds);
    const interval = setInterval(() => {
      setRemaining((prev) => {
        if (prev <= 1) {
          clearInterval(interval);
          onRetry();
          return 0;
        }
        return prev - 1;
      });
    }, 1000);
    return () => clearInterval(interval);
  }, [retryAfterSeconds, onRetry]);

  const countdown =
    retryAfterSeconds && onRetry && remaining > 0
      ? `Retrying in ${remaining}s…`
      : undefined;

  return (
    <EmptyState
      icon={<HourglassEmptyIcon />}
      variant="warning"
      title="Too many requests"
      description="Too many requests. Please try again shortly."
      meta={countdown}
      py={py}
      action={
        !retryAfterSeconds && onRetry ? (
          <Button variant="contained" onClick={onRetry}>
            Try again
          </Button>
        ) : undefined
      }
    />
  );
}
