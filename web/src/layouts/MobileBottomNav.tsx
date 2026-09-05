import { BottomNavigation, BottomNavigationAction, Paper } from '@mui/material';
import HomeIcon from '@mui/icons-material/Home';
import MovieIcon from '@mui/icons-material/Movie';
import LiveTvIcon from '@mui/icons-material/LiveTv';
import VideoLibraryIcon from '@mui/icons-material/VideoLibrary';
import FavoriteIcon from '@mui/icons-material/Favorite';
import { useTranslation } from 'react-i18next';
import { useLocation, useNavigate } from 'react-router-dom';

const items = [
  { to: '/home', key: 'nav.home', icon: <HomeIcon /> },
  { to: '/movies', key: 'nav.movies', icon: <MovieIcon /> },
  { to: '/tv', key: 'nav.tvShows', icon: <LiveTvIcon /> },
  { to: '/libraries', key: 'nav.libraries', icon: <VideoLibraryIcon /> },
  { to: '/favorites', key: 'nav.favorites', icon: <FavoriteIcon /> },
];

export function MobileBottomNav() {
  const { t } = useTranslation('common');
  const navigate = useNavigate();
  const { pathname } = useLocation();
  const active = items.find((item) => pathname.startsWith(item.to))?.to ?? false;

  return (
    <Paper
      component="nav"
      aria-label="Navegação principal"
      elevation={8}
      sx={{
        display: { xs: 'block', md: 'none' },
        position: 'fixed',
        zIndex: (theme) => theme.zIndex.appBar,
        left: 0,
        right: 0,
        bottom: 0,
        borderRadius: 0,
        borderTop: (theme) => `1px solid ${theme.tokens.outlineVariant}`,
        pb: 'env(safe-area-inset-bottom)',
      }}
    >
      <BottomNavigation
        showLabels
        value={active}
        onChange={(_event, value: string) => navigate(value)}
        sx={{
          height: 64,
          bgcolor: 'background.paper',
          '& .MuiBottomNavigationAction-root': { minWidth: 0, px: 0.5 },
          '& .MuiBottomNavigationAction-label': { fontSize: 10, mt: 0.25 },
          '& .Mui-selected .MuiBottomNavigationAction-label': { fontSize: 10, fontWeight: 700 },
        }}
      >
        {items.map((item) => (
          <BottomNavigationAction key={item.to} value={item.to} label={t(item.key)} icon={item.icon} />
        ))}
      </BottomNavigation>
    </Paper>
  );
}
