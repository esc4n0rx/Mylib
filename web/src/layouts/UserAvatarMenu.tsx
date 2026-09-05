import { useState } from 'react';
import { Avatar, Divider, IconButton, ListItemText, Menu, MenuItem, Typography } from '@mui/material';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { useAuth } from '@/app/AuthProvider';

export function UserAvatarMenu() {
  const { t } = useTranslation('common');
  const { user, profile, logout } = useAuth();
  const navigate = useNavigate();
  const [anchor, setAnchor] = useState<HTMLElement | null>(null);

  const initials = (profile?.name ?? user?.displayName ?? user?.username ?? '?')
    .split(' ')
    .map((p) => p[0])
    .slice(0, 2)
    .join('')
    .toUpperCase();

  return (
    <>
      <IconButton onClick={(e) => setAnchor(e.currentTarget)} size="small" aria-label="Menu do usuário">
        <Avatar src={profile?.avatarUrl} sx={{ width: 30, height: 30, fontSize: 13 }}>{initials}</Avatar>
      </IconButton>
      <Menu anchorEl={anchor} open={Boolean(anchor)} onClose={() => setAnchor(null)}>
        <MenuItem disabled sx={{ opacity: '1 !important' }}>
          <ListItemText
            primary={profile?.name ?? user?.displayName ?? user?.username}
            secondary={profile?.isKids ? 'Perfil infantil' : user?.username}
            primaryTypographyProps={{ variant: 'h3' }}
            secondaryTypographyProps={{ variant: 'body2' }}
          />
        </MenuItem>
        <Divider />
        <MenuItem onClick={() => { setAnchor(null); navigate('/profiles'); }}>
          <Typography variant="body1">{t('userMenu.switchProfile')}</Typography>
        </MenuItem>
        <MenuItem disabled><Typography variant="body1">{t('userMenu.account')}</Typography></MenuItem>
        {!profile?.isKids ? <MenuItem
          onClick={() => {
            setAnchor(null);
            navigate('/settings');
          }}
        >
          <Typography variant="body1">{t('userMenu.settings')}</Typography>
        </MenuItem> : null}
        <Divider />
        <MenuItem
          onClick={() => {
            setAnchor(null);
            logout();
            navigate('/login', { replace: true });
          }}
        >
          <Typography variant="body1">{t('userMenu.signOut')}</Typography>
        </MenuItem>
      </Menu>
    </>
  );
}
