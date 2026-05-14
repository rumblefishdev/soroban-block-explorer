import { Skeleton, Stack } from '@mui/material';

import { CardSkeleton } from './CardSkeleton.js';

interface DetailSkeletonProps {
  sections?: number;
}

export function DetailSkeleton({ sections = 3 }: DetailSkeletonProps) {
  return (
    <Stack spacing={3}>
      <Stack spacing={1}>
        <Skeleton variant="text" width="30%" height={40} />
        <Skeleton variant="text" width="60%" height={20} />
      </Stack>
      {Array.from({ length: sections }).map((_, i) => (
        <CardSkeleton key={i} lines={4} />
      ))}
    </Stack>
  );
}
