import { Box, Card, CardContent, Divider, Stack, Typography } from '@mui/material';
import DnsIcon from '@mui/icons-material/Dns';
import { useTranslation } from 'react-i18next';
import { StatusDot } from '@/components/StatusBadge';

export function ServerPreview({ name, language }: { name: string; language: string }) {
  const { t } = useTranslation('setup');
  return (
    <Card sx={{ width: '100%', maxWidth: 320 }}>
      <Box
        sx={{
          height: 88,
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          background: (th) =>
            `linear-gradient(135deg, ${th.tokens.primaryContainer}, ${th.tokens.surfaceContainerHigh})`,
        }}
      >
        <DnsIcon sx={{ fontSize: 40, color: (th) => th.tokens.primary }} />
      </Box>
      <CardContent>
        <Stack direction="row" justifyContent="space-between" alignItems="center">
          <Typography variant="h2">{name || t('server.namePlaceholder')}</Typography>
          <Stack direction="row" spacing={0.5} alignItems="center">
            <StatusDot tone="success" />
            <Typography variant="overline">{t('server.previewReady')}</Typography>
          </Stack>
        </Stack>
        <Typography variant="body1" color="text.secondary">
          {t('server.previewPlatform')}
        </Typography>
        <Divider sx={{ my: 1.5 }} />
        <Stack direction="row" justifyContent="space-between">
          <Typography variant="body2" color="text.secondary">
            {t('server.language')}
          </Typography>
          <Typography variant="body2">{language}</Typography>
        </Stack>
      </CardContent>
    </Card>
  );
}
