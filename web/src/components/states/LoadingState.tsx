import { Skeleton, Stack } from '@mui/material';

interface LoadingStateProps {
  rows?: number;
  /** Height of each skeleton block, px. */
  height?: number;
}

/** Content-shaped skeleton. Preferred over a centred spinner for full pages. */
export function LoadingState({ rows = 3, height = 96 }: LoadingStateProps) {
  return (
    <Stack spacing={2} aria-busy="true" aria-live="polite">
      {Array.from({ length: rows }).map((_, i) => (
        <Skeleton key={i} variant="rounded" height={height} />
      ))}
    </Stack>
  );
}
