import { Box, Button, Card, CardContent, Skeleton, Stack, Typography } from '@mui/material';
import { useNavigate } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { api } from '@/api';
import { StatusDot } from '@/components/StatusBadge';
import { LoadingState } from '@/components/states/LoadingState';
import { formatNumber } from '@/utils/format';
import { useLibraries } from '@/features/libraries/hooks';
import { MediaPosterCard } from '@/features/media/components/MediaPosterCard';
import { ContinueWatchingCard } from '@/features/playback/components/ContinueWatchingCard';
import { beginPlayback } from '@/features/playback/session';
import { useAuth } from '@/app/AuthProvider';
import { RecommendationSection, RecommendationSkeleton } from '@/features/recommendations/components/RecommendationSection';

export default function HomePage() {
  const { t } = useTranslation('common');
  const navigate = useNavigate();
  const { user } = useAuth();
  const serverQuery = useQuery({ queryKey: ['server'], queryFn: api.server.info });
  const librariesQuery = useLibraries();
  const recentQuery = useQuery({ queryKey: ['media', 'recent'], queryFn: () => api.media.recent({ limit: 20 }) });
  const continueQuery = useQuery({ queryKey: ['playback', 'continue'], queryFn: api.playback.continueWatching });
  const metricsQuery = useQuery({ queryKey: ['server','metrics'], queryFn: api.server.metrics, enabled: !!user?.isAdmin, refetchInterval: 10_000 });
  const recommendationsQuery = useQuery({ queryKey: ['recommendations','home'], queryFn: api.recommendations.home, staleTime: 5 * 60_000, retry: 1 });

  if (serverQuery.isLoading || librariesQuery.isLoading) return <LoadingState rows={3} />;

  const libraries = librariesQuery.data?.items ?? [];
  const unscanned = libraries.find((l) => !l.lastSuccessfulScanAt);

  return (
    <Box>
      <Box component="section" sx={{ display: 'grid', gridTemplateColumns: { xs: '1fr', md: 'minmax(0, 1fr) auto' }, gap: { xs: 3, md: 6 }, alignItems: 'center', mb: { xs: 6, md: 8 }, py: { xs: 2, md: 3 } }}>
        <Box>
          <Typography variant="overline" color="primary" sx={{ fontWeight: 750 }}>Sua biblioteca pessoal</Typography>
          <Typography variant="h1" sx={{ mt: 0.5, fontSize: { xs: 30, md: 38 }, letterSpacing: '-0.025em' }}>{t('home.welcomeTitle')}</Typography>
          <Typography color="text.secondary" sx={{ mt: 1, maxWidth: 620 }}>{t('home.welcomeBody')}</Typography>
          <Stack direction={{ xs: 'column', sm: 'row' }} spacing={1.5} sx={{ mt: 3, alignItems: { xs: 'stretch', sm: 'center' } }}>
            <Button variant="contained" onClick={() => navigate(libraries.length === 0 ? '/libraries/new' : '/libraries')}>{libraries.length === 0 ? t('home.createFirstLibrary') : t('home.manageLibraries')}</Button>
            {unscanned ? <Button variant="outlined" onClick={() => navigate(`/libraries/${unscanned.id}?tab=scan`)}>{t('home.scanLibrary')}</Button> : null}
          </Stack>
        </Box>
        <Stack direction="row" spacing={2} useFlexGap flexWrap="wrap" sx={{ width: { xs: '100%', md: 'auto' }, maxWidth: {md: 380} }}>
          <SummaryCard label="Status" value={t('home.serverOnline')} status={serverQuery.data?.status === 'online'} />
          <SummaryCard label={t('home.librariesCount')} value={formatNumber(libraries.length)} />
          {user?.isAdmin ? <SummaryCard label="Streams ativos" value={formatNumber(metricsQuery.data?.activePlaybackSessions??0)} /> : null}
          {user?.isAdmin ? <SummaryCard label="Jobs ativos" value={formatNumber((metricsQuery.data?.activeScanJobs??0)+(metricsQuery.data?.queuedJobs??0))} /> : null}
        </Stack>
      </Box>
      {continueQuery.data?.items.length ? <Box component="section"><Typography variant="h2" sx={{ mb: 2 }}>Continuar assistindo</Typography><Stack direction="row" spacing={2} sx={{ overflowX: 'auto', pb: 1, mx: { xs: -2, md: 0 }, px: { xs: 2, md: 0 }, scrollSnapType: 'x proximity', '& > *': { scrollSnapAlign: 'start' } }}>{continueQuery.data.items.map(item => <ContinueWatchingCard key={item.episodeId ?? item.mediaItemId} item={item} onPlay={async () => { const value = await beginPlayback({ mediaItemId: item.mediaItemId, episodeId: item.episodeId }); navigate(`/player/${value.response.sessionId}`); }} />)}</Stack></Box> : null}
      <Stack spacing={{ xs: 6, md: 8 }} sx={{ mt: continueQuery.data?.items.length ? { xs: 6, md: 8 } : 0 }}>
        {recommendationsQuery.isLoading ? <RecommendationSkeleton /> : null}
        {recommendationsQuery.isSuccess ? recommendationsQuery.data.sections.map((section) => <RecommendationSection key={section.key} section={section} />) : null}
      </Stack>
      <Box component="section" sx={{ mt: { xs: 6, md: 8 } }}>
        <Typography variant="h2" sx={{ mb: 2 }}>Adicionados recentemente</Typography>
        {recentQuery.isLoading ? <Box sx={{ display: 'grid', gridTemplateColumns: { xs: 'repeat(2, 1fr)', sm: 'repeat(8, minmax(120px, 1fr))' }, gap: { xs: 2, md: 3 }, overflow: 'hidden' }}>{Array.from({ length: 8 }, (_, i) => <Skeleton key={i} variant="rounded" sx={{ aspectRatio: '2/3' }} />)}</Box> : recentQuery.data?.items.length ? <Box sx={{ display: 'grid', gridAutoFlow: 'column', gridAutoColumns: { xs: '42vw', sm: 150, md: 'minmax(130px, 1fr)' }, gridTemplateColumns: { md: 'repeat(8, minmax(130px, 1fr))' }, gap: { xs: 2, md: 3 }, overflowX: 'auto', pb: 1, mx: { xs: -2, md: 0 }, px: { xs: 2, md: 0 }, scrollSnapType: 'x proximity', '& > *': { scrollSnapAlign: 'start' } }}>{recentQuery.data.items.map((item) => <MediaPosterCard key={item.id} item={item} />)}</Box> : <Typography color="text.secondary">Os conteúdos catalogados aparecerão aqui após o primeiro scan.</Typography>}
      </Box>
    </Box>
  );
}

function SummaryCard({ label, value, status }: { label: string; value: string; status?: boolean }) {
  return (
    <Card sx={{ flex: 1, width: { md: 170 } }}><CardContent sx={{ p: 2.5, '&:last-child': { pb: 2.5 } }}>
      <Typography variant="overline" color="text.secondary">{label}</Typography>
      <Stack direction="row" spacing={1} alignItems="center" sx={{ mt: 0.5 }}>
        {status !== undefined ? <StatusDot tone={status ? 'success' : 'error'} /> : null}
        <Typography variant="h3" noWrap>{value}</Typography>
      </Stack>
    </CardContent></Card>
  );
}
