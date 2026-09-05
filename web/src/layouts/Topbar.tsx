import { Badge, Box, IconButton, InputBase, Stack, Tooltip, Typography } from '@mui/material';
import SearchIcon from '@mui/icons-material/Search';
import SyncIcon from '@mui/icons-material/Sync';
import NotificationsNoneIcon from '@mui/icons-material/NotificationsNone';
import { useTranslation } from 'react-i18next';
import { UserAvatarMenu } from './UserAvatarMenu';
import { useQuery } from '@tanstack/react-query';
import { api } from '@/api';
import { useAuth } from '@/app/AuthProvider';
import { useNavigate } from 'react-router-dom';

export function Topbar({ title }: { title: string }) {
  const { t } = useTranslation('common');
  const { user } = useAuth(); const navigate = useNavigate();
  const alerts = useQuery({ queryKey:['server','alerts'], queryFn:api.server.alerts, enabled:!!user?.isAdmin, refetchInterval:30_000 });
  const critical = alerts.data?.items.filter(item=>item.severity==='CRITICAL').length ?? 0;
  return (
    <Stack
      direction="row"
      alignItems="center"
      justifyContent="space-between"
      sx={{
        px: { xs: 2, md: 3 },
        py: { xs: 1, md: 1.5 },
        minHeight: { xs: 60, md: 58 },
        borderBottom: (th) => `1px solid ${th.tokens.outlineVariant}`,
        backgroundColor: (th) => th.tokens.surface,
        position: { xs: 'sticky', md: 'static' },
        top: 0,
        zIndex: (th) => th.zIndex.appBar,
      }}
    >
      <Typography variant="h2" noWrap sx={{ fontSize: { xs: 18, md: 20 }, maxWidth: { xs: 150, sm: 'none' } }}>{title}</Typography>
      <Stack direction="row" spacing={{ xs: 0.25, sm: 1.5 }} alignItems="center">
        <Tooltip title={t('topbar.searchComingSoon')}>
          <Box
            sx={{
              display: { xs: 'none', sm: 'flex' },
              alignItems: 'center',
              gap: 1,
              px: 1.5,
              height: 34,
              borderRadius: 2,
              border: (th) => `1px solid ${th.tokens.outlineVariant}`,
              color: 'text.secondary',
              opacity: 0.7,
            }}
          >
            <SearchIcon fontSize="small" />
            <InputBase
              disabled
              placeholder={t('topbar.searchPlaceholder')}
              sx={{ fontSize: 13, width: 160 }}
            />
          </Box>
        </Tooltip>
        <Tooltip title={t('topbar.searchPlaceholder')}>
          <IconButton aria-label={t('topbar.searchPlaceholder')} sx={{ display: { xs: 'inline-flex', sm: 'none' } }}>
            <SearchIcon fontSize="small" />
          </IconButton>
        </Tooltip>
        <SyncIcon fontSize="small" sx={{ color: 'text.secondary', display: { xs: 'none', sm: 'block' } }} />
        <IconButton aria-label="Notificações" onClick={()=>user?.isAdmin&&navigate('/activity')}><Badge color="error" badgeContent={critical} invisible={!critical}><NotificationsNoneIcon fontSize="small" /></Badge></IconButton>
        <UserAvatarMenu />
      </Stack>
    </Stack>
  );
}
