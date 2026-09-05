import { Alert, Box, Button, Card, CardContent, Chip, Stack, Tab, Tabs, Typography } from '@mui/material';
import { useState } from 'react';
import { useParams, useSearchParams } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { PageHeader, SectionHeader } from '@/components/PageHeader';
import { QueryBoundary } from '@/components/QueryBoundary';
import { EmptyState } from '@/components/states/EmptyState';
import { formatBytes, formatDateTime, formatNumber } from '@/utils/format';
import type { Library } from '@/api';
import { useLibrary, useLibraryStats, usePathStatuses, useUnmatched } from '../hooks';
import { ScanPanel } from '../components/ScanPanel';
import { ManualIdentifyDialog } from '../components/ManualIdentifyDialog';
import type { UnmatchedItem } from '@/api';
import { AutoSyncPanel } from '../components/AutoSyncPanel';
import { RemoteSourcesPanel } from '../components/RemoteSourcesPanel';

const TABS = ['overview', 'paths', 'sources', 'sync', 'scan', 'unmatched'] as const;
type TabKey = (typeof TABS)[number];

export default function LibraryDetailPage() {
  const { id = '' } = useParams();
  const { t } = useTranslation('libraries');
  const [params, setParams] = useSearchParams();
  const query = useLibrary(id);

  const tab = (params.get('tab') as TabKey) ?? 'overview';

  return (
    <Box>
      <QueryBoundary query={query}>
        {(library) => (
          <>
            <PageHeader
              title={library.name}
              description={library.description}
              actions={
                <Stack direction="row" spacing={0.5}>
                  <Chip
                    size="small"
                    variant="outlined"
                    label={t(`type.${library.type}`)}
                  />
                  <Chip
                    size="small"
                    variant="outlined"
                    label={t(`privacy.${library.privacy}`)}
                  />
                  <Chip
                    size="small"
                    variant="outlined"
                    label={library.metadataLanguage}
                  />
                </Stack>
              }
            />
            <Tabs
              value={tab}
              onChange={(_, value) => setParams({ tab: value })}
              sx={{
                mb: 3,
                borderBottom: (th) => `1px solid ${th.tokens.outlineVariant}`,
              }}
            >
              {TABS.map((key) => (
                <Tab key={key} value={key} label={t(`detail.${key}`)} />
              ))}
            </Tabs>

            {tab === 'overview' ? <Overview library={library} /> : null}
            {tab === 'paths' ? <Paths library={library} /> : null}
            {tab === 'sources' ? <RemoteSourcesPanel library={library} /> : null}
            {tab === 'sync' ? <AutoSyncPanel library={library} /> : null}
            {tab === 'scan' ? <ScanPanel library={library} /> : null}
            {tab === 'unmatched' ? <Unmatched libraryId={library.id} /> : null}
          </>
        )}
      </QueryBoundary>
    </Box>
  );
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

function Overview({ library }: { library: Library }) {
  const { t } = useTranslation('libraries');
  const stats = useLibraryStats(library.id);
  const values = stats.data ?? library.stats;
  return (
    <Box>
      <SectionHeader title={t('detail.overview')} />
      {library.operationalStatus === 'PATH_UNAVAILABLE' ? <Alert severity="warning" sx={{ mb: 2 }}>{t('paths.unavailableWarning')}</Alert> : null}
      <Box sx={{ display: 'grid', gridTemplateColumns: 'repeat(auto-fit,minmax(150px,1fr))', gap: 1.5, mb: 3 }}>
        <Stat label={t('stats.size')} value={formatBytes(values.totalSizeBytes)} />
        <Stat label={t('stats.items')} value={formatNumber(values.mediaItemCount)} />
        <Stat label={t('stats.files')} value={formatNumber(values.fileCount)} />
        <Stat label={t('stats.unmatched')} value={formatNumber(values.unmatchedCount)} />
      </Box>
      <Box sx={{ maxWidth: 560 }}>
        <Row label={t('form.type')} value={t(`type.${library.type}`)} />
        <Row label={t('privacy.label')} value={t(`privacy.${library.privacy}`)} />
        <Row label={t('detail.status')} value={t(`status.${library.operationalStatus}`)} />
        <Row label={t('card.lastScan')} value={library.lastScanAt ? formatDateTime(library.lastScanAt) : t('card.never')} />
      </Box>
    </Box>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return <Card variant="outlined"><CardContent><Typography variant="caption" color="text.secondary">{label}</Typography><Typography variant="h2">{value}</Typography></CardContent></Card>;
}

function Paths({ library }: { library: Library }) {
  const { t } = useTranslation('libraries');
  const statuses = usePathStatuses(library.id);
  const paths = statuses.data?.items ?? library.paths ?? [];
  return (
    <Box sx={{ maxWidth: 640 }}>
      <SectionHeader title={t('detail.paths')} />
      <Stack spacing={1}>
        {paths.map((p) => (
          <Stack
            key={p.id}
            direction="row"
            justifyContent="space-between"
            sx={{
              px: 1.5,
              py: 1,
              border: (th) => `1px solid ${th.tokens.outlineVariant}`,
              borderRadius: 2,
            }}
          >
            <Typography
              variant="body1"
              sx={{ fontFamily: 'monospace', wordBreak: 'break-all' }}
            >
              {p.path}
            </Typography>
            <Stack alignItems="flex-end" spacing={0.5}>
              <Chip size="small" color={p.status === 'AVAILABLE' ? 'success' : 'warning'} variant="outlined" label={p.status} />
              <Typography variant="caption" color="text.secondary">{p.lastCheckedAt ? formatDateTime(p.lastCheckedAt) : '—'}</Typography>
            </Stack>
          </Stack>
        ))}
      </Stack>
    </Box>
  );
}

function Unmatched({ libraryId }: { libraryId: string }) {
  const { t } = useTranslation('libraries');
  const query = useUnmatched(libraryId);
  const [selected, setSelected] = useState<UnmatchedItem>();
  return (
    <Box>
      <SectionHeader title={t('unmatched.title')} />
      <Typography variant="body2" color="text.secondary" sx={{ mb: 2 }}>
        {t('unmatched.identifyNote')}
      </Typography>
      <QueryBoundary query={query}>
        {(data) =>
          data.items.length === 0 ? (
            <EmptyState title={t('unmatched.empty')} />
          ) : (
            <Stack spacing={1}>
              {data.items.map((item) => (
                <Stack
                  key={item.mediaFileId}
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
                  <Box minWidth={0}>
                    <Typography variant="body1" sx={{ wordBreak: 'break-all' }}>
                      {item.filename}
                    </Typography>
                    <Typography variant="caption" color="text.secondary">
                      {item.normalizedTitle ?? 'Título não detectado'}
                      {item.season !== undefined
                        ? ` · T${item.season} E${item.episode}`
                        : ''}{' '}
                      · {item.status}
                    </Typography>
                  </Box>
                  <Button
                    size="small"
                    variant="outlined"
                    onClick={() => setSelected(item)}
                  >
                    {t('unmatched.identify')}
                  </Button>
                </Stack>
              ))}
            </Stack>
          )
        }
      </QueryBoundary>
      <ManualIdentifyDialog
        libraryId={libraryId}
        item={selected}
        open={Boolean(selected)}
        onClose={() => setSelected(undefined)}
      />
    </Box>
  );
}
