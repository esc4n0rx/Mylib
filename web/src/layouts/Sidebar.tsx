import { Box, Chip, List, ListItemButton, ListItemIcon, ListItemText, Stack, Typography } from '@mui/material';
import HomeIcon from '@mui/icons-material/Home';
import MovieIcon from '@mui/icons-material/Movie';
import LiveTvIcon from '@mui/icons-material/LiveTv';
import VideoLibraryIcon from '@mui/icons-material/VideoLibrary';
import SettingsIcon from '@mui/icons-material/Settings';
import FavoriteIcon from '@mui/icons-material/Favorite';
import MonitorHeartOutlinedIcon from '@mui/icons-material/MonitorHeartOutlined';
import type { ReactNode } from 'react';
import { NavLink } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { BrandMark } from '@/components/BrandMark';
import { useAuth } from '@/app/AuthProvider';

export const SIDEBAR_WIDTH = 240;

interface NavItem {
  to: string;
  labelKey: string;
  icon: ReactNode;
  disabled?: boolean;
}

export function Sidebar({ serverName }: { serverName?: string }) {
  const { t } = useTranslation('common');
  const { user, profile } = useAuth();

  const media: NavItem[] = [
    { to: '/home', labelKey: 'nav.home', icon: <HomeIcon /> },
    { to: '/movies', labelKey: 'nav.movies', icon: <MovieIcon /> },
    { to: '/tv', labelKey: 'nav.tvShows', icon: <LiveTvIcon /> },
    ...(!profile?.isKids ? [{ to: '/libraries', labelKey: 'nav.libraries', icon: <VideoLibraryIcon /> }] : []),
    { to: '/favorites', labelKey: 'nav.favorites', icon: <FavoriteIcon /> },
  ];
  const server: NavItem[] = profile?.isKids ? [] : [
    { to: '/settings', labelKey: 'nav.settings', icon: <SettingsIcon /> },
    ...(user?.isAdmin ? [{ to: '/activity', labelKey: 'nav.activity', icon: <MonitorHeartOutlinedIcon /> }] : []),
  ];

  return (
    <Box
      component="nav"
      sx={{
        width: SIDEBAR_WIDTH,
        flexShrink: 0,
        borderRight: (th) => `1px solid ${th.tokens.outlineVariant}`,
        backgroundColor: (th) => th.tokens.surfaceContainerLow,
        display: { xs: 'none', md: 'flex' },
        flexDirection: 'column',
        p: 2,
      }}
    >
      <Stack direction="row" spacing={1.5} alignItems="center" sx={{ px: 1, py: 1, mb: 2 }}>
        <BrandMark />
        <Box sx={{ minWidth: 0 }}>
          <Typography variant="h3">MyLib</Typography>
          <Typography variant="body2" color="text.secondary" noWrap>
            {serverName ?? '—'}
          </Typography>
        </Box>
      </Stack>

      <NavGroup title={t('nav.sectionMedia')} items={media} />
      <Box sx={{ height: 16 }} />
      <NavGroup title={t('nav.sectionServer')} items={server} />
    </Box>
  );
}

function NavGroup({ title, items }: { title: string; items: NavItem[] }) {
  const { t } = useTranslation('common');
  return (
    <Box>
      <Typography variant="overline" color="text.secondary" sx={{ px: 1 }}>
        {title}
      </Typography>
      <List dense disablePadding sx={{ mt: 0.5 }}>
        {items.map((item) =>
          item.disabled ? (
            <ListItemButton key={item.to} disabled sx={{ borderRadius: 1.5, mb: 0.25 }}>
              <ListItemIcon sx={{ minWidth: 34, '& svg': { fontSize: 20 } }}>
                {item.icon}
              </ListItemIcon>
              <ListItemText primaryTypographyProps={{ variant: 'body1' }}>
                {t(item.labelKey)}
              </ListItemText>
              <Chip size="small" variant="outlined" label={t('states.comingSoon')} />
            </ListItemButton>
          ) : (
          <ListItemButton
            key={item.to}
            component={NavLink}
            to={item.to}
            sx={{
              borderRadius: 1.5,
              mb: 0.25,
              '&.active': {
                backgroundColor: (th) => th.tokens.sidebarActiveBg,
                color: (th) => th.tokens.sidebarActiveText,
                '& .MuiListItemIcon-root': { color: (th) => th.tokens.sidebarActiveText },
              },
            }}
          >
            <ListItemIcon sx={{ minWidth: 34, '& svg': { fontSize: 20 } }}>
              {item.icon}
            </ListItemIcon>
            <ListItemText primaryTypographyProps={{ variant: 'body1' }}>
              {t(item.labelKey)}
            </ListItemText>
          </ListItemButton>
          ),
        )}
      </List>
    </Box>
  );
}
