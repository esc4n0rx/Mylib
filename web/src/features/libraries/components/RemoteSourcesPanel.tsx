import { useState } from 'react';
import {
  Alert,
  Box,
  Button,
  Card,
  CardActionArea,
  CardContent,
  Chip,
  CircularProgress,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  Divider,
  FormControlLabel,
  Stack,
  Switch,
  TextField,
  Typography,
} from '@mui/material';
import PlaylistPlayIcon from '@mui/icons-material/PlaylistPlay';
import CloudIcon from '@mui/icons-material/Cloud';
import FolderIcon from '@mui/icons-material/Folder';
import { useTranslation } from 'react-i18next';
import type { Library, M3uPreview, RemoteProviderType, RemoteSource } from '@/api';
import { api, ApiError } from '@/api';
import { SectionHeader } from '@/components/PageHeader';
import { QueryBoundary } from '@/components/QueryBoundary';
import { EmptyState } from '@/components/states/EmptyState';
import { useToast } from '@/app/ToastProvider';
import { formatDateTime } from '@/utils/format';
import {
  startRemoteSync,
  useCreateRemoteSource,
  useRemoteSourceEntries,
  useRemoteSourceMutations,
  useRemoteSources,
} from '../remoteSourceHooks';
import {
  M3uSelectionTree,
  emptySelection,
  selectionToRules,
  type SelectionState,
} from './M3uSelectionTree';
import { GoogleDrivePicker, type DriveSelection } from './GoogleDrivePicker';

type Wizard =
  | { step: 'pick' }
  | { step: 'configure'; provider: RemoteProviderType }
  | { step: 'select'; provider: RemoteProviderType; preview: M3uPreview; config: Record<string, unknown>; name: string };

export function RemoteSourcesPanel({ library }: { library: Library }) {
  const { t } = useTranslation('remoteSources');
  const query = useRemoteSources(library.id);
  const [wizard, setWizard] = useState<Wizard | null>(null);

  return (
    <Box sx={{ maxWidth: 780 }}>
      <SectionHeader
        title={t('title')}
        action={
          <Button variant="contained" onClick={() => setWizard({ step: 'pick' })}>
            {t('addSource')}
          </Button>
        }
      />
      <Typography variant="body2" color="text.secondary" sx={{ mb: 2 }}>
        {t('description')}
      </Typography>
      <QueryBoundary query={query}>
        {(data) =>
          data.items.length === 0 ? (
            <EmptyState title={t('empty.title')} body={t('empty.body')} />
          ) : (
            <Stack spacing={1.5}>
              {data.items.map((source) => (
                <SourceCard key={source.id} libraryId={library.id} source={source} />
              ))}
            </Stack>
          )
        }
      </QueryBoundary>
      {wizard ? (
        <SourceWizard
          library={library}
          wizard={wizard}
          setWizard={setWizard}
          onClose={() => setWizard(null)}
        />
      ) : null}
    </Box>
  );
}

function SourceCard({ libraryId, source }: { libraryId: string; source: RemoteSource }) {
  const { t } = useTranslation('remoteSources');
  const { notify } = useToast();
  const { update, remove, sync } = useRemoteSourceMutations(libraryId, source.id);
  const entries = useRemoteSourceEntries(source.id, { pageSize: 1 });
  const isSyncing = source.status === 'SYNCING' || sync.isPending;

  const runSync = async () => {
    try {
      await sync.mutateAsync();
      notify(t('sync.started'), 'info');
    } catch (error) {
      notify(error instanceof ApiError ? error.localizedMessage : t('status.ERROR'), 'error');
    }
  };

  return (
    <Card variant="outlined">
      <CardContent>
        <Stack direction="row" justifyContent="space-between" alignItems="flex-start">
          <Box>
            <Typography variant="h3">{source.name}</Typography>
            <Typography variant="caption" color="text.secondary">
              {t(`provider.${source.providerType}`)}
            </Typography>
          </Box>
          <Chip
            size="small"
            variant="outlined"
            icon={isSyncing ? <CircularProgress size={12} color="inherit" /> : undefined}
            color={source.status === 'READY' ? 'success' : source.status === 'SYNCING' ? 'info' : 'warning'}
            label={t(`status.${isSyncing ? 'SYNCING' : source.status}`)}
          />
        </Stack>
        {source.lastError && !isSyncing ? (
          <Alert severity="warning" sx={{ mt: 1 }}>
            {source.lastError}
          </Alert>
        ) : null}
        <Stack direction="row" spacing={3} sx={{ mt: 1.5 }} flexWrap="wrap" useFlexGap>
          <Field label={t('card.entriesLabel')} value={String(entries.data?.total ?? 0)} />
          <Field label={t('card.lastSync')} value={source.lastSyncAt ? formatDateTime(source.lastSyncAt) : t('card.never')} />
          <Field label={t('card.nextSync')} value={source.nextSyncAt ? formatDateTime(source.nextSyncAt) : t('card.never')} />
        </Stack>
        <Divider sx={{ my: 1.5 }} />
        <Stack direction="row" spacing={1} flexWrap="wrap" useFlexGap>
          <Button size="small" variant="contained" onClick={runSync} disabled={isSyncing || !source.isActive}>
            {isSyncing ? t('card.syncing') : t('card.sync')}
          </Button>
          <Button
            size="small"
            onClick={() => update.mutate({ isActive: !source.isActive })}
            disabled={update.isPending}
          >
            {source.isActive ? t('card.disable') : t('card.enable')}
          </Button>
          <Button
            size="small"
            color="error"
            onClick={() => {
              if (window.confirm(t('card.removeConfirm'))) remove.mutate();
            }}
            disabled={remove.isPending}
          >
            {t('card.remove')}
          </Button>
        </Stack>
      </CardContent>
    </Card>
  );
}

function Field({ label, value }: { label: string; value: string }) {
  return (
    <Box>
      <Typography variant="caption" color="text.secondary">
        {label}
      </Typography>
      <Typography variant="body2">{value}</Typography>
    </Box>
  );
}

function SourceWizard({
  library,
  wizard,
  setWizard,
  onClose,
}: {
  library: Library;
  wizard: Wizard;
  setWizard: (next: Wizard) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation('remoteSources');
  const { notify } = useToast();
  const create = useCreateRemoteSource(library.id);

  const [name, setName] = useState('');
  const [url, setUrl] = useState('');
  const [file, setFile] = useState<File | null>(null);
  const [autoSync, setAutoSync] = useState(true);
  const [interval, setIntervalMinutes] = useState(720);
  const [busy, setBusy] = useState(false);
  const [drive, setDrive] = useState<DriveSelection | null>(null);
  const [selection, setSelection] = useState<SelectionState>(emptySelection());
  const [error, setError] = useState<string | null>(null);

  const finish = async (config: Record<string, unknown>, provider: RemoteProviderType) => {
    const source = await create.mutateAsync({
      name: name.trim(),
      providerType: provider,
      config,
      autoSync: { enabled: autoSync, intervalMinutes: interval },
    });
    return source;
  };

  const analyze = async (provider: RemoteProviderType) => {
    setBusy(true);
    setError(null);
    try {
      if (provider === 'M3U_URL') {
        const preview = await api.remoteSources.previewM3u({ type: 'url', url: url.trim() });
        setWizard({ step: 'select', provider, preview, config: { url: url.trim() }, name });
      } else if (provider === 'M3U_FILE') {
        if (!file) return;
        const { uploadId } = await api.remoteSources.uploadM3u(file);
        const preview = await api.remoteSources.previewM3u({ type: 'upload', uploadId });
        setWizard({ step: 'select', provider, preview, config: { uploadId }, name });
      }
    } catch (caught) {
      setError(caught instanceof ApiError ? caught.localizedMessage : t('status.ERROR'));
    } finally {
      setBusy(false);
    }
  };

  const createDrive = async () => {
    if (!drive || drive.folders.length === 0) return;
    setBusy(true);
    setError(null);
    try {
      const source = await finish(
        { connectionId: drive.connectionId, folders: drive.folders },
        'GOOGLE_DRIVE',
      );
      await startRemoteSync(source.id);
      notify(t('sync.started'), 'info');
      onClose();
    } catch (caught) {
      setError(caught instanceof ApiError ? caught.localizedMessage : t('status.ERROR'));
    } finally {
      setBusy(false);
    }
  };

  const confirmSelection = async () => {
    if (wizard.step !== 'select') return;
    setBusy(true);
    setError(null);
    try {
      const source = await finish(wizard.config, wizard.provider);
      const rules = selectionToRules(selection);
      if (rules.length > 0) await api.remoteSources.setSelections(source.id, rules);
      await startRemoteSync(source.id);
      notify(t('sync.started'), 'info');
      onClose();
    } catch (caught) {
      setError(caught instanceof ApiError ? caught.localizedMessage : t('status.ERROR'));
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog open onClose={onClose} maxWidth="sm" fullWidth>
      <DialogTitle>{t('addSource')}</DialogTitle>
      <DialogContent dividers>
        {error ? (
          <Alert severity="error" sx={{ mb: 2 }}>
            {error}
          </Alert>
        ) : null}

        {wizard.step === 'pick' ? (
          <Stack spacing={1.5}>
            <ProviderCard
              icon={<PlaylistPlayIcon />}
              title={t('provider.M3U_URL')}
              description={t('provider.m3uDesc')}
              onClick={() => setWizard({ step: 'configure', provider: 'M3U_URL' })}
            />
            <ProviderCard
              icon={<FolderIcon />}
              title={t('provider.M3U_FILE')}
              description={t('provider.m3uDesc')}
              onClick={() => setWizard({ step: 'configure', provider: 'M3U_FILE' })}
            />
            <ProviderCard
              icon={<CloudIcon />}
              title={t('provider.GOOGLE_DRIVE')}
              description={t('provider.driveDesc')}
              onClick={() => setWizard({ step: 'configure', provider: 'GOOGLE_DRIVE' })}
            />
          </Stack>
        ) : null}

        {wizard.step === 'configure' ? (
          <Stack spacing={2}>
            <TextField
              label={t('form.name')}
              value={name}
              onChange={(event) => setName(event.target.value)}
              autoFocus
            />
            {wizard.provider === 'M3U_URL' ? (
              <TextField
                label={t('form.url')}
                value={url}
                onChange={(event) => setUrl(event.target.value)}
                helperText={t('form.urlHelper')}
              />
            ) : null}
            {wizard.provider === 'M3U_FILE' ? (
              <Button variant="outlined" component="label">
                {file ? file.name : t('form.chooseFile')}
                <input
                  hidden
                  type="file"
                  accept=".m3u,.m3u8"
                  onChange={(event) => setFile(event.target.files?.[0] ?? null)}
                />
              </Button>
            ) : null}
            {wizard.provider === 'GOOGLE_DRIVE' ? (
              <GoogleDrivePicker value={drive} onChange={setDrive} />
            ) : null}
            <FormControlLabel
              control={<Switch checked={autoSync} onChange={(event) => setAutoSync(event.target.checked)} />}
              label={t('form.autoSync')}
            />
            {autoSync ? (
              <TextField
                type="number"
                label={t('form.intervalMinutes')}
                value={interval}
                inputProps={{ min: 5, max: 10080 }}
                onChange={(event) => setIntervalMinutes(Number(event.target.value))}
              />
            ) : null}
          </Stack>
        ) : null}

        {wizard.step === 'select' ? (
          <Stack spacing={2}>
            <Stack direction="row" spacing={2}>
              <Metric label={t('preview.movies')} value={wizard.preview.movieCandidates} />
              <Metric label={t('preview.series')} value={wizard.preview.tvCandidates} />
              <Metric label={t('preview.unknown')} value={wizard.preview.unknownCandidates} />
            </Stack>
            <Typography variant="body2" color="text.secondary">
              {t('preview.totalEntries', { count: wizard.preview.totalEntries })}
            </Typography>
            <M3uSelectionTree preview={wizard.preview} value={selection} onChange={setSelection} />
          </Stack>
        ) : null}
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose}>{t('form.cancel')}</Button>
        {wizard.step === 'configure' && wizard.provider !== 'GOOGLE_DRIVE' ? (
          <Button
            variant="contained"
            disabled={busy || !name.trim() || (wizard.provider === 'M3U_URL' ? !url.trim() : !file)}
            onClick={() => analyze(wizard.provider)}
          >
            {busy ? t('form.analyzing') : t('form.analyze')}
          </Button>
        ) : null}
        {wizard.step === 'configure' && wizard.provider === 'GOOGLE_DRIVE' ? (
          <Button
            variant="contained"
            disabled={busy || !name.trim() || !drive || drive.folders.length === 0}
            onClick={createDrive}
          >
            {t('form.create')}
          </Button>
        ) : null}
        {wizard.step === 'select' ? (
          <Button variant="contained" disabled={busy} onClick={confirmSelection}>
            {busy ? t('preview.saving') : t('preview.confirm')}
          </Button>
        ) : null}
      </DialogActions>
    </Dialog>
  );
}

function ProviderCard({
  icon,
  title,
  description,
  onClick,
}: {
  icon: React.ReactNode;
  title: string;
  description: string;
  onClick: () => void;
}) {
  return (
    <Card variant="outlined">
      <CardActionArea onClick={onClick} sx={{ p: 2 }}>
        <Stack direction="row" spacing={1.5} alignItems="center">
          {icon}
          <Box>
            <Typography variant="body1">{title}</Typography>
            <Typography variant="caption" color="text.secondary">
              {description}
            </Typography>
          </Box>
        </Stack>
      </CardActionArea>
    </Card>
  );
}

function Metric({ label, value }: { label: string; value: number }) {
  return (
    <Box>
      <Typography variant="h3">{value.toLocaleString('pt-BR')}</Typography>
      <Typography variant="caption" color="text.secondary">
        {label}
      </Typography>
    </Box>
  );
}
