import { Box, CircularProgress, Stack, Typography } from '@mui/material';
import { BrandMark } from './BrandMark';

export function StartupScreen() {
  return (
    <Box
      sx={{
        minHeight: '100vh',
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        backgroundColor: (t) => t.tokens.externalBackground,
      }}
    >
      <Stack spacing={2} alignItems="center">
        <BrandMark size={48} />
        <Typography variant="h2">MyLib</Typography>
        <CircularProgress size={20} />
      </Stack>
    </Box>
  );
}
