import { Box, Chip, Stack, Typography } from '@mui/material';

type Tone = 'neutral' | 'success' | 'warning' | 'error' | 'info';

type ThemeArg = import('@mui/material').Theme;
const toneColor: Record<Tone, (t: ThemeArg) => string> = {
  neutral: (t) => t.tokens.onSurfaceVariant,
  success: (t) => t.tokens.primary,
  warning: () => '#B26A00',
  error: (t) => t.tokens.error,
  info: (t) => t.tokens.onSurfaceVariant,
};

export function StatusDot({ tone = 'neutral' }: { tone?: Tone }) {
  return (
    <Box
      component="span"
      sx={{
        width: 8,
        height: 8,
        borderRadius: '50%',
        flexShrink: 0,
        backgroundColor: (t) => toneColor[tone](t),
      }}
    />
  );
}

export function StatusBadge({
  label,
  tone = 'neutral',
}: {
  label: string;
  tone?: Tone;
}) {
  return (
    <Chip
      size="small"
      variant="outlined"
      label={
        <Stack direction="row" alignItems="center" spacing={0.75}>
          <StatusDot tone={tone} />
          <Typography variant="body2" component="span">
            {label}
          </Typography>
        </Stack>
      }
    />
  );
}
