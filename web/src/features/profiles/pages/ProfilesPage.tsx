import { useEffect, useMemo, useState } from 'react';
import {
  Alert, Avatar, Box, Button, Card, CardActionArea, Checkbox, Chip, CircularProgress,
  Dialog, DialogActions, DialogContent, DialogTitle, FormControlLabel, Grid, IconButton,
  MenuItem, Pagination, Stack, Tab, Tabs, TextField, Typography,
} from '@mui/material';
import AddIcon from '@mui/icons-material/Add';
import EditIcon from '@mui/icons-material/Edit';
import LockOutlinedIcon from '@mui/icons-material/LockOutlined';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import { useTranslation } from 'react-i18next';
import { api, ApiError, type AvatarItem, type Profile } from '@/api';
import { useAuth } from '@/app/AuthProvider';
import { AppFrame } from '@/layouts/AppFrame';
import { BrandMark } from '@/components/BrandMark';

const AGE_OPTIONS = [0, 10, 12, 14, 16, 18] as const;
const CATEGORIES: Array<{ id?: AvatarItem['category']; label: string }> = [
  { label: 'Todos' }, { id: 'dp', label: 'Disney+' }, { id: 'nf', label: 'Netflix' },
  { id: 'pop', label: 'Pop' }, { id: 'pp', label: 'Famosos' }, { id: 'pv', label: 'Prime Video' },
];

export default function ProfilesPage() {
  const { t } = useTranslation('profiles');
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { user, selectProfile, logout } = useAuth();
  const [manage, setManage] = useState(false);
  const [pinTarget, setPinTarget] = useState<Profile>();
  const [pin, setPin] = useState('');
  const [editor, setEditor] = useState<Profile | null | undefined>();

  const profiles = useQuery({ queryKey: ['profiles'], queryFn: () => api.profiles.list() });
  const select = useMutation({
    mutationFn: ({ profile, pinValue }: { profile: Profile; pinValue?: string }) => api.profiles.select(profile.id, pinValue),
    onSuccess: (result) => {
      selectProfile(result.accessToken);
      navigate('/home', { replace: true });
    },
  });

  useEffect(() => {
    const items = profiles.data?.items;
    const only = items?.[0];
    if (!user?.profileId && items?.length === 1 && only && !only.pinProtected && !select.isPending) {
      select.mutate({ profile: only });
    }
  }, [profiles.data?.items, select, user?.profileId]);

  const choose = (profile: Profile) => {
    if (manage) { setEditor(profile); return; }
    if (profile.pinProtected) { setPin(''); setPinTarget(profile); return; }
    select.mutate({ profile });
  };

  return (
    <AppFrame>
      <Stack sx={{ minHeight: { xs: '100vh', md: 'calc(100vh - 48px)' }, p: { xs: 3, md: 6 } }}>
        <Stack direction="row" alignItems="center" justifyContent="space-between">
          <Stack direction="row" spacing={1.5} alignItems="center"><BrandMark /><Typography variant="h3">MyLib</Typography></Stack>
          <Button color="inherit" onClick={() => { logout(); navigate('/login', { replace: true }); }}>Sair</Button>
        </Stack>
        <Stack alignItems="center" justifyContent="center" spacing={4} sx={{ flex: 1, py: 6 }}>
          <Box textAlign="center">
            <Typography variant="h1">{t('whoIsWatching')}</Typography>
            <Typography color="text.secondary" sx={{ mt: 1 }}>{manage ? t('manageProfiles') : t('chooseProfile')}</Typography>
          </Box>
          {profiles.isLoading ? <CircularProgress /> : null}
          {profiles.isError ? <Alert severity="error">Não foi possível carregar os perfis.</Alert> : null}
          <Grid container spacing={3} justifyContent="center" sx={{ maxWidth: 900 }}>
            {profiles.data?.items.map((profile) => (
              <Grid item key={profile.id} xs={6} sm={4} md={3}>
                <Card elevation={0} sx={{ bgcolor: 'transparent', textAlign: 'center' }}>
                  <CardActionArea onClick={() => choose(profile)} sx={{ borderRadius: 3, p: 1 }} aria-label={`${manage ? 'Editar' : 'Selecionar'} ${profile.name}`}>
                    <Box sx={{ position: 'relative' }}>
                      <Avatar src={profile.avatarUrl} alt="" variant="rounded" sx={{ width: '100%', height: 'auto', aspectRatio: '1', borderRadius: 3, bgcolor: 'primary.main' }}>{profile.name[0]}</Avatar>
                      {profile.pinProtected ? <LockOutlinedIcon sx={{ position: 'absolute', right: 8, bottom: 8, bgcolor: 'background.paper', borderRadius: '50%', p: .5 }} /> : null}
                      {manage ? <EditIcon sx={{ position: 'absolute', inset: 0, m: 'auto', p: 1, borderRadius: '50%', bgcolor: 'rgba(0,0,0,.65)', color: 'white', width: 48, height: 48 }} /> : null}
                    </Box>
                    <Typography variant="h3" sx={{ mt: 1.5 }}>{profile.name}</Typography>
                    {profile.isKids ? <Chip size="small" label="Infantil" sx={{ mt: 1 }} /> : null}
                  </CardActionArea>
                </Card>
              </Grid>
            ))}
            {manage ? (
              <Grid item xs={6} sm={4} md={3}>
                <CardActionArea onClick={() => setEditor(null)} sx={{ borderRadius: 3, p: 1, textAlign: 'center' }}>
                  <Box sx={{ aspectRatio: '1', border: '2px dashed', borderColor: 'divider', borderRadius: 3, display: 'grid', placeItems: 'center' }}><AddIcon sx={{ fontSize: 52 }} /></Box>
                  <Typography variant="h3" sx={{ mt: 1.5 }}>{t('addProfile')}</Typography>
                </CardActionArea>
              </Grid>
            ) : null}
          </Grid>
          <Button variant={manage ? 'contained' : 'outlined'} onClick={() => setManage((value) => !value)}>{manage ? t('finish') : t('manageProfiles')}</Button>
        </Stack>
      </Stack>

      <Dialog open={Boolean(pinTarget)} onClose={() => setPinTarget(undefined)} fullWidth maxWidth="xs">
        <DialogTitle>Digite o PIN de {pinTarget?.name}</DialogTitle>
        <DialogContent>
          <TextField autoFocus fullWidth label="PIN" type="password" inputProps={{ inputMode: 'numeric', maxLength: 6 }} value={pin} onChange={(event) => setPin(event.target.value.replace(/\D/g, ''))} sx={{ mt: 1 }} />
          {select.error ? <Alert severity="error" sx={{ mt: 2 }}>{select.error instanceof ApiError && select.error.status === 429 ? t('rateLimited') : t('wrongPin')}</Alert> : null}
        </DialogContent>
        <DialogActions><Button onClick={() => setPinTarget(undefined)}>{t('cancel')}</Button><Button variant="contained" disabled={pin.length < 4 || select.isPending} onClick={() => pinTarget && select.mutate({ profile: pinTarget, pinValue: pin })}>{t('unlock')}</Button></DialogActions>
      </Dialog>

      <ProfileEditor profile={editor} open={editor !== undefined} onClose={() => setEditor(undefined)} onSaved={() => { setEditor(undefined); void queryClient.invalidateQueries({ queryKey: ['profiles'] }); }} />
    </AppFrame>
  );
}

export function ProfileEditor({ profile, userId, open, onClose, onSaved }: { profile: Profile | null | undefined; userId?: string; open: boolean; onClose: () => void; onSaved: () => void }) {
  const { t } = useTranslation('profiles');
  const [name, setName] = useState('');
  const [avatarId, setAvatarId] = useState('default.png');
  const [avatarUrl, setAvatarUrl] = useState('/api/v1/avatars/fallback/default.png');
  const [isKids, setKids] = useState(false);
  const [age, setAge] = useState<number>(18);
  const [newPin, setNewPin] = useState('');
  const [avatarsOpen, setAvatarsOpen] = useState(false);
  const libraries = useQuery({ queryKey: ['profiles', profile?.id, 'libraries'], queryFn: () => api.profiles.libraryAccess(profile!.id), enabled: Boolean(profile?.id && open) });
  const [libraryIds, setLibraryIds] = useState<string[]>([]);

  useEffect(() => {
    setName(profile?.name ?? ''); setAvatarId(profile?.avatarId ?? 'default.png'); setAvatarUrl(profile?.avatarUrl ?? '/api/v1/avatars/fallback/default.png');
    setKids(profile?.isKids ?? false); setAge(profile?.maxAgeRating ?? 18); setNewPin('');
  }, [profile, open]);
  useEffect(() => { if (libraries.data) setLibraryIds(libraries.data.libraries.filter((item) => item.isAllowed).map((item) => item.libraryId)); }, [libraries.data]);

  const save = useMutation({
    mutationFn: async () => {
      const saved = profile ? await api.profiles.update(profile.id, { name, avatarId, isKids, maxAgeRating: age as Profile['maxAgeRating'] }) : await api.profiles.create({ name, avatarId, isKids, maxAgeRating: age, userId });
      if (newPin) await api.profiles.setPin(saved.id, newPin);
      if (profile && libraries.data) await api.profiles.updateLibraryAccess(saved.id, libraryIds);
      return saved;
    }, onSuccess: onSaved,
  });
  const disable = useMutation({ mutationFn: () => api.profiles.disable(profile!.id), onSuccess: onSaved });

  return <>
    <Dialog open={open} onClose={onClose} fullWidth maxWidth="sm">
      <DialogTitle>{profile ? t('editProfile') : t('addProfile')}</DialogTitle>
      <DialogContent><Stack spacing={2.5} sx={{ mt: 1 }}>
        <Stack direction="row" spacing={2} alignItems="center"><IconButton onClick={() => setAvatarsOpen(true)}><Avatar src={avatarUrl} variant="rounded" sx={{ width: 72, height: 72 }}>{name[0]}</Avatar></IconButton><Button onClick={() => setAvatarsOpen(true)}>{t('selectAvatar')}</Button></Stack>
        <TextField label={t('name')} value={name} onChange={(event) => setName(event.target.value)} inputProps={{ maxLength: 40 }} />
        <FormControlLabel control={<Checkbox checked={isKids} onChange={(event) => { setKids(event.target.checked); if (event.target.checked && age > 12) setAge(12); }} />} label={t('kidsProfile')} />
        <TextField select label={t('maximumRating')} value={age} onChange={(event) => setAge(Number(event.target.value))}>{AGE_OPTIONS.map((value) => <MenuItem key={value} value={value}>{value === 0 ? 'Livre' : `${value} anos`}</MenuItem>)}</TextField>
        <TextField label={profile?.pinProtected ? 'Novo PIN (opcional)' : 'PIN (opcional)'} type="password" value={newPin} onChange={(event) => setNewPin(event.target.value.replace(/\D/g, '').slice(0, 6))} helperText="Use de 4 a 6 dígitos" />
        {libraries.data ? <Box><Typography variant="h3" sx={{ mb: 1 }}>{t('allowedLibraries')}</Typography>{libraries.data.libraries.map((library) => <FormControlLabel key={library.libraryId} sx={{ display: 'flex' }} control={<Checkbox checked={libraryIds.includes(library.libraryId)} onChange={(event) => setLibraryIds((ids) => event.target.checked ? [...ids, library.libraryId] : ids.filter((id) => id !== library.libraryId))} />} label={`${library.name} · ${library.minimumAge === 0 ? 'Livre' : `${library.minimumAge}+`}`} />)}</Box> : null}
        {save.error || disable.error ? <Alert severity="error">Não foi possível salvar o perfil.</Alert> : null}
      </Stack></DialogContent>
      <DialogActions sx={{ justifyContent: profile ? 'space-between' : 'flex-end' }}>
        {profile ? <Button color="error" onClick={() => disable.mutate()}>{t('disable')}</Button> : null}
        <Stack direction="row" spacing={1}><Button onClick={onClose}>{t('cancel')}</Button><Button variant="contained" disabled={!name.trim() || (newPin.length > 0 && newPin.length < 4) || save.isPending} onClick={() => save.mutate()}>{t('save')}</Button></Stack>
      </DialogActions>
    </Dialog>
    <AvatarSelector open={avatarsOpen} selected={avatarId} onClose={() => setAvatarsOpen(false)} onSelect={(avatar) => { setAvatarId(avatar.id); setAvatarUrl(avatar.url); setAvatarsOpen(false); }} />
  </>;
}

function AvatarSelector({ open, selected, onClose, onSelect }: { open: boolean; selected: string; onClose: () => void; onSelect: (avatar: AvatarItem) => void }) {
  const { t } = useTranslation('profiles');
  const [category, setCategory] = useState<AvatarItem['category'] | undefined>();
  const [page, setPage] = useState(1);
  const avatars = useQuery({ queryKey: ['avatars', category, page], queryFn: () => api.avatars.list({ category, page, pageSize: 40 }), enabled: open });
  const tab = useMemo(() => CATEGORIES.findIndex((item) => item.id === category), [category]);
  return <Dialog open={open} onClose={onClose} fullWidth maxWidth="md"><DialogTitle>{t('selectAvatar')}</DialogTitle><DialogContent>
    <Tabs value={Math.max(0, tab)} onChange={(_, value: number) => { setCategory(CATEGORIES[value]?.id); setPage(1); }} variant="scrollable" scrollButtons="auto">{CATEGORIES.map((item) => <Tab key={item.label} label={item.label} />)}</Tabs>
    {avatars.isLoading ? <Box sx={{ display: 'grid', placeItems: 'center', p: 6 }}><CircularProgress /></Box> : <Grid container spacing={2} sx={{ mt: 1 }}>{avatars.data?.items.map((avatar) => <Grid item key={avatar.id} xs={4} sm={3} md={2}><CardActionArea onClick={() => onSelect(avatar)} sx={{ borderRadius: 2, outline: avatar.id === selected ? '3px solid' : 'none', outlineColor: 'primary.main' }}><Box component="img" src={avatar.url} alt="" loading="lazy" sx={{ width: '100%', aspectRatio: '1', objectFit: 'cover', display: 'block', borderRadius: 2 }} /></CardActionArea></Grid>)}</Grid>}
    {(avatars.data?.totalPages ?? 0) > 1 ? <Stack alignItems="center" sx={{ mt: 3 }}><Pagination page={page} count={avatars.data?.totalPages} onChange={(_, value) => setPage(value)} /></Stack> : null}
  </DialogContent><DialogActions><Button onClick={onClose}>{t('close')}</Button></DialogActions></Dialog>;
}
