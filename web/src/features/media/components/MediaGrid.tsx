import { Box } from '@mui/material';
import type { MediaCard } from '@/api';
import { MediaPosterCard } from './MediaPosterCard';

export function MediaGrid({ items }: { items: MediaCard[] }) {
  return <Box sx={{ display: 'grid', gridTemplateColumns: { xs: 'repeat(2, minmax(0, 1fr))', sm: 'repeat(auto-fill, minmax(150px, 1fr))' }, gap: { xs: 2, sm: 4 } }}>{items.map((item) => <MediaPosterCard key={item.id} item={item} />)}</Box>;
}
