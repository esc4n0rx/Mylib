import { Controller, useForm } from 'react-hook-form';
import { zodResolver } from '@hookform/resolvers/zod';
import {
  Box,
  Card,
  CardActionArea,
  MenuItem,
  Stack,
  TextField,
  ToggleButton,
  ToggleButtonGroup,
  Typography,
} from '@mui/material';
import MovieIcon from '@mui/icons-material/Movie';
import LiveTvIcon from '@mui/icons-material/LiveTv';
import { useTranslation } from 'react-i18next';
import {
  defaultLibraryValues,
  LANGUAGE_OPTIONS,
  libraryFormSchema,
  REGION_OPTIONS,
  toCreateRequest,
  type LibraryFormValues,
} from '../schema';
import { PathManager } from './PathManager';
import { PasswordField } from '@/components/PasswordField';
import type { CreateLibraryRequest } from '@/api';

interface LibraryFormProps {
  formId: string;
  initialValues?: Partial<LibraryFormValues>;
  onSubmit: (request: CreateLibraryRequest, values: LibraryFormValues) => void;
}

export function LibraryForm({ formId, initialValues, onSubmit }: LibraryFormProps) {
  const { t } = useTranslation('libraries');
  const {
    control,
    handleSubmit,
    watch,
    setValue,
    formState: { errors },
  } = useForm<LibraryFormValues>({
    resolver: zodResolver(libraryFormSchema),
    defaultValues: { ...defaultLibraryValues, ...initialValues },
    mode: 'onBlur',
  });

  const type = watch('type');
  const privacy = watch('privacy');
  const paths = watch('paths');

  return (
    <Box
      component="form"
      id={formId}
      noValidate
      onSubmit={handleSubmit((values) => onSubmit(toCreateRequest(values), values))}
    >
      <Stack spacing={3}>
        <Controller
          control={control}
          name="name"
          render={({ field }) => (
            <TextField
              {...field}
              label={t('form.name')}
              required
              error={Boolean(errors.name)}
              helperText={errors.name ? t('form.name') : undefined}
            />
          )}
        />

        <Controller
          control={control}
          name="description"
          render={({ field }) => (
            <TextField {...field} label={t('form.description')} multiline minRows={2} />
          )}
        />

        <Box>
          <Typography variant="h3" sx={{ mb: 1 }}>
            {t('form.type')}
          </Typography>
          <Stack direction={{ xs: 'column', sm: 'row' }} spacing={2}>
            {(
              [
                { value: 'MOVIE', label: t('type.movie'), icon: <MovieIcon /> },
                { value: 'TV_SHOW', label: t('type.tvShow'), icon: <LiveTvIcon /> },
              ] as const
            ).map((opt) => (
              <Card
                key={opt.value}
                variant="outlined"
                sx={{
                  flex: 1,
                  borderColor: (th) =>
                    type === opt.value ? th.tokens.primary : th.tokens.outlineVariant,
                  borderWidth: type === opt.value ? 2 : 1,
                }}
              >
                <CardActionArea
                  onClick={() => setValue('type', opt.value, { shouldValidate: true })}
                  sx={{ p: 2, display: 'flex', gap: 1.5, justifyContent: 'flex-start' }}
                >
                  {opt.icon}
                  <Typography variant="h3">{opt.label}</Typography>
                </CardActionArea>
              </Card>
            ))}
          </Stack>
        </Box>

        <Box>
          <Typography variant="h3" sx={{ mb: 1 }}>
            {t('privacy.label')}
          </Typography>
          <Controller
            control={control}
            name="privacy"
            render={({ field }) => (
              <ToggleButtonGroup
                exclusive
                value={field.value}
                onChange={(_, value) => value && field.onChange(value)}
                size="small"
              >
                <ToggleButton value="PUBLIC">{t('privacy.public')}</ToggleButton>
                <ToggleButton value="PRIVATE">{t('privacy.private')}</ToggleButton>
              </ToggleButtonGroup>
            )}
          />
          <Typography variant="body2" color="text.secondary" sx={{ mt: 0.5 }}>
            {privacy === 'PRIVATE' ? t('privacy.privateDesc') : t('privacy.publicDesc')}
          </Typography>
        </Box>

        {privacy === 'PRIVATE' ? (
          <Stack direction={{ xs: 'column', sm: 'row' }} spacing={2}>
            <Controller
              control={control}
              name="password"
              render={({ field }) => (
                <PasswordField
                  {...field}
                  label={t('form.libraryPassword')}
                  error={Boolean(errors.password)}
                />
              )}
            />
            <Controller
              control={control}
              name="confirmPassword"
              render={({ field }) => (
                <PasswordField
                  {...field}
                  label={t('form.confirmPassword')}
                  error={Boolean(errors.confirmPassword)}
                />
              )}
            />
          </Stack>
        ) : null}

        <Stack direction={{ xs: 'column', sm: 'row' }} spacing={2}>
          <Controller
            control={control}
            name="metadataLanguage"
            render={({ field }) => (
              <TextField {...field} select label={t('form.metadataLanguage')}>
                {LANGUAGE_OPTIONS.map((opt) => (
                  <MenuItem key={opt.value} value={opt.value}>
                    {opt.label}
                  </MenuItem>
                ))}
              </TextField>
            )}
          />
          <Controller
            control={control}
            name="metadataRegion"
            render={({ field }) => (
              <TextField {...field} select label={t('form.metadataRegion')}>
                {REGION_OPTIONS.map((opt) => (
                  <MenuItem key={opt.value} value={opt.value}>
                    {opt.label}
                  </MenuItem>
                ))}
              </TextField>
            )}
          />
          <Controller
            control={control}
            name="minimumAge"
            render={({ field }) => (
              <TextField
                {...field}
                type="number"
                label={t('form.minimumAge')}
                inputProps={{ min: 0, max: 21 }}
                error={Boolean(errors.minimumAge)}
              />
            )}
          />
        </Stack>

        <PathManager
          paths={paths}
          onChange={(next) => setValue('paths', next, { shouldValidate: true })}
          error={errors.paths ? t('form.paths') : undefined}
        />
      </Stack>
    </Box>
  );
}
