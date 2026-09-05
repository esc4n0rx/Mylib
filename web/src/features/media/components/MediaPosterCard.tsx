import { Box, Card, CardActionArea, Chip, Stack, Typography } from '@mui/material';
import StarRoundedIcon from '@mui/icons-material/StarRounded';
import MovieOutlinedIcon from '@mui/icons-material/MovieOutlined';
import { useNavigate } from 'react-router-dom';
import type { MediaCard } from '@/api';
import { FavoriteButton } from './FavoriteButton';
import { imageUrl } from '../utils';

export function MediaPosterCard({ item }: { item: MediaCard }) {
  const navigate = useNavigate();
  const target = item.mediaType === 'TV_SHOW' ? `/tv/${item.id}` : `/media/${item.id}`;
  return <Card sx={{ width: '100%', minWidth: 0, bgcolor: 'transparent', boxShadow: 'none', position: 'relative' }}>
    <CardActionArea onClick={() => navigate(target)} sx={{ borderRadius: (t) => `${t.ds.radius.poster}px` }}>
      <Box sx={{ position: 'relative', aspectRatio: '2 / 3', overflow: 'hidden', borderRadius: (t) => `${t.ds.radius.poster}px`, bgcolor: (t) => t.tokens.surfaceContainerHigh }}>
        {item.posterPath ? <Box component="img" src={imageUrl(item.posterPath)} alt="" loading="lazy" sx={{ width: '100%', height: '100%', objectFit: 'cover' }} /> : <Stack sx={{ height: '100%' }} alignItems="center" justifyContent="center"><MovieOutlinedIcon color="disabled" sx={{ fontSize: 48 }} /></Stack>}
        {item.rating !== undefined ? <Chip size="small" icon={<StarRoundedIcon />} label={item.rating.toFixed(1)} sx={{ position: 'absolute', left: 6, bottom: 6, bgcolor: 'rgba(0,0,0,.72)', color: 'white', '& .MuiChip-icon': { color: '#FFD54F' } }} /> : null}
      </Box>
      <Typography fontWeight={650} noWrap sx={{ mt: 1, fontSize: { xs: 14, sm: 16 } }}>{item.title}</Typography>
      <Typography variant="body2" color="text.secondary" noWrap>{item.year ?? '—'}{item.mediaType === 'TV_SHOW' && item.numberOfSeasons ? ` • ${item.numberOfSeasons} temp.` : ''}</Typography>
      {item.recommendationReason ? <Typography variant="caption" color="text.secondary" noWrap sx={{ display: 'block', mt: 0.35 }}>{item.recommendationReason}</Typography> : null}
    </CardActionArea>
    <Box sx={{ position: 'absolute', top: 4, right: 4, zIndex: 1, borderRadius: '50%', bgcolor: 'rgba(0,0,0,.58)', color: 'white' }}><FavoriteButton id={item.id} isFavorite={item.isFavorite} size="small" /></Box>
  </Card>;
}
