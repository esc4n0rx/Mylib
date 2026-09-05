import { Box } from '@mui/material';
import VideoLibraryIcon from '@mui/icons-material/VideoLibrary';

export function BrandMark({ size = 32 }: { size?: number }) {
  return (
    <Box
      sx={{
        width: size,
        height: size,
        borderRadius: `${Math.round(size / 4)}px`,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        backgroundColor: (t) => t.tokens.primaryContainer,
        color: (t) => t.tokens.onPrimaryContainer,
        flexShrink: 0,
      }}
    >
      <VideoLibraryIcon sx={{ fontSize: size * 0.6 }} />
    </Box>
  );
}
