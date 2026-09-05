import { Box, Button, Card, CardContent, Chip, LinearProgress, Stack, Typography } from '@mui/material';
import StarRoundedIcon from '@mui/icons-material/StarRounded';
import PlayArrowIcon from '@mui/icons-material/PlayArrow';
import type { Episode } from '@/api';
import { imageUrl, formatRuntime } from '../utils';

export function EpisodeCard({ episode, progress, onPlay }: { episode: Episode; progress?: number; onPlay?: () => void }) {
  const code = `S${episode.seasonNumber.toString().padStart(2, '0')}E${episode.episodeNumber.toString().padStart(2, '0')}`;
  return <Card variant="outlined"><Stack direction={{ xs: 'column', sm: 'row' }}>
    <Box sx={{ width: { xs: '100%', sm: 220 }, aspectRatio: '16/9', flexShrink: 0, bgcolor: (t) => t.tokens.surfaceContainerHigh, position: 'relative' }}>{episode.stillPath ? <Box component="img" src={imageUrl(episode.stillPath, 'w780')} alt="" loading="lazy" sx={{ width: '100%', height: '100%', objectFit: 'cover' }} /> : null}{progress !== undefined ? <LinearProgress variant="determinate" value={progress} sx={{ position: 'absolute', left: 0, right: 0, bottom: 0 }} /> : null}</Box>
    <CardContent sx={{ minWidth: 0, flex: 1 }}><Stack direction="row" spacing={1} alignItems="center"><Chip size="small" label={code} /><Typography variant="h3" noWrap>{episode.name ?? `Episódio ${episode.episodeNumber}`}</Typography></Stack><Typography variant="body2" color="text.secondary" sx={{ mt: 1, display: '-webkit-box', WebkitLineClamp: 2, WebkitBoxOrient: 'vertical', overflow: 'hidden' }}>{episode.overview || 'Sinopse não disponível.'}</Typography><Stack direction="row" spacing={2} alignItems="center" sx={{ mt: 2 }}><Typography variant="body2">{formatRuntime(episode.runtime)}</Typography>{episode.rating !== undefined ? <Typography variant="body2"><StarRoundedIcon sx={{ fontSize: 15, color: 'warning.main', verticalAlign: 'text-bottom' }} /> {episode.rating.toFixed(1)}</Typography> : null}<Typography variant="body2">{episode.airDate ? new Date(episode.airDate).toLocaleDateString('pt-BR') : '—'}</Typography>{episode.mediaFileId && onPlay ? <Button size="small" startIcon={<PlayArrowIcon />} onClick={onPlay}>{progress ? 'Continuar' : 'Reproduzir'}</Button> : null}{progress !== undefined && progress >= 92 ? <Chip size="small" color="success" label="Concluído" /> : null}</Stack></CardContent>
  </Stack></Card>;
}
