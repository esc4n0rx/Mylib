import { useEffect, useMemo, useState } from 'react';
import { Avatar, Box, Button, Card, CardActionArea, CardContent, Checkbox, Chip, Dialog, DialogActions, DialogContent, DialogTitle, IconButton, MenuItem, Pagination, Stack, Tab, Tabs, TextField, Tooltip, Typography } from '@mui/material';
import AddRoundedIcon from '@mui/icons-material/AddRounded';
import EditOutlinedIcon from '@mui/icons-material/EditOutlined';
import SearchRoundedIcon from '@mui/icons-material/SearchRounded';
import LockResetRoundedIcon from '@mui/icons-material/LockResetRounded';
import PersonOffOutlinedIcon from '@mui/icons-material/PersonOffOutlined';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { api, type Library, type ManagedUser, type Profile } from '@/api';
import { useAuth } from '@/app/AuthProvider';
import { useToast } from '@/app/ToastProvider';
import { ConfirmationDialog } from '@/components/ConfirmationDialog';
import { PasswordField } from '@/components/PasswordField';
import { EmptyState } from '@/components/states/EmptyState';
import { ProfileEditor } from '@/features/profiles/pages/ProfilesPage';

const emptyForm = { username: '', displayName: '', email: '', password: '', confirmPassword: '' };

export function UsersSettingsSection() {
  const { user: currentUser } = useAuth();
  const toast = useToast();
  const client = useQueryClient();
  const [searchInput, setSearchInput] = useState('');
  const [search, setSearch] = useState('');
  const [status, setStatus] = useState('all');
  const [role, setRole] = useState('all');
  const [page, setPage] = useState(1);
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<ManagedUser>();
  const [confirmUser, setConfirmUser] = useState<ManagedUser>();

  useEffect(() => { const timer = window.setTimeout(() => { setSearch(searchInput); setPage(1); }, 400); return () => window.clearTimeout(timer); }, [searchInput]);
  const users = useQuery({ queryKey: ['users', { search, status, role, page }], queryFn: () => api.users.list({ search: search || undefined, status, role, page, pageSize: 12 }), enabled: Boolean(currentUser?.isAdmin) });
  const toggle = useMutation({ mutationFn: async (target: ManagedUser) => target.isActive ? api.users.disable(target.id) : api.users.enable(target.id), onSuccess: (_value, target) => { toast.notify(target.isActive ? 'Usuário desativado' : 'Usuário ativado', 'success'); void client.invalidateQueries({ queryKey: ['users'] }); setConfirmUser(undefined); } });

  if (!currentUser?.isAdmin) return <EmptyState title="Acesso restrito" body="Somente administradores podem gerenciar usuários." />;
  const items = users.data?.items ?? [];
  return <Box>
    <Stack direction={{ xs: 'column', sm: 'row' }} justifyContent="space-between" spacing={2} sx={{ mb: 3 }}>
      <Box><Typography variant="h2">Usuários</Typography><Typography color="text.secondary">Gerencie quem pode acessar o servidor MyLib.</Typography></Box>
      <Button variant="contained" startIcon={<AddRoundedIcon />} onClick={() => setCreating(true)}>Adicionar usuário</Button>
    </Stack>
    <Stack direction={{ xs: 'column', md: 'row' }} spacing={2} sx={{ mb: 3 }}>
      <TextField value={searchInput} onChange={(event) => setSearchInput(event.target.value)} placeholder="Pesquisar usuário" InputProps={{ startAdornment: <SearchRoundedIcon color="action" sx={{ mr: 1 }} /> }} sx={{ flex: 1 }} />
      <TextField select label="Status" value={status} onChange={(event) => { setStatus(event.target.value); setPage(1); }} sx={{ minWidth: 150 }}><MenuItem value="all">Todos</MenuItem><MenuItem value="active">Ativos</MenuItem><MenuItem value="disabled">Desativados</MenuItem></TextField>
      <TextField select label="Perfil" value={role} onChange={(event) => { setRole(event.target.value); setPage(1); }} sx={{ minWidth: 170 }}><MenuItem value="all">Todos</MenuItem><MenuItem value="Administrator">Administradores</MenuItem><MenuItem value="User">Usuários</MenuItem></TextField>
    </Stack>
    {items.length ? <Stack spacing={1.5}>{items.map((managed) => <UserRow key={managed.id} user={managed} onEdit={() => setEditing(managed)} onToggle={() => managed.isActive ? setConfirmUser(managed) : toggle.mutate(managed)} />)}</Stack> : <Card><CardContent><EmptyState title="Nenhum usuário adicional" body="Crie usuários para compartilhar seu servidor MyLib com outras pessoas." action={{ label: 'Adicionar usuário', onClick: () => setCreating(true) }} /></CardContent></Card>}
    {(users.data?.totalPages ?? 0) > 1 ? <Pagination page={page} count={users.data?.totalPages ?? 1} onChange={(_event, value) => setPage(value)} sx={{ mt: 3, display: 'flex', justifyContent: 'center' }} /> : null}
    <CreateUserDialog open={creating} onClose={() => setCreating(false)} onCreated={() => { setCreating(false); void client.invalidateQueries({ queryKey: ['users'] }); }} />
    <EditUserDialog user={editing} onClose={() => setEditing(undefined)} onUpdated={() => { void client.invalidateQueries({ queryKey: ['users'] }); }} />
    <ConfirmationDialog open={Boolean(confirmUser)} title="Desativar usuário" body={`O usuário ${confirmUser?.displayName ?? ''} não poderá entrar até ser ativado novamente. O histórico e os acessos serão preservados.`} confirmLabel="Desativar" destructive loading={toggle.isPending} onClose={() => setConfirmUser(undefined)} onConfirm={() => confirmUser && toggle.mutate(confirmUser)} />
  </Box>;
}

function UserRow({ user, onEdit, onToggle }: { user: ManagedUser; onEdit: () => void; onToggle: () => void }) {
  return <Card><CardContent sx={{ p: 2, '&:last-child': { pb: 2 } }}><Stack direction="row" spacing={2} alignItems="center"><Avatar sx={{ bgcolor: 'primary.main', color: 'primary.contrastText' }}>{user.displayName.slice(0, 1).toUpperCase()}</Avatar><Box sx={{ minWidth: 0, flex: 1 }}><Stack direction="row" spacing={1} alignItems="center" flexWrap="wrap" useFlexGap><Typography fontWeight={700} noWrap>{user.displayName}</Typography><Chip size="small" label={user.isActive ? 'Ativo' : 'Desativado'} color={user.isActive ? 'success' : 'default'} variant="outlined" />{user.isAdmin ? <Chip size="small" label="Administrador" color="primary" /> : null}</Stack><Typography variant="body2" color="text.secondary" noWrap>@{user.username}{user.email ? ` · ${user.email}` : ''}</Typography><Typography variant="caption" color="text.secondary">{user.isAdmin ? 'Todas as bibliotecas' : `${user.libraryAccessCount} biblioteca(s)`} · Último acesso: {user.lastLoginAt ? new Date(user.lastLoginAt).toLocaleString('pt-BR') : 'Nunca'}</Typography></Box><Tooltip title="Editar"><IconButton onClick={onEdit}><EditOutlinedIcon /></IconButton></Tooltip>{!user.isAdmin ? <Tooltip title={user.isActive ? 'Desativar' : 'Ativar'}><IconButton onClick={onToggle} color={user.isActive ? 'default' : 'primary'}><PersonOffOutlinedIcon /></IconButton></Tooltip> : null}</Stack></CardContent></Card>;
}

function LibrarySelector({ libraries, selected, onChange, disabled }: { libraries: Library[]; selected: Set<string>; onChange: (value: Set<string>) => void; disabled?: boolean }) {
  return <Stack spacing={1}>{libraries.map((library) => { const checked = selected.has(library.id); return <Card key={library.id} variant="outlined"><Stack direction="row" alignItems="center" spacing={1} sx={{ p: 1.5 }}><Checkbox checked={checked} disabled={disabled} onChange={() => { const next = new Set(selected); if (checked) next.delete(library.id); else next.add(library.id); onChange(next); }} /><Box sx={{ flex: 1 }}><Typography fontWeight={650}>{library.name}</Typography><Typography variant="caption" color="text.secondary">{library.type === 'MOVIE' ? 'Filmes' : 'Séries'} · {library.privacy === 'PUBLIC' ? 'Pública' : 'Privada'}</Typography></Box></Stack></Card>; })}</Stack>;
}

function CreateUserDialog({ open, onClose, onCreated }: { open: boolean; onClose: () => void; onCreated: () => void }) {
  const toast = useToast(); const [form, setForm] = useState(emptyForm); const [selected, setSelected] = useState<Set<string>>(new Set()); const libraries = useQuery({ queryKey: ['libraries'], queryFn: api.libraries.list, enabled: open });
  const create = useMutation({ mutationFn: () => api.users.create({ username: form.username, displayName: form.displayName, email: form.email || undefined, password: form.password, libraryAccess: [...selected].map((libraryId) => ({ libraryId, canView: true, canPlay: true })) }), onSuccess: () => { toast.notify('Usuário criado', 'success'); setForm(emptyForm); setSelected(new Set()); onCreated(); } });
  const error = form.password !== form.confirmPassword ? 'As senhas não coincidem.' : form.password.length < 10 ? 'A senha deve ter pelo menos 10 caracteres.' : '';
  return <Dialog open={open} onClose={onClose} maxWidth="sm" fullWidth><DialogTitle>Adicionar usuário</DialogTitle><DialogContent><Stack spacing={2} sx={{ mt: 1 }}><TextField label="Nome de usuário" value={form.username} onChange={(e) => setForm({ ...form, username: e.target.value })} /><TextField label="Nome de exibição" value={form.displayName} onChange={(e) => setForm({ ...form, displayName: e.target.value })} /><TextField label="E-mail" type="email" value={form.email} onChange={(e) => setForm({ ...form, email: e.target.value })} /><PasswordField label="Senha inicial" value={form.password} onChange={(e) => setForm({ ...form, password: e.target.value })} /><PasswordField label="Confirmar senha" value={form.confirmPassword} error={Boolean(form.confirmPassword && error)} helperText={form.confirmPassword ? error : ''} onChange={(e) => setForm({ ...form, confirmPassword: e.target.value })} /><Typography variant="h3" sx={{ pt: 1 }}>Acesso às bibliotecas</Typography><LibrarySelector libraries={libraries.data?.items ?? []} selected={selected} onChange={setSelected} />{create.isError ? <Typography color="error">Não foi possível criar o usuário. Verifique os dados informados.</Typography> : null}</Stack></DialogContent><DialogActions><Button onClick={onClose}>Cancelar</Button><Button variant="contained" disabled={!form.username || !form.displayName || Boolean(error) || create.isPending} onClick={() => create.mutate()}>Criar usuário</Button></DialogActions></Dialog>;
}

function EditUserDialog({ user, onClose, onUpdated }: { user?: ManagedUser; onClose: () => void; onUpdated: () => void }) {
  const toast = useToast(); const [tab, setTab] = useState(0); const [general, setGeneral] = useState({ username: '', displayName: '', email: '' }); const [selected, setSelected] = useState<Set<string>>(new Set()); const [password, setPassword] = useState(''); const [confirmAction, setConfirmAction] = useState<'access' | 'password'>();
  const libraries = useQuery({ queryKey: ['libraries'], queryFn: api.libraries.list, enabled: Boolean(user) }); const access = useQuery({ queryKey: ['users', user?.id, 'access'], queryFn: () => api.users.libraryAccess(user!.id), enabled: Boolean(user && !user.isAdmin) });
  useEffect(() => { if (user) setGeneral({ username: user.username, displayName: user.displayName, email: user.email ?? '' }); }, [user]);
  useEffect(() => { setSelected(new Set(access.data?.libraries.filter((item) => item.canView).map((item) => item.libraryId) ?? [])); }, [access.data]);
  const update = useMutation({ mutationFn: () => api.users.update(user!.id, general), onSuccess: () => { toast.notify('Usuário atualizado', 'success'); onUpdated(); } });
  const saveAccess = useMutation({ mutationFn: () => api.users.updateLibraryAccess(user!.id, [...selected].map((libraryId) => ({ libraryId, canView: true, canPlay: true }))), onSuccess: () => { toast.notify('Acesso atualizado', 'success'); setConfirmAction(undefined); void access.refetch(); onUpdated(); } });
  const reset = useMutation({ mutationFn: () => api.users.resetPassword(user!.id, password), onSuccess: () => { toast.notify('Senha redefinida', 'success'); setPassword(''); setConfirmAction(undefined); } });
  const removed = useMemo(() => (access.data?.libraries.filter((item) => item.canView).length ?? 0) > selected.size, [access.data, selected]);
  return <><Dialog open={Boolean(user)} onClose={onClose} maxWidth="sm" fullWidth><DialogTitle>Editar usuário</DialogTitle><Tabs value={tab} onChange={(_e, value: number) => setTab(value)} variant="fullWidth"><Tab label="Geral" /><Tab label="Acessos" /><Tab label="Perfis" /><Tab label="Segurança" /></Tabs><DialogContent sx={{ minHeight: 320 }}>{tab === 0 ? <Stack spacing={2} sx={{ mt: 1 }}><TextField label="Nome de usuário" value={general.username} onChange={(e) => setGeneral({ ...general, username: e.target.value })} /><TextField label="Nome de exibição" value={general.displayName} onChange={(e) => setGeneral({ ...general, displayName: e.target.value })} /><TextField label="E-mail" value={general.email} onChange={(e) => setGeneral({ ...general, email: e.target.value })} /><Button variant="contained" onClick={() => update.mutate()} disabled={update.isPending}>Salvar alterações</Button></Stack> : null}{tab === 1 ? user?.isAdmin ? <EmptyState title="Acesso total" body="Administradores podem acessar todas as bibliotecas." /> : <Stack spacing={2}><LibrarySelector libraries={libraries.data?.items ?? []} selected={selected} onChange={setSelected} /><Button variant="contained" onClick={() => removed ? setConfirmAction('access') : saveAccess.mutate()} disabled={saveAccess.isPending}>Salvar acessos</Button></Stack> : null}{tab === 2 && user ? <UserProfilesTab user={user} /> : null}{tab === 3 ? <Stack spacing={2} sx={{ mt: 1 }}><LockResetRoundedIcon color="primary" /><Typography variant="h3">Redefinir senha</Typography><PasswordField label="Nova senha" value={password} onChange={(e) => setPassword(e.target.value)} helperText="Mínimo de 10 caracteres" /><Button variant="contained" startIcon={<LockResetRoundedIcon />} disabled={password.length < 10} onClick={() => setConfirmAction('password')}>Redefinir senha</Button></Stack> : null}</DialogContent><DialogActions><Button onClick={onClose}>Fechar</Button></DialogActions></Dialog><ConfirmationDialog open={confirmAction === 'access'} title="Remover acesso" body="O conteúdo das bibliotecas removidas deixará de aparecer para este usuário. Favoritos e progresso serão preservados." confirmLabel="Atualizar acessos" onClose={() => setConfirmAction(undefined)} onConfirm={() => saveAccess.mutate()} loading={saveAccess.isPending} /><ConfirmationDialog open={confirmAction === 'password'} title="Redefinir senha" body="A senha atual será substituída imediatamente." confirmLabel="Redefinir" onClose={() => setConfirmAction(undefined)} onConfirm={() => reset.mutate()} loading={reset.isPending} /></>;
}

function UserProfilesTab({ user }: { user: ManagedUser }) {
  const client = useQueryClient();
  const [editor, setEditor] = useState<Profile | null | undefined>();
  const profiles = useQuery({ queryKey: ['profiles', 'user', user.id], queryFn: () => api.profiles.list(user.id) });
  const saved = () => { setEditor(undefined); void client.invalidateQueries({ queryKey: ['profiles', 'user', user.id] }); };
  return <Stack spacing={2} sx={{ mt: 1 }}><Stack direction="row" justifyContent="space-between" alignItems="center"><Box><Typography variant="h3">Perfis</Typography><Typography variant="body2" color="text.secondary">Avatar, controle parental, PIN e bibliotecas.</Typography></Box><Button size="small" startIcon={<AddRoundedIcon />} onClick={() => setEditor(null)}>Adicionar</Button></Stack>{profiles.data?.items.map((profile) => <Card key={profile.id} variant="outlined"><CardActionArea onClick={() => setEditor(profile)}><Stack direction="row" spacing={1.5} alignItems="center" sx={{ p: 1.5 }}><Avatar src={profile.avatarUrl}>{profile.name[0]}</Avatar><Box sx={{ flex: 1 }}><Typography fontWeight={700}>{profile.name}</Typography><Typography variant="caption" color="text.secondary">{profile.isKids ? `Infantil · ${profile.maxAgeRating}+` : `Adulto · ${profile.maxAgeRating}+`}{profile.pinProtected ? ' · PIN' : ''}</Typography></Box><EditOutlinedIcon color="action" /></Stack></CardActionArea></Card>)}<ProfileEditor profile={editor} userId={user.id} open={editor !== undefined} onClose={() => setEditor(undefined)} onSaved={saved} /></Stack>;
}
