import { useEffect } from 'react';
import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { Stack, TextField, Typography } from '@mui/material';
import { useTranslation } from 'react-i18next';
import { serverStepSchema, SERVER_LANGUAGE_OPTIONS, type ServerStepValues } from '../schema';

interface ServerStepProps {
  formId: string;
  values: ServerStepValues;
  onNext: (values: ServerStepValues) => void;
  onChange: (values: ServerStepValues) => void;
}

export function ServerStep({ formId, values, onNext, onChange }: ServerStepProps) {
  const { t } = useTranslation('setup');
  const { register, handleSubmit, watch, formState: { errors } } = useForm<ServerStepValues>({
    resolver: zodResolver(serverStepSchema),
    defaultValues: values,
  });

  useEffect(() => {
    const sub = watch((v) => onChange(v as ServerStepValues));
    return () => sub.unsubscribe();
  }, [watch, onChange]);

  return (
    <Stack spacing={3}>
      <div>
        <Typography variant="h1">{t('server.title')}</Typography>
        <Typography variant="body1" color="text.secondary" sx={{ mt: 0.5 }}>
          {t('server.subtitle')}
        </Typography>
      </div>
      <form id={formId} noValidate onSubmit={handleSubmit(onNext)}>
        <Stack spacing={3}>
          <TextField
            label={t('server.name')}
            placeholder={t('server.namePlaceholder')}
            required
            error={Boolean(errors.serverName)}
            {...register('serverName')}
          />
          <TextField
            select
            label={t('server.language')}
            defaultValue={values.serverLanguage}
            SelectProps={{ native: true }}
            {...register('serverLanguage')}
          >
            {SERVER_LANGUAGE_OPTIONS.map((opt) => (
              <option key={opt.value} value={opt.value}>
                {opt.label}
              </option>
            ))}
          </TextField>
        </Stack>
      </form>
    </Stack>
  );
}
