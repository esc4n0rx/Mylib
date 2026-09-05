import { Box, styled } from '@mui/material';
import { Outlet, useLocation } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { motion } from 'framer-motion';
import { api } from '@/api';
import { AppFrame } from './AppFrame';
import { Sidebar } from './Sidebar';
import { Topbar } from './Topbar';
import { MobileBottomNav } from './MobileBottomNav';

const TITLE_BY_PREFIX: Record<string, string> = {
  '/home': 'nav.home',
  '/movies': 'nav.movies',
  '/media': 'nav.movies',
  '/tv': 'nav.tvShows',
  '/favorites': 'nav.favorites',
  '/libraries': 'nav.libraries',
  '/users': 'nav.users',
  '/settings': 'nav.settings',
  '/activity': 'nav.activity',
};

const MotionMain = styled(motion.main)(({ theme }) => ({
  flex: 1,
  overflowY: 'auto',
  padding: theme.spacing(4),
  scrollBehavior: 'smooth',
  [theme.breakpoints.down('md')]: {
    padding: theme.spacing(2),
    paddingBottom: `calc(${theme.spacing(20)} + env(safe-area-inset-bottom))`,
  },
}));

export function AppShell() {
  const { t } = useTranslation('common');
  const location = useLocation();
  const serverQuery = useQuery({ queryKey: ['server'], queryFn: api.server.info, retry: false });

  const prefix = Object.keys(TITLE_BY_PREFIX).find((p) => location.pathname.startsWith(p));
  const titleKey = prefix ? TITLE_BY_PREFIX[prefix] : undefined;
  const title = titleKey ? t(titleKey) : 'MyLib';

  return (
    <AppFrame>
      <Box sx={{ display: 'flex', flex: 1, minHeight: 0 }}>
        <Sidebar serverName={serverQuery.data?.name} />
        <Box sx={{ flex: 1, display: 'flex', flexDirection: 'column', minWidth: 0 }}>
          <Topbar title={title} />
          <MotionMain
            key={location.pathname}
            initial={{ opacity: 0, y: 8 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ duration: 0.32, ease: [0.22, 1, 0.36, 1] }}
          >
            <Outlet />
          </MotionMain>
        </Box>
        <MobileBottomNav />
      </Box>
    </AppFrame>
  );
}
