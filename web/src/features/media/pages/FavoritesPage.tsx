import { useState } from 'react';
import { Box, Pagination, Skeleton, Tab, Tabs, Typography } from '@mui/material';
import FavoriteBorderIcon from '@mui/icons-material/FavoriteBorder';
import { useQuery } from '@tanstack/react-query';
import { api, type MediaType } from '@/api';
import { EmptyState } from '@/components/states/EmptyState';
import { MediaGrid } from '../components/MediaGrid';

export default function FavoritesPage() {
  const [type, setType] = useState<MediaType | ''>(''); const [page, setPage] = useState(1);
  const query = useQuery({ queryKey: ['media', 'favorites', type, page], queryFn: () => api.media.favorites({ type: type || undefined, page, pageSize: 24 }) });
  return <Box><Typography variant="h1">Favoritos</Typography><Typography color="text.secondary" sx={{ mt: 1 }}>Os filmes e séries que você adicionou à Minha Lista.</Typography><Tabs value={type} onChange={(_e, value: MediaType | '') => { setType(value); setPage(1); }} sx={{ my: 3 }}><Tab value="" label="Todos" /><Tab value="MOVIE" label="Filmes" /><Tab value="TV_SHOW" label="Séries" /></Tabs>{query.isLoading ? <Skeleton variant="rounded" height={360} /> : query.data?.items.length ? <><MediaGrid items={query.data.items} /><Pagination page={page} count={query.data.totalPages ?? 1} onChange={(_e, value) => setPage(value)} sx={{ mt: 5, display: 'flex', justifyContent: 'center' }} /></> : <EmptyState icon={<FavoriteBorderIcon sx={{ fontSize: 48 }} />} title="Sua lista está vazia" body="Adicione filmes e séries à Minha Lista para encontrá-los rapidamente aqui." />}</Box>;
}
