import { useEffect, useState } from 'react';
import { Box, Chip, FormControl, InputLabel, MenuItem, Pagination, Select, Skeleton, Stack, TextField, Typography } from '@mui/material';
import SearchIcon from '@mui/icons-material/Search';
import { useQuery } from '@tanstack/react-query';
import { api, type MediaType } from '@/api';
import { EmptyState } from '@/components/states/EmptyState';
import { MediaGrid } from '../components/MediaGrid';
import { GenreSection } from '../components/GenreSection';

export default function CatalogPage({ type }: { type: MediaType }) {
  const [searchInput, setSearchInput] = useState(''); const [search, setSearch] = useState('');
  const [genre, setGenre] = useState(''); const [year, setYear] = useState(''); const [rating, setRating] = useState(''); const [sort, setSort] = useState('addedAt'); const [favorite, setFavorite] = useState(false); const [page, setPage] = useState(1);
  useEffect(() => { const timer = window.setTimeout(() => { setSearch(searchInput); setPage(1); }, 400); return () => window.clearTimeout(timer); }, [searchInput]);
  const genresQuery = useQuery({ queryKey: ['media', type, 'genres'], queryFn: type === 'MOVIE' ? api.media.movieGenres : api.media.tvGenres });
  const queryParams = { page, pageSize: 24, search: search || undefined, genre: genre || undefined, year: year ? Number(year) : undefined, minRating: rating ? Number(rating) : undefined, sort, favorite: favorite || undefined };
  const catalogQuery = useQuery({ queryKey: ['media', type, 'catalog', queryParams], queryFn: () => type === 'MOVIE' ? api.media.movies(queryParams) : api.media.tvShows(queryParams) });
  const filtered = Boolean(search || genre || year || rating || favorite);
  const title = type === 'MOVIE' ? 'Filmes' : 'Séries';
  return <Box><Typography variant="h1">{title}</Typography><Typography color="text.secondary" sx={{ mt: 1, mb: 4 }}>Explore seu catálogo por gênero, ano, avaliação e biblioteca.</Typography>
    <Stack direction={{ xs: 'column', md: 'row' }} spacing={2} sx={{ mb: 3 }}>
      <TextField value={searchInput} onChange={(e) => setSearchInput(e.target.value)} label="Pesquisar" placeholder="Título, gênero ou ano" InputProps={{ startAdornment: <SearchIcon color="action" sx={{ mr: 1 }} /> }} sx={{ flex: 1 }} />
      <FormControl sx={{ minWidth: 170 }}><InputLabel>Gênero</InputLabel><Select value={genre} label="Gênero" onChange={(e) => { setGenre(e.target.value); setPage(1); }}><MenuItem value="">Todos</MenuItem>{genresQuery.data?.map((g) => <MenuItem key={g.id} value={g.id}>{g.name}</MenuItem>)}</Select></FormControl>
      <TextField label="Ano" type="number" value={year} onChange={(e) => { setYear(e.target.value); setPage(1); }} sx={{ width: 120 }} />
      <FormControl sx={{ minWidth: 150 }}><InputLabel>Rating mínimo</InputLabel><Select value={rating} label="Rating mínimo" onChange={(e) => { setRating(e.target.value); setPage(1); }}><MenuItem value="">Qualquer</MenuItem>{[5,6,7,8,9].map((r) => <MenuItem key={r} value={r}>{r}+</MenuItem>)}</Select></FormControl>
      <FormControl sx={{ minWidth: 190 }}><InputLabel>Ordenar por</InputLabel><Select value={sort} label="Ordenar por" onChange={(e) => setSort(e.target.value)}><MenuItem value="addedAt">Recentemente adicionados</MenuItem><MenuItem value="title">Título</MenuItem><MenuItem value="year">Ano</MenuItem><MenuItem value="rating">Rating</MenuItem><MenuItem value="popularity">Popularidade</MenuItem></Select></FormControl>
    </Stack>
    <Stack direction="row" spacing={1} sx={{ mb: 4 }}><Chip label="Somente Minha Lista" color={favorite ? 'primary' : 'default'} variant={favorite ? 'filled' : 'outlined'} onClick={() => { setFavorite(!favorite); setPage(1); }} />{filtered ? <Chip label="Limpar filtros" onDelete={() => { setSearchInput(''); setSearch(''); setGenre(''); setYear(''); setRating(''); setFavorite(false); }} /> : null}</Stack>
    {!filtered && genresQuery.data?.length ? <Stack spacing={7}>{genresQuery.data.map((g) => <GenreSection key={g.id} genre={g} type={type} onViewAll={() => setGenre(g.id)} />)}</Stack> : catalogQuery.isLoading ? <Box sx={{ display: 'grid', gridTemplateColumns: 'repeat(6, 1fr)', gap: 3 }}>{Array.from({ length: 12 }, (_, i) => <Skeleton key={i} variant="rounded" sx={{ aspectRatio: '2 / 3' }} />)}</Box> : catalogQuery.data?.items.length ? <><MediaGrid items={catalogQuery.data.items} /><Pagination page={page} count={catalogQuery.data.totalPages ?? 1} onChange={(_e, value) => setPage(value)} sx={{ mt: 5, display: 'flex', justifyContent: 'center' }} /></> : <EmptyState title="Nenhum conteúdo encontrado" body="Ajuste a pesquisa ou remova alguns filtros." />}
  </Box>;
}
