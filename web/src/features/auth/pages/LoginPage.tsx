import { useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import { z } from 'zod';
import { Alert, Box, Button, Checkbox, FormControlLabel, Stack, TextField, Typography } from '@mui/material';
import { useMutation } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { api, ApiError } from '@/api';
import { useAuth } from '@/app/AuthProvider';
import { AuthShell } from '@/layouts/AuthShell';
import { PasswordField } from '@/components/PasswordField';

const schema = z.object({
  username: z.string().trim().min(1),
  password: z.string().min(1),
  rememberMe: z.boolean(),
});
type Values = z.infer<typeof schema>;

export default function LoginPage() {
  const { t } = useTranslation('auth');
  const navigate = useNavigate();
  const { login } = useAuth();
  const {
    register,
    handleSubmit,
    formState: { errors },
  } = useForm<Values>({
    resolver: zodResolver(schema),
    defaultValues: { username: '', password: '', rememberMe: true },
  });

  const loginMutation = useMutation({
    mutationFn: (values: Values) => api.auth.login(values.username, values.password),
    onSuccess: (res) => {
      login(res.accessToken);
      navigate(res.profileSelectionRequired ? '/profiles' : '/home', { replace: true });
    },
  });

  const errorMessage = (() => {
    const err = loginMutation.error;
    if (!err) return null;
    if (err instanceof ApiError) {
      if (err.kind === 'network') return t('common:states.serverOfflineTitle', { ns: 'common' });
      if (err.status === 429) return t('rateLimited');
      // Never reveal user enumeration — always the same message for 401.
      return t('invalidCredentials');
    }
    return t('invalidCredentials');
  })();

  return (
    <AuthShell>
      <Stack spacing={4}>
        <Box>
          <Typography variant="h2">{t('welcomeBack')}</Typography>
          <Typography variant="body1" color="text.secondary" sx={{ mt: 0.5 }}>
            {t('welcomeSubtitle')}
          </Typography>
        </Box>

        {errorMessage ? <Alert severity="error">{errorMessage}</Alert> : null}

        <Box
          component="form"
          noValidate
          onSubmit={handleSubmit((values) => loginMutation.mutate(values))}
        >
          <Stack spacing={2}>
            <TextField
              label={t('usernameOrEmail')}
              autoComplete="username"
              autoFocus
              error={Boolean(errors.username)}
              {...register('username')}
            />
            <PasswordField
              label={t('password')}
              autoComplete="current-password"
              error={Boolean(errors.password)}
              {...register('password')}
            />
            <FormControlLabel
              control={<Checkbox defaultChecked {...register('rememberMe')} />}
              label={<Typography variant="body1">{t('rememberMe')}</Typography>}
            />
            <Button type="submit" variant="contained" disabled={loginMutation.isPending}>
              {loginMutation.isPending ? t('signingIn') : t('signIn')}
            </Button>
          </Stack>
        </Box>
      </Stack>
    </AuthShell>
  );
}
