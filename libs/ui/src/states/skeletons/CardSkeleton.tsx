import { Card, CardContent, Skeleton, Stack } from '@mui/material';

interface CardSkeletonProps {
  lines?: number;
  showHeader?: boolean;
}

export function CardSkeleton({
  lines = 3,
  showHeader = true,
}: CardSkeletonProps) {
  return (
    <Card>
      <CardContent>
        <Stack spacing={1.5}>
          {showHeader && <Skeleton variant="text" width="40%" height={28} />}
          {Array.from({ length: lines }).map((_, i) => (
            <Stack key={i} direction="row" spacing={2} alignItems="center">
              <Skeleton variant="text" width="30%" />
              <Skeleton variant="text" width="55%" />
            </Stack>
          ))}
        </Stack>
      </CardContent>
    </Card>
  );
}
