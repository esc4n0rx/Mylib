import { Button, Stack, Typography } from '@mui/material';
import ReportGmailerrorredIcon from '@mui/icons-material/ReportGmailerrorred';
import { useTranslation } from 'react-i18next';

interface ErrorStateProps {
  title?: string;
  body?: string;
  onRetry?: () => void;
  fullHeight?: boolean;
}

export function ErrorState({ title, body, onRetry, fullHeight }: ErrorStateProps) {
  const { t } = useTranslation('common');
  return (
    <Stack
      alignItems="center"
      justifyContent="center"
      spacing={1.5}
      sx={{ textAlign: 'center', py: 8, px: 3, minHeight: fullHeight ? '60vh' : undefined }}
    >
      <ReportGmailerrorredIcon sx={{ fontSize: 40, color: 'error.main' }} />
      <Typography variant="h2">{title ?? t('states.errorTitle')}</Typography>
      <Typography variant="body1" color="text.secondary" sx={{ maxWidth: 420 }}>
        {body ?? t('states.errorBody')}
      </Typography>
      {onRetry ? (
        <Button variant="outlined" onClick={onRetry} sx={{ mt: 1 }}>
          {t('actions.retry')}
        </Button>
      ) : null}
    </Stack>
  );
}
