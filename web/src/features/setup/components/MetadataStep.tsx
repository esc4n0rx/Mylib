import { Alert, Link, Stack, TextField, Typography } from '@mui/material';
import { useTranslation } from 'react-i18next';

interface MetadataStepProps {
  formId: string;
  value: string;
  onChange: (value: string) => void;
  onNext: () => void;
}

// TMDB is optional at setup time: the server runs fine without it, only metadata enrichment
// (posters, overviews, cast) stays disabled until a key is provided here or later from settings.
export function MetadataStep({ formId, value, onChange, onNext }: MetadataStepProps) {
  const { t } = useTranslation('setup');
  return (
    <Stack spacing={3}>
      <div>
        <Typography variant="h1">{t('metadata.title')}</Typography>
        <Typography variant="body1" color="text.secondary" sx={{ mt: 0.5 }}>
          {t('metadata.subtitle')}
        </Typography>
      </div>
      <form
        id={formId}
        noValidate
        onSubmit={(event) => {
          event.preventDefault();
          onNext();
        }}
      >
        <Stack spacing={2}>
          <TextField
            label={t('metadata.apiKey')}
            placeholder={t('metadata.apiKeyPlaceholder')}
            value={value}
            onChange={(event) => onChange(event.target.value)}
            fullWidth
          />
          <Alert severity="info">
            {t('metadata.hint')}{' '}
            <Link href="https://www.themoviedb.org/settings/api" target="_blank" rel="noreferrer">
              themoviedb.org/settings/api
            </Link>
          </Alert>
        </Stack>
      </form>
    </Stack>
  );
}
