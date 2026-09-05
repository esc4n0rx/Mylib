import { Box, Button, Skeleton, Stack, Typography } from '@mui/material';
import { useQuery } from '@tanstack/react-query';
import { api, type Genre, type MediaType } from '@/api';
import { MediaPosterCard } from './MediaPosterCard';

export function GenreSection({ genre, type, onViewAll }: { genre: Genre; type: MediaType; onViewAll: () => void }) {
  const query = useQuery({ queryKey: ['media', type, 'genre', genre.id], queryFn: () => api.media.byGenre(type, genre.id, { pageSize: 8 }) });
  return <Box component="section">
    <Stack direction="row" alignItems="center" justifyContent="space-between" sx={{ mb: 2 }}><Box><Typography variant="h2">{genre.name}</Typography><Typography variant="body2" color="text.secondary">{genre.count} títulos</Typography></Box><Button onClick={onViewAll}>Ver todos</Button></Stack>
    <Box sx={{ display: 'grid', gridTemplateColumns: 'repeat(8, minmax(120px, 1fr))', gap: 3, overflowX: 'auto', pb: 1 }}>{query.isLoading ? Array.from({ length: 8 }, (_, i) => <Skeleton key={i} variant="rounded" sx={{ aspectRatio: '2 / 3', minWidth: 120 }} />) : query.data?.items.map((item) => <MediaPosterCard key={item.id} item={item} />)}</Box>
  </Box>;
}
