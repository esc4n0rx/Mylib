import { Box, Button, Card, CardContent, Chip, Stack, Typography } from '@mui/material';
import MovieIcon from '@mui/icons-material/Movie';
import LiveTvIcon from '@mui/icons-material/LiveTv';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import type { Library } from '@/api';
import { formatBytes, formatDateTime, formatNumber } from '@/utils/format';

export function LibraryCard({ library }: { library: Library }) {
  const { t } = useTranslation('libraries');
  const navigate = useNavigate();
  const Icon = library.type === 'MOVIE' ? MovieIcon : LiveTvIcon;
  const pathCount = library.paths?.length ?? 0;

  return (
    <Card>
      <CardContent>
        <Stack direction="row" spacing={1.5} alignItems="flex-start">
          <Box
            sx={{
              width: 40,
              height: 40,
              borderRadius: 2,
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              backgroundColor: (th) => th.tokens.surfaceContainerHigh,
              color: 'text.secondary',
              flexShrink: 0,
            }}
          >
            <Icon fontSize="small" />
          </Box>
          <Box sx={{ minWidth: 0, flex: 1 }}>
            <Typography variant="h3" noWrap>
              {library.name}
            </Typography>
            <Stack direction="row" spacing={0.5} sx={{ mt: 0.5 }} flexWrap="wrap" useFlexGap>
              <Chip size="small" variant="outlined" label={t(`type.${library.type}`)} />
              <Chip size="small" variant="outlined" label={t(`privacy.${library.privacy}`)} />
              <Chip size="small" variant="outlined" label={library.metadataLanguage} />
              <Chip size="small" color={library.operationalStatus === 'READY' ? 'success' : 'warning'} variant="outlined" label={t(`status.${library.operationalStatus}`)} />
            </Stack>
          </Box>
        </Stack>

        {library.description ? (
          <Typography variant="body1" color="text.secondary" sx={{ mt: 1.5 }}>
            {library.description}
          </Typography>
        ) : null}

        <Stack direction="row" spacing={3} sx={{ mt: 1.5 }}>
          <Typography variant="body2" color="text.secondary">
            {formatNumber(library.stats.mediaItemCount)} {t('stats.items').toLocaleLowerCase()}
          </Typography>
          <Typography variant="body2" color="text.secondary">
            {formatBytes(library.stats.totalSizeBytes)}
          </Typography>
          <Typography variant="body2" color="text.secondary">
            {t('card.pathsCount', { count: pathCount })}
          </Typography>
          <Typography variant="body2" color="text.secondary">
            {t('card.lastScan')}:{' '}
            {library.lastScanAt ? formatDateTime(library.lastScanAt) : t('card.never')}
          </Typography>
        </Stack>
        <Typography variant="caption" color="text.secondary" sx={{ display: 'block', mt: 1 }}>
          {t('sync.next')}: {library.nextSyncAt ? formatDateTime(library.nextSyncAt) : t('card.never')}
        </Typography>

        <Stack direction="row" spacing={1} sx={{ mt: 2 }}>
          <Button size="small" variant="contained" onClick={() => navigate(`/libraries/${library.id}`)}>
            {t('card.open')}
          </Button>
          <Button
            size="small"
            variant="outlined"
            onClick={() => navigate(`/libraries/${library.id}?tab=scan`)}
          >
            {t('card.scan')}
          </Button>
        </Stack>
      </CardContent>
    </Card>
  );
}
