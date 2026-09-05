import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { Box, LinearProgress, Stack, TextField, Typography } from '@mui/material';
import { useTranslation } from 'react-i18next';
import { PasswordField } from '@/components/PasswordField';
import { passwordStrength, STRENGTH_KEYS } from '@/utils/password';
import { adminStepSchema, type AdminStepValues } from '../schema';

interface AdminStepProps {
  formId: string;
  values: AdminStepValues | null;
  onNext: (values: AdminStepValues) => void;
}

const EMPTY: AdminStepValues = {
  username: '',
  displayName: '',
  email: '',
  password: '',
  confirmPassword: '',
};

export function AdminStep({ formId, values, onNext }: AdminStepProps) {
  const { t } = useTranslation('setup');
  const { register, handleSubmit, watch, formState: { errors } } = useForm<AdminStepValues>({
    resolver: zodResolver(adminStepSchema),
    defaultValues: values ?? EMPTY,
  });

  const password = watch('password');
  const strength = passwordStrength(password ?? '');

  return (
    <Stack spacing={3}>
      <div>
        <Typography variant="h1">{t('administrator.title')}</Typography>
        <Typography variant="body1" color="text.secondary" sx={{ mt: 0.5 }}>
          {t('administrator.subtitle')}
        </Typography>
      </div>
      <form id={formId} noValidate onSubmit={handleSubmit(onNext)}>
        <Stack spacing={2.5}>
          <Stack direction={{ xs: 'column', sm: 'row' }} spacing={2}>
            <TextField
              label={t('administrator.username')}
              required
              error={Boolean(errors.username)}
              helperText={errors.username ? 'a-z 0-9 . _ -' : undefined}
              {...register('username')}
            />
            <TextField
              label={t('administrator.displayName')}
              required
              error={Boolean(errors.displayName)}
              {...register('displayName')}
            />
          </Stack>
          <TextField
            label={t('administrator.email')}
            type="email"
            error={Boolean(errors.email)}
            {...register('email')}
          />
          <Box>
            <PasswordField
              label={t('administrator.password')}
              required
              error={Boolean(errors.password)}
              helperText={errors.password ? t('administrator.passwordMinLength') : undefined}
              {...register('password')}
            />
            {password ? (
              <Box sx={{ mt: 1 }}>
                <LinearProgress
                  variant="determinate"
                  value={(strength / 4) * 100}
                  color={strength >= 3 ? 'primary' : 'error'}
                />
                <Typography variant="caption" color="text.secondary">
                  {t(`administrator.strength.${STRENGTH_KEYS[strength]}`)}
                </Typography>
              </Box>
            ) : null}
          </Box>
          <PasswordField
            label={t('administrator.confirmPassword')}
            required
            error={Boolean(errors.confirmPassword)}
            {...register('confirmPassword')}
          />
        </Stack>
      </form>
    </Stack>
  );
}
