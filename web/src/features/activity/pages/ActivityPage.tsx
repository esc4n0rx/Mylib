import { useState } from 'react';
import { Alert, Avatar, Box, Button, Card, CardContent, Chip, Divider, LinearProgress, Stack, Tab, Table, TableBody, TableCell, TableContainer, TableHead, TableRow, Tabs, Typography } from '@mui/material';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { Navigate } from 'react-router-dom';
import { api, type MetricPoint, type PlaybackSession, type ServerAlert } from '@/api';
import { useAuth } from '@/app/AuthProvider';
import { PageHeader, SectionHeader } from '@/components/PageHeader';
import { ConfirmationDialog } from '@/components/ConfirmationDialog';
import { StatusBadge } from '@/components/StatusBadge';
import { formatBytes, formatDateTime, formatNumber, formatPercent, formatRelativeDuration } from '@/utils/format';

type TabId = 'overview'|'playback'|'transcoding'|'jobs'|'storage';

export default function ActivityPage() {
  const { user } = useAuth();
  const [tab, setTab] = useState<TabId>('overview');
  const metrics = useQuery({ queryKey:['server','metrics'], queryFn:api.server.metrics, refetchInterval:4_000 });
  const alerts = useQuery({ queryKey:['server','alerts'], queryFn:api.server.alerts, refetchInterval:20_000 });
  if (user && !user.isAdmin) return <Navigate to="/home" replace />;
  return <Box sx={{maxWidth:1280,mx:'auto'}}>
    <PageHeader title="Atividade" description="Saúde, recursos e operações do servidor em tempo real." />
    <ServerAlerts items={alerts.data?.items ?? []} />
    <Card sx={{mb:3}}><Tabs value={tab} onChange={(_,v)=>setTab(v)} variant="scrollable" allowScrollButtonsMobile>
      <Tab value="overview" label="Visão geral"/><Tab value="playback" label="Reproduções"/><Tab value="transcoding" label="Transcoding"/><Tab value="jobs" label="Jobs"/><Tab value="storage" label="Armazenamento"/>
    </Tabs></Card>
    {tab==='overview' ? <Overview metrics={metrics.data} /> : null}
    {tab==='playback' ? <PlaybackTab /> : null}
    {tab==='transcoding' ? <TranscodingTab /> : null}
    {tab==='jobs' ? <JobsTab /> : null}
    {tab==='storage' ? <StorageTab /> : null}
  </Box>;
}

function ServerAlerts({items}:{items:ServerAlert[]}) { if(!items.length)return null; return <Stack spacing={1} sx={{mb:3}}>{items.map(item=><Alert key={item.id} severity={item.severity==='CRITICAL'?'error':item.severity==='WARNING'?'warning':'info'}><strong>{item.title}</strong> — {item.message}</Alert>)}</Stack> }

function Overview({metrics}:{metrics?:Awaited<ReturnType<typeof api.server.metrics>>}) {
  const activity=useQuery({queryKey:['activity',{pageSize:8}],queryFn:()=>api.activity.list({pageSize:8}),refetchInterval:15_000});
  const cards:Array<[string,string]>=[['CPU',metrics?formatPercent(metrics.cpuUsagePercent):'—'],['Memória',metrics?`${formatBytes(metrics.memoryUsedBytes)} / ${formatBytes(metrics.memoryTotalBytes)}`:'—'],['Streams ativos',formatNumber(metrics?.activePlaybackSessions??0)],['Transcodes',`${metrics?.activeTranscodes??0} / ${metrics?.transcodeLimit??0}`],['Jobs',formatNumber((metrics?.activeScanJobs??0)+(metrics?.queuedJobs??0))],['Fila',`${metrics?.queuedTranscodes??0} / ${metrics?.transcodeQueueLimit??0}`]];
  return <Stack spacing={3}>
    <Box sx={{display:'grid',gridTemplateColumns:{xs:'repeat(2,1fr)',lg:'repeat(6,1fr)'},gap:2}}>{cards.map(([label,value])=><ServerMetricCard key={label} label={label} value={value}/>)}</Box>
    <Box sx={{display:'grid',gridTemplateColumns:{xs:'1fr',md:'repeat(3,1fr)'},gap:2}}>
      <MiniMetricChart title="CPU" points={metrics?.history??[]} field="cpuUsagePercent" suffix="%" />
      <MiniMetricChart title="Memória" points={metrics?.history??[]} field="memoryUsagePercent" suffix="%" />
      <MiniMetricChart title="Streams ativos" points={metrics?.history??[]} field="activePlaybackSessions" />
    </Box>
    <Card><CardContent><SectionHeader title="Atividade recente"/>{activity.data?.items.length ? <Stack divider={<Divider/>}>{activity.data.items.map(item=><Box key={item.id} sx={{py:1.5}}><Typography fontWeight={700}>{item.title}</Typography><Typography variant="body2" color="text.secondary">{item.message} · {formatDateTime(item.createdAt)}</Typography></Box>)}</Stack>:<Typography color="text.secondary">Nenhuma atividade registrada.</Typography>}</CardContent></Card>
  </Stack>;
}

export function ServerMetricCard({label,value}:{label:string;value:string}) { return <Card><CardContent sx={{p:2.5,'&:last-child':{pb:2.5}}}><Typography variant="overline" color="text.secondary">{label}</Typography><Typography variant="h2" sx={{mt:.5}}>{value}</Typography></CardContent></Card> }

export function MiniMetricChart({title,points,field,suffix}:{title:string;points:MetricPoint[];field:keyof MetricPoint;suffix?:string}) {
  const values=points.map(p=>Number(p[field])).filter(Number.isFinite); const max=Math.max(...values,1); const poly=values.map((v,i)=>`${values.length<2?0:i/(values.length-1)*100},${38-(v/max)*34}`).join(' '); const last=values.at(-1)??0;
  return <Card><CardContent><Stack direction="row" justifyContent="space-between"><Typography fontWeight={700}>{title}</Typography><Typography color="text.secondary">{last.toFixed(field==='activePlaybackSessions'?0:1)}{suffix}</Typography></Stack><Box component="svg" viewBox="0 0 100 40" preserveAspectRatio="none" sx={{width:'100%',height:90,mt:2,color:'primary.main'}}><polyline points={poly} fill="none" stroke="currentColor" strokeWidth="2" vectorEffect="non-scaling-stroke"/></Box></CardContent></Card>
}

function PlaybackTab(){
  const [mode,setMode]=useState<'ALL'|'DIRECT_PLAY'|'DIRECT_STREAM'|'TRANSCODE'>('ALL'); const [target,setTarget]=useState<PlaybackSession>(); const qc=useQueryClient();
  const sessions=useQuery({queryKey:['playback','sessions'],queryFn:api.playback.sessions,refetchInterval:4_000});
  const stop=useMutation({mutationFn:(id:string)=>api.playback.stop(id),onSuccess:async()=>{setTarget(undefined);await qc.invalidateQueries({queryKey:['playback','sessions']});}});
  const items=(sessions.data?.items??[]).filter(s=>mode==='ALL'||s.playbackMode===mode);
  return <><Stack direction="row" spacing={1} sx={{mb:2,overflowX:'auto'}}>{[['ALL','Todos'],['DIRECT_PLAY','Direct Play'],['DIRECT_STREAM','Direct Stream'],['TRANSCODE','Transcode']].map(([v,l])=><Chip key={v} label={l} clickable color={mode===v?'primary':'default'} onClick={()=>setMode(v as typeof mode)}/>)}</Stack><Stack spacing={2}>{items.length?items.map(s=><ActiveSessionCard key={s.sessionId} session={s} onStop={()=>setTarget(s)}/>):<Card><CardContent><Typography color="text.secondary">Nenhuma reprodução ativa.</Typography></CardContent></Card>}</Stack><ConfirmationDialog open={!!target} title="Encerrar reprodução?" body={`A reprodução de ${target?.media.title??'mídia'} será encerrada imediatamente.`} confirmLabel="Encerrar reprodução" destructive loading={stop.isPending} onClose={()=>setTarget(undefined)} onConfirm={()=>target&&stop.mutate(target.sessionId)}/></>;
}

export function ActiveSessionCard({session,onStop}:{session:PlaybackSession;onStop:()=>void}) { const episode=session.media.seasonNumber?`T${String(session.media.seasonNumber).padStart(2,'0')} E${String(session.media.episodeNumber??0).padStart(2,'0')}`:undefined; return <Card><CardContent><Stack direction={{xs:'column',sm:'row'}} spacing={2} alignItems={{sm:'center'}}><Avatar>{session.user.displayName.slice(0,1).toUpperCase()}</Avatar><Box sx={{flex:1,minWidth:0}}><Typography variant="h3">{session.media.title}</Typography><Typography color="text.secondary">{episode??(session.media.mediaType==='MOVIE'?'Filme':'Série')} · {session.quality}{session.bitrate?` · ${formatBytes(session.bitrate)}/s`:''}</Typography><Typography variant="body2" color="text.secondary" sx={{mt:.75}}>{duration(session.position)} / {duration(session.duration)} · {session.clientName??'Cliente desconhecido'} · {session.ipAddress??'IP não informado'}</Typography></Box><Stack alignItems={{xs:'flex-start',sm:'flex-end'}} spacing={1}><StatusBadge label={modeLabel(session.playbackMode)} tone={session.playbackMode==='TRANSCODE'?'warning':'success'}/><Button size="small" color="error" onClick={onStop}>Encerrar reprodução</Button></Stack></Stack></CardContent></Card> }

function TranscodingTab(){const q=useQuery({queryKey:['playback','transcodes'],queryFn:api.playback.transcodes,refetchInterval:4_000});return <Stack spacing={2}><Card><CardContent><Stack direction="row" spacing={3}><Typography><strong>{q.data?.active??0}</strong> ativos</Typography><Typography><strong>{q.data?.queued??0}</strong> na fila</Typography><Typography>Limite <strong>{q.data?.limit??0}</strong></Typography></Stack></CardContent></Card>{q.data?.items.length?q.data.items.map(p=><Card key={p.pipelineId}><CardContent><Stack direction={{xs:'column',sm:'row'}} justifyContent="space-between" spacing={2}><Box><Typography variant="h3">{p.media}</Typography><Typography color="text.secondary">{p.sourceCodec.toUpperCase()} {p.sourceResolution} → {p.targetCodec.toUpperCase()} {p.targetResolution}</Typography><Typography variant="body2" sx={{mt:1}}>{p.hardwareAccelerator} · {p.bitrate?`${formatBytes(p.bitrate)}/s · `:''}{p.activeViewers} {p.activeViewers===1?'espectador':'espectadores'}</Typography></Box><StatusBadge label={p.status} tone={p.status==='ERROR'?'error':'success'}/></Stack></CardContent></Card>):<Card><CardContent><Typography color="text.secondary">Nenhum pipeline ativo.</Typography></CardContent></Card>}</Stack>}

function JobsTab(){const q=useQuery({queryKey:['jobs'],queryFn:()=>api.jobs.list({pageSize:50}),refetchInterval:5_000});return <TableContainer component={Card}><Table><TableHead><TableRow><TableCell>Tipo</TableCell><TableCell>Status</TableCell><TableCell>Biblioteca</TableCell><TableCell>Progresso</TableCell><TableCell>Origem</TableCell><TableCell>Início</TableCell><TableCell>Duração</TableCell></TableRow></TableHead><TableBody>{q.data?.items.map(j=><TableRow key={j.id}><TableCell>{jobType(j.type)}</TableCell><TableCell><StatusBadge label={jobStatus(j.status)} tone={j.status==='FAILED'?'error':j.status==='RUNNING'?'info':'success'}/></TableCell><TableCell>{j.library.name}</TableCell><TableCell sx={{minWidth:150}}><LinearProgress variant="determinate" value={Math.min(100,j.progress)}/><Typography variant="caption">{Math.round(j.progress)}%</Typography></TableCell><TableCell>{j.source}</TableCell><TableCell>{formatDateTime(j.startedAt)}</TableCell><TableCell>{j.duration!=null?formatRelativeDuration(j.duration):'—'}</TableCell></TableRow>)}</TableBody></Table></TableContainer>}

function StorageTab(){const q=useQuery({queryKey:['server','storage'],queryFn:api.server.storage,refetchInterval:15_000});const s=q.data;if(!s)return <Typography color="text.secondary">Carregando armazenamento…</Typography>;return <Stack spacing={3}><Box sx={{display:'grid',gridTemplateColumns:{xs:'repeat(2,1fr)',md:'repeat(4,1fr)'},gap:2}}><ServerMetricCard label="Mídia" value={formatBytes(s.libraryStorage.reduce((a,b)=>a+b.sizeBytes,0))}/><ServerMetricCard label="Cache" value={`${formatBytes(s.transcodeCache.sizeBytes)} / ${formatBytes(s.transcodeCache.maxBytes)}`}/><ServerMetricCard label="Banco" value={formatBytes(s.database.sizeBytes)}/><ServerMetricCard label="Logs" value={formatBytes(s.logs.sizeBytes)}/></Box><Card><CardContent><SectionHeader title="Uso geral"/><Typography fontWeight={700}>{formatBytes(s.systemStorage.usedBytes)} de {formatBytes(s.systemStorage.totalBytes)}</Typography><LinearProgress color={s.systemStorage.usagePercent>=95?'error':s.systemStorage.usagePercent>=85?'warning':'primary'} variant="determinate" value={s.systemStorage.usagePercent} sx={{my:1,height:8,borderRadius:4}}/><Typography variant="body2" color="text.secondary">{formatPercent(s.systemStorage.usagePercent)} em uso · {formatBytes(s.systemStorage.freeBytes)} livres · {s.systemStorage.path}</Typography></CardContent></Card><Card><CardContent><SectionHeader title="Bibliotecas"/><Stack divider={<Divider/>}>{s.libraryStorage.map(l=><Stack key={l.id} direction="row" justifyContent="space-between" sx={{py:1.5}}><Box><Typography fontWeight={700}>{l.name}</Typography><Typography variant="body2" color="text.secondary">{formatNumber(l.fileCount)} arquivos · {formatNumber(l.contentCount)} conteúdos</Typography></Box><Stack alignItems="flex-end"><Typography fontWeight={700}>{formatBytes(l.sizeBytes)}</Typography><StatusBadge label={l.status} tone={l.status==='READY'?'success':'warning'}/></Stack></Stack>)}</Stack></CardContent></Card></Stack>}

function duration(ms:number){const total=Math.max(0,Math.floor(ms/1000));return [Math.floor(total/3600),Math.floor(total%3600/60),total%60].map(v=>String(v).padStart(2,'0')).join(':')}
function modeLabel(mode:string){return mode==='DIRECT_PLAY'?'Direct Play':mode==='DIRECT_STREAM'?'Direct Stream':'Transcode'}
function jobType(type:string){return type==='LIBRARY_AUTO_SYNC'?'Auto sync':'Scan da biblioteca'}
function jobStatus(status:string){return ({QUEUED:'Na fila',RUNNING:'Em execução',COMPLETED:'Concluído',COMPLETED_WITH_WARNINGS:'Concluído com avisos',FAILED:'Falhou',CANCELLED:'Cancelado'} as Record<string,string>)[status]??status}
