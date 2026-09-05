import { useEffect, useState, type ReactNode } from 'react';
import { Box, Button, Card, CardActionArea, CardContent, Chip, Divider, List, ListItemButton, ListItemIcon, ListItemText, Stack, TextField, Typography } from '@mui/material';
import DashboardOutlinedIcon from '@mui/icons-material/DashboardOutlined';
import PaletteOutlinedIcon from '@mui/icons-material/PaletteOutlined';
import DnsOutlinedIcon from '@mui/icons-material/DnsOutlined';
import PlayCircleOutlineIcon from '@mui/icons-material/PlayCircleOutline';
import GroupOutlinedIcon from '@mui/icons-material/GroupOutlined';
import ExtensionOutlinedIcon from '@mui/icons-material/ExtensionOutlined';
import InfoOutlinedIcon from '@mui/icons-material/InfoOutlined';
import CheckRoundedIcon from '@mui/icons-material/CheckRounded';
import ConstructionRoundedIcon from '@mui/icons-material/ConstructionRounded';
import { useQuery } from '@tanstack/react-query';
import { api, ApiError } from '@/api';
import { PageHeader, SectionHeader } from '@/components/PageHeader';
import { StatusDot } from '@/components/StatusBadge';
import { useUiStore } from '@/stores/uiStore';
import type { ThemeMode } from '@/theme/theme';
import { useLocation, useNavigate } from 'react-router-dom';
import { useAuth } from '@/app/AuthProvider';
import { UsersSettingsSection } from '@/features/users/components/UsersSettingsSection';

type SectionId = 'general' | 'personalization' | 'server' | 'playback' | 'users' | 'plugins' | 'about';
type Section = { id: SectionId; label: string; description: string; icon: ReactNode };

const sections: Section[] = [
  { id: 'general', label: 'Dados gerais', description: 'Servidor e status', icon: <DashboardOutlinedIcon /> },
  { id: 'personalization', label: 'Personalização', description: 'Tema e aparência', icon: <PaletteOutlinedIcon /> },
  { id: 'server', label: 'Servidor', description: 'Conexão e armazenamento', icon: <DnsOutlinedIcon /> },
  { id: 'playback', label: 'Reprodução', description: 'Qualidade e comportamento', icon: <PlayCircleOutlineIcon /> },
  { id: 'users', label: 'Usuários', description: 'Perfis e permissões', icon: <GroupOutlinedIcon /> },
  { id: 'plugins', label: 'Plugins', description: 'Integrações', icon: <ExtensionOutlinedIcon /> },
  { id: 'about', label: 'Sobre', description: 'Versão e informações', icon: <InfoOutlinedIcon /> },
];

const themeOptions: Array<{ value: ThemeMode; name: string; description: string; colors: string[] }> = [
  { value: 'system', name: 'Sistema', description: 'Acompanha o dispositivo', colors: ['#FBF9F8', '#171A18'] },
  { value: 'light', name: 'Claro', description: 'Neutro e luminoso', colors: ['#FBF9F8', '#75CE67'] },
  { value: 'dark', name: 'Escuro', description: 'Confortável à noite', colors: ['#171A18', '#91E083'] },
  { value: 'ocean', name: 'Oceano', description: 'Azul calmo e nítido', colors: ['#F7FAFC', '#006780'] },
  { value: 'violet', name: 'Ametista', description: 'Violeta sofisticado', colors: ['#FCF8FF', '#77558D'] },
  { value: 'sunset', name: 'Pôr do sol', description: 'Quente e acolhedor', colors: ['#FFF8F4', '#9A4522'] },
  { value: 'rose', name: 'Rosa', description: 'Suave e expressivo', colors: ['#FFF8F9', '#9B405C'] },
  { value: 'midnight', name: 'Meia-noite', description: 'Azul profundo', colors: ['#0E1726', '#AFC6FF'] },
];

export default function SettingsPage() {
  const [active, setActive] = useState<SectionId>('general');
  const location = useLocation(); const navigate = useNavigate(); const { user } = useAuth();
  useEffect(() => { if (location.pathname === '/settings/users') setActive('users'); else if (user && !user.isAdmin) setActive('personalization'); }, [location.pathname, user]);
  const visibleSections = user?.isAdmin ? sections : sections.filter((section) => section.id === 'personalization');
  const current = sections.find((section) => section.id === active) ?? sections[0]!;
  return <Box sx={{ maxWidth: 1180, mx: 'auto' }}>
    <PageHeader title="Configurações" />
    <Typography color="text.secondary" sx={{ mt: -2, mb: 4 }}>Gerencie a experiência e o funcionamento do MyLib.</Typography>
    <Stack direction={{ xs: 'column', md: 'row' }} spacing={{ xs: 2, md: 6 }} alignItems="flex-start">
      <Card sx={{ width: { xs: '100%', md: 280 }, flexShrink: 0, position: { md: 'sticky' }, top: { md: 16 } }}>
        <List disablePadding sx={{ p: 1 }}>{visibleSections.map((section) => <ListItemButton key={section.id} selected={active === section.id} onClick={() => { setActive(section.id); navigate(section.id === 'users' ? '/settings/users' : '/settings'); }} sx={{ borderRadius: 2, mb: 0.5, py: 1.25, '&.Mui-selected': { bgcolor: (theme) => theme.tokens.sidebarActiveBg, color: (theme) => theme.tokens.sidebarActiveText }, '&.Mui-selected:hover': { bgcolor: (theme) => theme.tokens.sidebarActiveBg } }}><ListItemIcon sx={{ minWidth: 38, color: 'inherit' }}>{section.icon}</ListItemIcon><ListItemText primary={section.label} secondary={section.description} primaryTypographyProps={{ fontWeight: 650 }} secondaryTypographyProps={{ fontSize: 12, color: 'inherit', sx: { opacity: 0.72 } }} /></ListItemButton>)}</List>
      </Card>
      <Box sx={{ flex: 1, minWidth: 0, width: '100%' }}>
        <Typography variant="h1" sx={{ fontSize: { xs: 26, md: 32 } }}>{current.label}</Typography><Typography color="text.secondary" sx={{ mt: 0.5, mb: 3 }}>{current.description}</Typography>
        {active === 'general' ? <GeneralSection /> : null}{active === 'personalization' ? <PersonalizationSection /> : null}{active === 'server' ? <ServerSection /> : null}{active === 'playback' ? <PlaybackSection /> : null}{active === 'users' ? <UsersSettingsSection /> : null}{active === 'plugins' ? <ComingSoon title="Plugins e integrações" /> : null}{active === 'about' ? <AboutSection /> : null}
      </Box>
    </Stack>
  </Box>;
}

function GeneralSection() {
  const server = useQuery({ queryKey: ['server'], queryFn: api.server.info, retry: false });
  const health = useQuery({ queryKey: ['server','health'], queryFn: api.server.health, retry: false });
  const data = server.data; const h=health.data;
  const values = [['Nome do servidor', data?.name ?? '—'], ['Versão', data?.version ?? '—'], ['Banco de dados', data?.databaseType ?? '—'], ['Tempo em atividade', data ? formatUptime(data.uptimeSeconds) : '—'],['Sistema operacional',h?.operatingSystem??'—'],['Arquitetura',h?.architecture??'—'],['Data directory',h?.dataDirectory??'—'],['FFmpeg',h?.ffmpegAvailable?'Disponível':'Indisponível'],['FFprobe',h?.ffprobeAvailable?'Disponível':'Indisponível']];
  return <Card><CardContent sx={{ p: { xs: 3, md: 4 } }}><SectionHeader title="Visão geral" /><Stack direction="row" spacing={1} alignItems="center" sx={{ mb: 3 }}><StatusDot tone={h?.status==='HEALTHY'?'success':h?.status==='DEGRADED'?'warning':'error'} /><Typography fontWeight={700}>{h?.status==='HEALTHY'?'Servidor saudável':h?.status==='DEGRADED'?'Servidor com avisos':'Servidor indisponível'}</Typography></Stack>{values.map(([label, value], index) => <Box key={label}>{index > 0 ? <Divider /> : null}<Stack direction={{ xs: 'column', sm: 'row' }} justifyContent="space-between" spacing={0.5} sx={{ py: 2 }}><Typography color="text.secondary">{label}</Typography><Typography fontWeight={650} sx={{wordBreak:'break-all'}}>{value}</Typography></Stack></Box>)}</CardContent></Card>;
}

function ServerSection(){const server=useQuery({queryKey:['server'],queryFn:api.server.info});const health=useQuery({queryKey:['server','health'],queryFn:api.server.health});const [name,setName]=useState('');useEffect(()=>{if(server.data)setName(server.data.name)},[server.data]);return <Stack spacing={2}><Card><CardContent sx={{p:4}}><SectionHeader title="Servidor"/><Stack spacing={2}><TextField label="Nome do servidor" value={name} onChange={e=>setName(e.target.value)}/><Button variant="contained" sx={{alignSelf:'flex-start'}} disabled={!name.trim()||name===server.data?.name} onClick={()=>void api.server.update(name).then(()=>server.refetch())}>Salvar nome</Button></Stack></CardContent></Card><Card><CardContent sx={{p:4}}><SectionHeader title="Conexão e armazenamento"/><ReadOnlyRows rows={[["Host",health.data?.host??'—'],["Porta",String(health.data?.port??'—')],["Timezone",Intl.DateTimeFormat().resolvedOptions().timeZone],["Data directory",health.data?.dataDirectory??'—'],["Database type",health.data?.databaseType??'—']]}/><Typography variant="body2" color="text.secondary" sx={{mt:2}}>Campos críticos são somente leitura porque alterações em runtime não são seguras.</Typography></CardContent></Card><TmdbSection/></Stack>}

// TMDB is optional: the setup wizard can leave it unset, and MYLIB_TMDB_API_KEY (when present)
// always wins over whatever is saved here, so this card is read-only in that case.
function TmdbSection(){const tmdb=useQuery({queryKey:['settings','tmdb'],queryFn:api.settings.tmdbStatus});const [apiKey,setApiKey]=useState('');const [error,setError]=useState<string|null>(null);const [saving,setSaving]=useState(false);const save=(value:string|null)=>{setSaving(true);setError(null);api.settings.updateTmdbKey(value).then(()=>{setApiKey('');return tmdb.refetch();}).catch(err=>setError(err instanceof ApiError?err.localizedMessage:'Erro inesperado.')).finally(()=>setSaving(false));};return <Card><CardContent sx={{p:4}}><SectionHeader title="Metadados (TMDB)"/><Stack direction="row" spacing={1} alignItems="center" sx={{mb:2}}><StatusDot tone={tmdb.data?.configured?'success':'warning'}/><Typography fontWeight={700}>{tmdb.data?.configured?'Chave configurada':'Chave não configurada'}</Typography></Stack><Stack direction={{xs:'column',sm:'row'}} spacing={2}><TextField label="Chave de API do TMDB" placeholder="Cole sua chave de API v3" value={apiKey} onChange={e=>setApiKey(e.target.value)} fullWidth/><Button variant="contained" sx={{alignSelf:{xs:'flex-start',sm:'center'},flexShrink:0}} disabled={!apiKey.trim()||saving} onClick={()=>save(apiKey.trim())}>Salvar chave</Button></Stack>{tmdb.data?.configured?<Button color="error" size="small" sx={{mt:1.5}} disabled={saving} onClick={()=>save(null)}>Remover chave</Button>:null}{error?<Typography color="error" variant="body2" sx={{mt:1.5}}>{error}</Typography>:null}</CardContent></Card>;}

function PlaybackSection(){const caps=useQuery({queryKey:['playback','capabilities'],queryFn:api.playback.capabilities});const metrics=useQuery({queryKey:['server','metrics'],queryFn:api.server.metrics});const hardware=caps.data?.hardwareAcceleration[0]?.replace('QUICK_SYNC','Intel Quick Sync')??'CPU';return <Stack spacing={2}><Card><CardContent sx={{p:4}}><SectionHeader title="Motor de mídia"/><ReadOnlyRows rows={[["FFmpeg",caps.data?.ffmpegAvailable?'Disponível':'Indisponível'],["FFprobe",caps.data?.ffprobeAvailable?'Disponível':'Indisponível'],["Aceleração de hardware",hardware]]}/></CardContent></Card><Card><CardContent sx={{p:4}}><SectionHeader title="Qualidade e comportamento"/><ReadOnlyRows rows={[["Qualidade automática","Ativada"],["Aceleração de hardware",hardware],["Máximo de transcodes",String(caps.data?.maxConcurrentTranscodes??'—')],["Máximo da fila",String(metrics.data?.transcodeQueueLimit??'—')],["Diretório de cache",caps.data?.ffmpegPath?caps.data.ffmpegPath.replace(/tools[\\/].*$/,'data/cache/transcode'):'—'],["Limite de cache",metrics.data?`${Math.round(metrics.data.transcodeCacheSizeBytes/1024/1024)} MB em uso`:'—'],["Auto próximo episódio","Ativado"]]}/><Typography variant="body2" color="text.secondary" sx={{mt:2}}>Os limites usam a configuração existente do servidor e exigem reinício para alteração.</Typography></CardContent></Card></Stack>}

function PersonalizationSection() {
  const mode = useUiStore((state) => state.themeMode); const setMode = useUiStore((state) => state.setThemeMode);
  return <Box><SectionHeader title="Escolha um tema" /><Typography color="text.secondary" sx={{ mt: -1, mb: 3 }}>Cada opção atualiza cores, superfícies, contraste e estados em toda a interface.</Typography><Box role="radiogroup" aria-label="Tema" sx={{ display: 'grid', gridTemplateColumns: { xs: '1fr', sm: 'repeat(2, minmax(0, 1fr))' }, gap: 2 }}>{themeOptions.map((option) => { const selected = mode === option.value; return <Card key={option.value} sx={{ borderWidth: selected ? 2 : 1, borderColor: selected ? 'primary.main' : 'divider' }}><CardActionArea role="radio" aria-checked={selected} onClick={() => setMode(option.value)} sx={{ p: 2.5 }}><Stack direction="row" justifyContent="space-between" alignItems="center"><Stack direction="row" spacing={1.5} alignItems="center"><Stack direction="row" sx={{ borderRadius: '50%', overflow: 'hidden', border: '1px solid rgba(0,0,0,.12)' }}>{option.colors.map((color) => <Box key={color} sx={{ width: 18, height: 36, bgcolor: color }} />)}</Stack><Box><Typography fontWeight={700}>{option.name}</Typography><Typography variant="body2" color="text.secondary">{option.description}</Typography></Box></Stack>{selected ? <CheckRoundedIcon color="primary" /> : null}</Stack></CardActionArea></Card>; })}</Box></Box>;
}

function ComingSoon({ title }: { title: string }) { return <Card><CardContent sx={{ minHeight: 260, display: 'flex', alignItems: 'center', justifyContent: 'center', textAlign: 'center', p: 4 }}><Box><ConstructionRoundedIcon color="primary" sx={{ fontSize: 48 }} /><Typography variant="h2" sx={{ mt: 2 }}>{title}</Typography><Chip label="Em construção" variant="outlined" sx={{ mt: 2 }} /></Box></CardContent></Card>; }
function AboutSection() { const health=useQuery({queryKey:['server','health'],queryFn:api.server.health});const caps=useQuery({queryKey:['playback','capabilities'],queryFn:api.playback.capabilities});return <Card><CardContent sx={{ p: { xs: 3, md: 4 } }}><Typography variant="h2">MyLib</Typography><Typography color="text.secondary" sx={{ mt: 1, lineHeight: 1.7 }}>Sua biblioteca pessoal de filmes e séries, organizada em um servidor simples, privado e agradável de usar.</Typography><Divider sx={{ my: 3 }} /><ReadOnlyRows rows={[["Versão",health.data?.version??'—'],["Build",import.meta.env.MODE],["FFmpeg",caps.data?.ffmpegAvailable?'Disponível':'Indisponível'],["FFprobe",caps.data?.ffprobeAvailable?'Disponível':'Indisponível'],["Database provider",health.data?.databaseType??'—'],["Licença","MIT"]]}/></CardContent></Card>; }
function ReadOnlyRows({rows}:{rows:Array<[string,string]>}){return <Stack divider={<Divider/>}>{rows.map(([label,value])=><Stack key={label} direction={{xs:'column',sm:'row'}} justifyContent="space-between" spacing={1} sx={{py:1.75}}><Typography color="text.secondary">{label}</Typography><Typography fontWeight={650} sx={{wordBreak:'break-all'}}>{value}</Typography></Stack>)}</Stack>}
function formatUptime(seconds: number) { const days = Math.floor(seconds / 86400); const hours = Math.floor((seconds % 86400) / 3600); const minutes = Math.floor((seconds % 3600) / 60); return [days ? `${days}d` : '', hours ? `${hours}h` : '', `${minutes}min`].filter(Boolean).join(' '); }
