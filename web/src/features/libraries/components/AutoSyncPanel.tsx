import { useEffect, useState } from 'react';
import {
  Alert,
  Box,
  Button,
  FormControlLabel,
  MenuItem,
  Stack,
  Switch,
  TextField,
  Typography,
} from '@mui/material';
import { useTranslation } from 'react-i18next';
import type { AutoSyncConfig, Library } from '@/api';
import { formatDateTime } from '@/utils/format';
import { useToast } from '@/app/ToastProvider';
import { SectionHeader } from '@/components/PageHeader';
import { useUpdateAutoSync } from '../hooks';

export function AutoSyncPanel({ library }: { library: Library }) {
  const { t } = useTranslation('libraries');
  const { notify } = useToast();
  const mutation = useUpdateAutoSync(library.id);
  const [config, setConfig] = useState<AutoSyncConfig>(library.autoSync);

  useEffect(() => setConfig(library.autoSync), [library.autoSync]);
  const change = (next: Partial<AutoSyncConfig>) => setConfig((current) => ({ ...current, ...next }));

  return (
    <Box sx={{ maxWidth: 680 }}>
      <SectionHeader title={t('sync.title')} />
      {library.lastError ? <Alert severity="error" sx={{ mb: 2 }}>{library.lastError}</Alert> : null}
      <Stack spacing={2.25}>
        <FormControlLabel
          control={<Switch checked={config.enabled} onChange={(event) => change({ enabled: event.target.checked })} />}
          label={t('sync.enabled')}
        />
        <TextField select label={t('sync.mode')} value={config.mode} disabled={!config.enabled}
          onChange={(event) => change({ mode: event.target.value as AutoSyncConfig['mode'] })}>
          <MenuItem value="INTERVAL">{t('sync.interval')}</MenuItem>
          <MenuItem value="SCHEDULE">{t('sync.schedule')}</MenuItem>
        </TextField>
        {config.mode === 'INTERVAL' ? (
          <TextField type="number" label={t('sync.intervalMinutes')} value={config.intervalMinutes}
            disabled={!config.enabled} inputProps={{ min: 5, max: 10080 }}
            onChange={(event) => change({ intervalMinutes: Number(event.target.value) })} />
        ) : (
          <Stack direction={{ xs: 'column', sm: 'row' }} spacing={2}>
            <TextField type="number" label={t('sync.hour')} value={config.schedule.hour} disabled={!config.enabled}
              inputProps={{ min: 0, max: 23 }} onChange={(event) => change({ schedule: { ...config.schedule, hour: Number(event.target.value) } })} />
            <TextField type="number" label={t('sync.minute')} value={config.schedule.minute} disabled={!config.enabled}
              inputProps={{ min: 0, max: 59 }} onChange={(event) => change({ schedule: { ...config.schedule, minute: Number(event.target.value) } })} />
          </Stack>
        )}
        <FormControlLabel control={<Switch checked={config.scanOnStartup} onChange={(event) => change({ scanOnStartup: event.target.checked })} />}
          label={t('sync.onStartup')} />
        <Stack direction="row" justifyContent="space-between" alignItems="center">
          <Box>
            <Typography variant="caption" color="text.secondary">{t('sync.next')}</Typography>
            <Typography variant="body2">{library.nextSyncAt ? formatDateTime(library.nextSyncAt) : t('card.never')}</Typography>
          </Box>
          <Button variant="contained" disabled={mutation.isPending} onClick={async () => {
            await mutation.mutateAsync(config);
            notify(t('toast.syncSaved'), 'success');
          }}>{t('sync.save')}</Button>
        </Stack>
      </Stack>
    </Box>
  );
}
