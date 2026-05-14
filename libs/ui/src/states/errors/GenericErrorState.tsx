import ReportProblemOutlinedIcon from '@mui/icons-material/ReportProblemOutlined';
import { Button } from '@mui/material';

import { EmptyState } from '../empty/EmptyState.js';

interface GenericErrorStateProps {
  onRetry?: () => void;
  description?: string;
  meta?: string;
}

export function GenericErrorState({
  onRetry,
  description,
  meta,
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
