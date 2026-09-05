import { useEffect, useState } from 'react';
import { Box, Button, Divider, LinearProgress, Stack, Typography } from '@mui/material';
import { useTranslation } from 'react-i18next';
import { ApiError, TERMINAL_SCAN_STATUSES, type Library } from '@/api';
import { useToast } from '@/app/ToastProvider';
import { ConfirmationDialog } from '@/components/ConfirmationDialog';
import { SectionHeader } from '@/components/PageHeader';
import { formatDateTime, formatNumber, formatPercent } from '@/utils/format';
import {
  useCancelScan,
  useScanHistory,
  useScanProgress,
  useStartScan,
} from '../hooks';
import { ScanStatusChip } from './ScanStatusChip';

export function ScanPanel({ library }: { library: Library }) {
  const { t } = useTranslation('libraries');
  const { notify } = useToast();
  const [activeScanId, setActiveScanId] = useState<string | null>(null);
  const [confirmCancel, setConfirmCancel] = useState(false);

  const history = useScanHistory(library.id);
  const startScan = useStartScan(library.id);
  const cancelScan = useCancelScan(library.id);
  const progress = useScanProgress(library.id, activeScanId);

  // Pick up an already-running scan from history on mount.
  useEffect(() => {
    if (activeScanId) return;
    const running = history.data?.items.find(
      (s) => !TERMINAL_SCAN_STATUSES.includes(s.status),
    );
    if (running) setActiveScanId(running.id);
  }, [history.data, activeScanId]);

  useEffect(() => {
    const status = progress.data?.status;
    if (status && TERMINAL_SCAN_STATUSES.includes(status)) {
      if (status.startsWith('COMPLETED')) notify(t('toast.scanCompleted'), 'success');
      void history.refetch();
    }
  }, [progress.data?.status]); // eslint-disable-line react-hooks/exhaustive-deps

  const scan = progress.data;
  const isRunning = Boolean(scan && !TERMINAL_SCAN_STATUSES.includes(scan.status));

  const handleStart = async () => {
    try {
      const res = await startScan.mutateAsync(undefined);
      setActiveScanId(res.jobId);
      notify(t('toast.scanStarted'), 'success');
    } catch (err) {
      notify(err instanceof ApiError ? err.localizedMessage : t('toast.scanStarted'), 'error');
    }
  };

  return (
    <Box>
      <SectionHeader
        title={t('detail.scan')}
        action={
          isRunning ? (
            <Button color="error" variant="outlined" onClick={() => setConfirmCancel(true)}>
              {t('scan.cancel')}
            </Button>
          ) : (
            <Button
              variant="contained"
              onClick={handleStart}
              disabled={startScan.isPending || !library.scanEnabled}
            >
              {t('scan.start')}
            </Button>
          )
        }
      />

      {scan ? (
        <Stack spacing={2}>
          <Stack direction="row" spacing={1} alignItems="center">
            <ScanStatusChip status={scan.status} />
            <Typography variant="h3">{formatPercent(scan.progress)}</Typography>
          </Stack>
          <LinearProgress
            variant={scan.status === 'QUEUED' ? 'indeterminate' : 'determinate'}
            value={Math.min(100, Math.max(0, scan.progress))}
          />
          <Stack direction="row" flexWrap="wrap" useFlexGap spacing={4}>
            <Metric label={t('scan.discovered')} value={scan.discoveredFiles} />
            <Metric label={t('scan.processed')} value={scan.processedFiles} />
            <Metric label={t('scan.matched')} value={scan.matchedFiles} />
            <Metric label={t('scan.unmatched')} value={scan.unmatchedFiles} />
            <Metric label={t('scan.failed')} value={scan.failedFiles} />
          </Stack>
        </Stack>
      ) : (
        <Typography variant="body1" color="text.secondary">
          {t('scan.noHistory')}
        </Typography>
      )}

      <Divider sx={{ my: 3 }} />
      <Typography variant="h3" sx={{ mb: 1.5 }}>
        {t('scan.history')}
      </Typography>
      <Stack spacing={1}>
        {history.data?.items.length ? (
          history.data.items.map((item) => (
            <Stack
              key={item.id}
              direction="row"
              justifyContent="space-between"
              alignItems="center"
              sx={{
                px: 1.5,
                py: 1,
                border: (th) => `1px solid ${th.tokens.outlineVariant}`,
                borderRadius: 2,
              }}
            >
              <Typography variant="body1">
                {formatDateTime(item.startedAt ?? item.createdAt)} · {item.triggerSource} · {item.scanType} · {formatNumber(item.processedFiles)}
              </Typography>
              <ScanStatusChip status={item.status} />
            </Stack>
          ))
        ) : (
          <Typography variant="body2" color="text.secondary">
            {t('scan.noHistory')}
          </Typography>
        )}
      </Stack>

      <ConfirmationDialog
        open={confirmCancel}
        title={t('scan.cancelConfirmTitle')}
        body={t('scan.cancelConfirmBody')}
        confirmLabel={t('scan.cancel')}
        destructive
        loading={cancelScan.isPending}
        onClose={() => setConfirmCancel(false)}
        onConfirm={async () => {
          if (activeScanId) await cancelScan.mutateAsync(activeScanId);
          setConfirmCancel(false);
        }}
      />
    </Box>
  );
}

function Metric({ label, value }: { label: string; value: number }) {
  return (
    <Box>
      <Typography variant="overline" color="text.secondary">
        {label}
      </Typography>
      <Typography variant="h3">{formatNumber(value)}</Typography>
    </Box>
  );
}
