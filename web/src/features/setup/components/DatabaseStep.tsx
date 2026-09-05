import { useState } from 'react';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import {
  Alert,
  Box,
  Button,
  Card,
  CardActionArea,
  Chip,
  Stack,
  TextField,
  Typography,
} from '@mui/material';
import { useMutation } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { api, ApiError, type DatabaseConfig } from '@/api';
import { mysqlSchema, type MysqlValues } from '../schema';

interface DatabaseStepProps {
  formId: string;
  value: DatabaseConfig;
  onNext: (value: DatabaseConfig) => void;
}

type TestState = 'idle' | 'testing' | 'connected' | 'failed';

const MYSQL_DEFAULTS: MysqlValues = {
  host: 'localhost',
  port: 3306,
  database: 'mylib',
  username: 'mylib',
  password: '',
  sslMode: 'preferred',
};

export function DatabaseStep({ formId, value, onNext }: DatabaseStepProps) {
  const { t } = useTranslation('setup');
  const [kind, setKind] = useState<'sqlite' | 'mysql'>(value.type);
  const [testState, setTestState] = useState<TestState>('idle');

  const { register, handleSubmit, formState: { errors } } = useForm<MysqlValues>({
    resolver: zodResolver(mysqlSchema),
    defaultValues: value.type === 'mysql' ? value : MYSQL_DEFAULTS,
  });

  const testConnection = useMutation({
    mutationFn: (config: DatabaseConfig) => api.setup.testDatabase(config),
    onMutate: () => setTestState('testing'),
    onSuccess: (res) => setTestState(res.success ? 'connected' : 'failed'),
    onError: () => setTestState('failed'),
  });

  const handleContinue = (e: React.FormEvent) => {
    e.preventDefault();
    if (kind === 'sqlite') {
      onNext({ type: 'sqlite' });
      return;
    }
    void handleSubmit((values) => onNext({ type: 'mysql', ...values }))(e);
  };

  return (
    <Stack spacing={3}>
      <div>
        <Typography variant="h1">{t('database.title')}</Typography>
        <Typography variant="body1" color="text.secondary" sx={{ mt: 0.5 }}>
          {t('database.subtitle')}
        </Typography>
      </div>

      <Stack direction={{ xs: 'column', sm: 'row' }} spacing={2}>
        {(['sqlite', 'mysql'] as const).map((option) => (
          <Card
            key={option}
            variant="outlined"
            sx={{
              flex: 1,
              borderColor: (th) =>
                kind === option ? th.tokens.primary : th.tokens.outlineVariant,
              borderWidth: kind === option ? 2 : 1,
            }}
          >
            <CardActionArea sx={{ p: 2 }} onClick={() => setKind(option)}>
              <Stack direction="row" spacing={1} alignItems="center">
                <Typography variant="h3">{t(`database.${option}`)}</Typography>
                {option === 'sqlite' ? (
                  <Chip size="small" color="primary" label={t('database.sqliteRecommended')} />
                ) : null}
              </Stack>
              <Typography variant="body2" color="text.secondary" sx={{ mt: 0.5 }}>
                {t(`database.${option}Desc`)}
              </Typography>
            </CardActionArea>
          </Card>
        ))}
      </Stack>

      <Box component="form" id={formId} onSubmit={handleContinue}>
        {kind === 'mysql' ? (
          <Stack spacing={2}>
            <Stack direction={{ xs: 'column', sm: 'row' }} spacing={2}>
              <TextField label={t('database.host')} error={Boolean(errors.host)} {...register('host')} />
              <TextField
                label={t('database.port')}
                type="number"
                sx={{ maxWidth: { sm: 120 } }}
                error={Boolean(errors.port)}
                {...register('port')}
              />
            </Stack>
            <TextField
              label={t('database.dbName')}
              error={Boolean(errors.database)}
              {...register('database')}
            />
            <Stack direction={{ xs: 'column', sm: 'row' }} spacing={2}>
              <TextField
                label={t('database.username')}
                error={Boolean(errors.username)}
                {...register('username')}
              />
              <TextField
                label={t('database.password')}
                type="password"
                error={Boolean(errors.password)}
                {...register('password')}
              />
            </Stack>
            <TextField
              select
              label={t('database.sslMode')}
              defaultValue={MYSQL_DEFAULTS.sslMode}
              SelectProps={{ native: true }}
              {...register('sslMode')}
            >
              {['disabled', 'preferred', 'required'].map((mode) => (
                <option key={mode} value={mode}>
                  {mode}
                </option>
              ))}
            </TextField>

            <Stack direction="row" spacing={2} alignItems="center">
              <Button
                variant="outlined"
                type="button"
                onClick={handleSubmit((values) =>
                  testConnection.mutate({ type: 'mysql', ...values }),
                )}
                disabled={testState === 'testing'}
              >
                {testState === 'testing' ? t('database.testing') : t('database.testConnection')}
              </Button>
              {testState === 'connected' ? (
                <Alert severity="success" sx={{ py: 0 }}>
                  {t('database.connected')}
                </Alert>
              ) : null}
              {testState === 'failed' ? (
                <Alert severity="error" sx={{ py: 0 }}>
                  {testConnection.error instanceof ApiError
                    ? testConnection.error.localizedMessage
                    : t('database.failed')}
                </Alert>
              ) : null}
            </Stack>
          </Stack>
        ) : null}
      </Box>
    </Stack>
  );
}
