import { useState } from 'react';
import {
  Box,
  Button,
  Dialog,
  DialogActions,
  DialogContent,
  DialogTitle,
  IconButton,
  Stack,
  Typography,
} from '@mui/material';
import AddIcon from '@mui/icons-material/Add';
import DeleteOutlineIcon from '@mui/icons-material/DeleteOutline';
import { useTranslation } from 'react-i18next';
import type { CreateLibraryRequest } from '@/api';
import { LibraryForm } from '@/features/libraries/components/LibraryForm';
import { EmptyState } from '@/components/states/EmptyState';

interface LibrariesStepProps {
  formId: string;
  libraries: CreateLibraryRequest[];
  onAdd: (library: CreateLibraryRequest) => void;
  onRemove: (index: number) => void;
  onNext: () => void;
}

export function LibrariesStep({ formId, libraries, onAdd, onRemove, onNext }: LibrariesStepProps) {
  const { t } = useTranslation('setup');
  const { t: tl } = useTranslation('libraries');
  const { t: tc } = useTranslation('common');
  const [open, setOpen] = useState(false);

  return (
    <Stack spacing={3}>
      <div>
        <Typography variant="h1">{t('libraries.title')}</Typography>
        <Typography variant="body1" color="text.secondary" sx={{ mt: 0.5 }}>
          {t('libraries.subtitle')}
        </Typography>
      </div>

      {libraries.length === 0 ? (
        <EmptyState title={tl('empty.title')} body={t('libraries.pendingNote')} />
      ) : (
        <Stack spacing={1}>
          {libraries.map((library, index) => (
            <Stack
              key={`${library.name}-${index}`}
              direction="row"
              justifyContent="space-between"
              alignItems="center"
              sx={{
                px: 1.5,
                py: 1,
                border: (th) => `1px solid ${th.tokens.outlineVariant}`,
                borderRadius: 2,
              }}
            >
              <Box>
                <Typography variant="h3">{library.name}</Typography>
                <Typography variant="body2" color="text.secondary">
                  {tl(`type.${library.type}`)} · {tl(`privacy.${library.privacy}`)} ·{' '}
                  {library.paths.length}
                </Typography>
              </Box>
              <IconButton size="small" onClick={() => onRemove(index)} aria-label="remover">
                <DeleteOutlineIcon fontSize="small" />
              </IconButton>
            </Stack>
          ))}
        </Stack>
      )}

      <Button startIcon={<AddIcon />} variant="outlined" onClick={() => setOpen(true)} sx={{ alignSelf: 'flex-start' }}>
        {t('libraries.addLibrary')}
      </Button>

      <form
        id={formId}
        onSubmit={(e) => {
          e.preventDefault();
          onNext();
        }}
      />

      <Dialog open={open} onClose={() => setOpen(false)} maxWidth="md" fullWidth>
        <DialogTitle>{tl('createLibrary')}</DialogTitle>
        <DialogContent dividers>
          <LibraryForm
            formId="setup-library-form"
            onSubmit={(request) => {
              onAdd(request);
              setOpen(false);
            }}
          />
        </DialogContent>
        <DialogActions>
          <Button onClick={() => setOpen(false)}>{tc('actions.cancel')}</Button>
          <Button type="submit" form="setup-library-form" variant="contained">
            {tl('createLibrary')}
          </Button>
        </DialogActions>
      </Dialog>
    </Stack>
  );
}
