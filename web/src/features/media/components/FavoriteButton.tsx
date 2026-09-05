import { useEffect, useState } from 'react';
import { IconButton, Tooltip } from '@mui/material';
import FavoriteIcon from '@mui/icons-material/Favorite';
import FavoriteBorderIcon from '@mui/icons-material/FavoriteBorder';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { api } from '@/api';
import { useToast } from '@/app/ToastProvider';

export function FavoriteButton({ id, isFavorite, size = 'medium' }: { id: string; isFavorite: boolean; size?: 'small' | 'medium' }) {
  const [active, setActive] = useState(isFavorite);
  const client = useQueryClient();
  const { notify } = useToast();
  useEffect(() => setActive(isFavorite), [isFavorite]);
  const mutation = useMutation<void, Error, boolean, { previous: boolean }>({
    mutationFn: async (next: boolean) => { if (next) { await api.media.addFavorite(id); } else { await api.media.removeFavorite(id); } },
    onMutate: (next) => { const previous = active; setActive(next); return { previous }; },
    onError: (_error, _next, context) => { setActive(context?.previous ?? isFavorite); notify('Não foi possível atualizar Minha Lista.', 'error'); },
    onSettled: () => { void client.invalidateQueries({ queryKey: ['media'] }); void client.invalidateQueries({ queryKey: ['recommendations'] }); },
  });
  const label = active ? 'Remover da Minha Lista' : 'Adicionar à Minha Lista';
  return <Tooltip title={label}><IconButton size={size} color={active ? 'primary' : 'default'} aria-label={label} disabled={mutation.isPending} onClick={(event) => { event.preventDefault(); event.stopPropagation(); mutation.mutate(!active); }}>{active ? <FavoriteIcon /> : <FavoriteBorderIcon />}</IconButton></Tooltip>;
}
