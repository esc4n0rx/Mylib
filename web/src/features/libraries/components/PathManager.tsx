import { useState } from 'react';
import { Box, Button, IconButton, Stack, TextField, Typography } from '@mui/material';
import AddIcon from '@mui/icons-material/Add';
import DeleteOutlineIcon from '@mui/icons-material/DeleteOutline';
import { useTranslation } from 'react-i18next';
import { useMutation } from '@tanstack/react-query';
import { api, ApiError, pathStatus, type PathValidationStatus } from '@/api';
import { StatusBadge } from '@/components/StatusBadge';

interface PathManagerProps {
  paths: string[];
  onChange: (paths: string[]) => void;
  error?: string;
}

const STATUS_TONE: Record<PathValidationStatus, 'success' | 'error' | 'warning'> = {
  VALID: 'success',
  NOT_FOUND: 'error',
  NOT_READABLE: 'warning',
  INVALID: 'error',
};

const STATUS_KEY: Record<PathValidationStatus, string> = {
  VALID: 'valid',
  NOT_FOUND: 'notFound',
  NOT_READABLE: 'notReadable',
  INVALID: 'invalid',
};

export function PathManager({ paths, onChange, error }: PathManagerProps) {
  const { t } = useTranslation('libraries');
  const [draft, setDraft] = useState('');
  const [statuses, setStatuses] = useState<Record<string, PathValidationStatus>>({});

  const validate = useMutation({
    mutationFn: (path: string) => api.libraries.validatePath(path),
  });

  const addPath = async () => {
    const value = draft.trim();
    if (!value || paths.includes(value)) return;
    onChange([...paths, value]);
    setDraft('');
    try {
      const result = await validate.mutateAsync(value);
      setStatuses((prev) => ({ ...prev, [value]: pathStatus(result) }));
    } catch (err) {
      if (err instanceof ApiError) setStatuses((prev) => ({ ...prev, [value]: 'INVALID' }));
    }
  };

  const removePath = (value: string) => {
    onChange(paths.filter((p) => p !== value));
  };

  return (
    <Box>
      <Typography variant="h3" sx={{ mb: 1 }}>
        {t('form.paths')}
      </Typography>
      <Stack spacing={1} sx={{ mb: 1.5 }}>
        {paths.map((path) => (
          <Stack
            key={path}
            direction="row"
            alignItems="center"
            justifyContent="space-between"
            sx={{
              px: 1.5,
              py: 1,
              border: (th) => `1px solid ${th.tokens.outlineVariant}`,
              borderRadius: 2,
            }}
          >
            <Typography variant="body1" sx={{ fontFamily: 'monospace', wordBreak: 'break-all' }}>
              {path}
            </Typography>
            <Stack direction="row" spacing={1} alignItems="center">
              {statuses[path] ? (
                <StatusBadge
                  tone={STATUS_TONE[statuses[path]!]}
                  label={t(`pathStatus.${STATUS_KEY[statuses[path]!]}`)}
                />
              ) : null}
              <IconButton size="small" onClick={() => removePath(path)} aria-label="remover">
                <DeleteOutlineIcon fontSize="small" />
              </IconButton>
            </Stack>
          </Stack>
        ))}
      </Stack>
      <Stack direction="row" spacing={1}>
        <TextField
          size="small"
          fullWidth
          label={t('form.folderPath')}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') {
              e.preventDefault();
              void addPath();
            }
          }}
          error={Boolean(error)}
          helperText={error}
        />
        <Button
          onClick={() => void addPath()}
          startIcon={<AddIcon />}
          variant="outlined"
          disabled={validate.isPending}
          sx={{ flexShrink: 0 }}
        >
          {validate.isPending ? t('form.validating') : t('form.addLocation')}
        </Button>
      </Stack>
    </Box>
  );
}
