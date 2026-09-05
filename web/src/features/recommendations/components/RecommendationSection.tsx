import { Box, Skeleton, Stack, Typography } from '@mui/material';
import type { RecommendationSectionData } from '@/api';
import { MediaPosterCard } from '@/features/media/components/MediaPosterCard';

export function RecommendationSection({ section }: { section: RecommendationSectionData }) {
  if (!section.items.length) return null;
  return <Box component="section" data-testid={`recommendation-${section.key}`}>
    <Typography variant="h2" sx={{ mb: 2 }}>{section.title}</Typography>
    <Box sx={{ display: 'grid', gridAutoFlow: 'column', gridAutoColumns: { xs: '42vw', sm: 150, md: 'minmax(130px, 1fr)' }, gridTemplateColumns: { md: 'repeat(8, minmax(130px, 1fr))' }, gap: { xs: 2, md: 3 }, overflowX: 'auto', pb: 1, mx: { xs: -2, md: 0 }, px: { xs: 2, md: 0 }, scrollSnapType: 'x proximity', '& > *': { scrollSnapAlign: 'start' } }}>
      {section.items.map((item) => <MediaPosterCard key={item.id} item={item} />)}
    </Box>
  </Box>;
}

export function RecommendationSkeleton() {
  return <Box component="section" aria-label="Carregando recomendações">
    <Typography variant="h2" sx={{ mb: 2 }}>Recomendado para Você</Typography>
    <Stack direction="row" spacing={2} sx={{ overflow: 'hidden' }}>{Array.from({ length: 6 }, (_, index) => <Box key={index} sx={{ width: { xs: '42vw', sm: 150 }, flexShrink: 0 }}><Skeleton variant="rounded" sx={{ aspectRatio: '2/3' }} /><Skeleton width="75%" sx={{ mt: 1 }} /></Box>)}</Stack>
  </Box>;
}
