import ReportProblemOutlinedIcon from '@mui/icons-material/ReportProblemOutlined';
import { Button } from '@mui/material';

import { EmptyState } from '../empty/EmptyState.js';

interface GenericErrorStateProps {
  onRetry?: () => void;
  description?: string;
  meta?: string;
  py?: number;
}

export function GenericErrorState({
  onRetry,
  description,
  meta,
  py,
}: GenericErrorStateProps) {
  return (
    <EmptyState
      icon={<ReportProblemOutlinedIcon />}
      variant="error"
      title="Something went wrong"
      description={
        description ??
        'An unexpected error occurred while rendering this section.'
      }
      meta={meta}
      py={py}
      action={
        onRetry ? (
          <Button variant="contained" onClick={onRetry}>
            Try again
          </Button>
        ) : undefined
      }
    />
  );
}
