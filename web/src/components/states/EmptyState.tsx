import { Box, Button, Stack, Typography } from '@mui/material';
import type { ReactNode } from 'react';

interface EmptyStateProps {
  icon?: ReactNode;
  title: string;
  body?: string;
  action?: { label: string; onClick: () => void };
}

export function EmptyState({ icon, title, body, action }: EmptyStateProps) {
  return (
    <Stack
      alignItems="center"
      justifyContent="center"
      spacing={1}
      sx={{
        textAlign: 'center',
        py: 8,
        px: 3,
        border: (t) => `1px dashed ${t.tokens.outlineVariant}`,
        borderRadius: (t) => `${t.ds.radius.card}px`,
      }}
    >
      {icon ? <Box sx={{ color: 'text.secondary', mb: 1 }}>{icon}</Box> : null}
      <Typography variant="h2">{title}</Typography>
      {body ? (
        <Typography variant="body1" color="text.secondary" sx={{ maxWidth: 420 }}>
          {body}
        </Typography>
      ) : null}
      {action ? (
        <Button variant="contained" onClick={action.onClick} sx={{ mt: 2 }}>
          {action.label}
        </Button>
      ) : null}
    </Stack>
  );
}
