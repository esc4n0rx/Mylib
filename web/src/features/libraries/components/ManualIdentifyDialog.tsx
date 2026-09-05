import {
  Alert,
  Box,
  Button,
  Chip,
  CircularProgress,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  Stack,
  TextField,
  Typography,
} from '@mui/material';
import SearchIcon from '@mui/icons-material/Search';
import { useEffect, useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { api, type IdentificationCandidate, type UnmatchedItem } from '@/api';
import { useToast } from '@/app/ToastProvider';
import { imageUrl } from '@/features/media/utils';

export function ManualIdentifyDialog({
  libraryId,
  item,
  open,
  onClose,
}: {
  libraryId: string;
  item?: UnmatchedItem;
  open: boolean;
  onClose: () => void;
}) {
  const [query, setQuery] = useState('');
  const [year, setYear] = useState('');
  const [results, setResults] = useState<IdentificationCandidate[]>([]);
  const queryClient = useQueryClient();
  const { notify } = useToast();
  useEffect(() => {
    if (open && item) {
      setQuery(item.normalizedTitle ?? '');
      setYear(item.year ? String(item.year) : '');
      setResults([]);
    }
  }, [open, item]);
  const search = useMutation({
    mutationFn: () =>
      api.media.identifySearch({
        libraryId,
        mediaFileId: item!.mediaFileId,
        query: query.trim(),
        year: year ? Number(year) : undefined,
      }),
    onSuccess: (data) => setResults(data.items),
  });
  const identify = useMutation({
    mutationFn: (candidate: IdentificationCandidate) =>
      api.media.identifyManual(item!.mediaFileId, candidate.providerId),
    onSuccess: (data) => {
      notify(
        data.associatedFiles > 1
          ? `${data.associatedFiles} episódios foram associados.`
          : 'Conteúdo identificado com sucesso.',
        'success',
      );
      void queryClient.invalidateQueries({
        queryKey: ['libraries', libraryId, 'unmatched'],
      });
      void queryClient.invalidateQueries({ queryKey: ['media'] });
      onClose();
    },
  });
  const submit = () => {
    if (query.trim() && item) search.mutate();
  };
  const episodeLabel =
    item?.season !== undefined
      ? ` · T${item.season}${item.episode !== undefined ? ` E${item.episode}` : ''}`
      : '';
  return (
    <Dialog open={open} onClose={onClose} fullWidth maxWidth="md">
      <DialogTitle>Identificar conteúdo</DialogTitle>
      <DialogContent>
        <Typography fontWeight={650} sx={{ wordBreak: 'break-all' }}>
          {item?.filename}
        </Typography>
        <Typography variant="body2" color="text.secondary" sx={{ mb: 3 }}>
          {item ? `Detectado: ${item.normalizedTitle ?? '—'}${episodeLabel}` : ''}
        </Typography>
        <Stack
          component="form"
          direction={{ xs: 'column', sm: 'row' }}
          spacing={1.5}
          onSubmit={(e) => {
            e.preventDefault();
            submit();
          }}
        >
          <TextField
            autoFocus
            fullWidth
            label="Título da série ou filme"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          <TextField
            label="Ano"
            value={year}
            onChange={(e) => setYear(e.target.value.replace(/\D/g, '').slice(0, 4))}
            sx={{ width: { sm: 130 } }}
          />
          <Button
            type="submit"
            variant="contained"
            startIcon={
              search.isPending ? (
                <CircularProgress size={18} color="inherit" />
              ) : (
                <SearchIcon />
              )
            }
            disabled={!query.trim() || search.isPending}
          >
            Buscar
          </Button>
        </Stack>
        {search.isError ? (
          <Alert severity="error" sx={{ mt: 2 }}>
            Não foi possível buscar no TMDB. Verifique a chave e tente novamente.
          </Alert>
        ) : null}
        {identify.isError ? (
          <Alert severity="error" sx={{ mt: 2 }}>
            Não foi possível associar o conteúdo. Tente novamente.
          </Alert>
        ) : null}
        {!search.isPending && search.isSuccess && !results.length ? (
          <Alert severity="info" sx={{ mt: 2 }}>
            Nenhum resultado encontrado. Tente o título original ou remova o ano.
          </Alert>
        ) : null}
        <Box
          sx={{
            display: 'grid',
            gridTemplateColumns: { xs: '1fr', sm: 'repeat(2,1fr)' },
            gap: 2,
            mt: 3,
          }}
        >
          {results.map((candidate) => (
            <Stack
              key={`${candidate.provider}-${candidate.providerId}`}
              direction="row"
              spacing={2}
              sx={{
                border: (t) => `1px solid ${t.tokens.outlineVariant}`,
                borderRadius: 2,
                p: 1.5,
              }}
            >
              <Box
                sx={{
                  width: 88,
                  aspectRatio: '2/3',
                  flexShrink: 0,
                  borderRadius: 1,
                  overflow: 'hidden',
                  bgcolor: (t) => t.tokens.surfaceContainerHigh,
                }}
              >
                {candidate.posterPath ? (
                  <Box
                    component="img"
                    src={imageUrl(candidate.posterPath)}
                    alt=""
                    sx={{ width: '100%', height: '100%', objectFit: 'cover' }}
                  />
                ) : null}
              </Box>
              <Stack minWidth={0} flex={1}>
                <Typography variant="h3">{candidate.title}</Typography>
                <Stack direction="row" spacing={1} sx={{ my: 1 }}>
                  <Chip size="small" label={candidate.year ?? 'Ano desconhecido'} />
                  {candidate.rating !== undefined ? (
                    <Chip size="small" label={`★ ${candidate.rating.toFixed(1)}`} />
                  ) : null}
                </Stack>
                <Typography
                  variant="body2"
                  color="text.secondary"
                  sx={{
                    display: '-webkit-box',
                    WebkitLineClamp: 3,
                    WebkitBoxOrient: 'vertical',
                    overflow: 'hidden',
                  }}
                >
                  {candidate.overview || 'Sem sinopse.'}
                </Typography>
                <Button
                  size="small"
                  sx={{ mt: 'auto', alignSelf: 'flex-start' }}
                  onClick={() => identify.mutate(candidate)}
                  disabled={identify.isPending}
                >
                  Associar este conteúdo
                </Button>
              </Stack>
            </Stack>
          ))}
        </Box>
      </DialogContent>
      <DialogActions>
        <Button onClick={onClose}>Cancelar</Button>
      </DialogActions>
    </Dialog>
  );
}
