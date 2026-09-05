import { Alert, Divider, Stack, Typography } from '@mui/material';
import { useTranslation } from 'react-i18next';
import type { CreateLibraryRequest, DatabaseConfig } from '@/api';

interface FinishStepProps {
  serverName: string;
  database: DatabaseConfig;
  adminUsername: string;
  libraries: CreateLibraryRequest[];
  error?: string | null;
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <Stack direction="row" justifyContent="space-between" sx={{ py: 0.75 }}>
      <Typography variant="body1" color="text.secondary">
        {label}
      </Typography>
      <Typography variant="body1">{value}</Typography>
    </Stack>
  );
}

export function FinishStep({
  serverName,
  database,
  adminUsername,
  libraries,
  error,
}: FinishStepProps) {
  const { t } = useTranslation('setup');
  return (
    <Stack spacing={3}>
      <div>
        <Typography variant="h1">{t('finish.title')}</Typography>
        <Typography variant="body1" color="text.secondary" sx={{ mt: 0.5 }}>
          {t('finish.subtitle')}
        </Typography>
      </div>

      {error ? <Alert severity="error">{error}</Alert> : null}

      <div>
        <Row label={t('finish.server')} value={serverName} />
        <Divider />
        <Row
          label={t('finish.database')}
          value={database.type === 'mysql' ? 'MySQL' : 'SQLite'}
        />
        <Divider />
        <Row label={t('finish.administrator')} value={adminUsername} />
        <Divider />
        <Row
          label={t('finish.libraries')}
          value={
            libraries.length
              ? libraries.map((l) => l.name).join(', ')
              : t('finish.noLibraries')
          }
        />
      </div>
    </Stack>
  );
}
