import { Chip } from '@mui/material';
import HourglassEmptyIcon from '@mui/icons-material/HourglassEmpty';
import TravelExploreIcon from '@mui/icons-material/TravelExplore';
import FactCheckIcon from '@mui/icons-material/FactCheck';
import SaveIcon from '@mui/icons-material/Save';
import CheckCircleIcon from '@mui/icons-material/CheckCircle';
import WarningAmberIcon from '@mui/icons-material/WarningAmber';
import ErrorOutlineIcon from '@mui/icons-material/ErrorOutline';
import BlockIcon from '@mui/icons-material/Block';
import type { ReactElement } from 'react';
import { useTranslation } from 'react-i18next';
import type { ScanStatus } from '@/api';

const META: Record<
  ScanStatus,
  { color: 'default' | 'info' | 'success' | 'warning' | 'error'; icon: ReactElement }
> = {
  QUEUED: { color: 'default', icon: <HourglassEmptyIcon fontSize="small" /> },
  SCANNING: { color: 'info', icon: <TravelExploreIcon fontSize="small" /> },
  MATCHING: { color: 'info', icon: <FactCheckIcon fontSize="small" /> },
  PERSISTING: { color: 'info', icon: <SaveIcon fontSize="small" /> },
  COMPLETED: { color: 'success', icon: <CheckCircleIcon fontSize="small" /> },
  COMPLETED_WITH_WARNINGS: { color: 'warning', icon: <WarningAmberIcon fontSize="small" /> },
  FAILED: { color: 'error', icon: <ErrorOutlineIcon fontSize="small" /> },
  CANCELLED: { color: 'default', icon: <BlockIcon fontSize="small" /> },
  SKIPPED_ALREADY_RUNNING: { color: 'default', icon: <BlockIcon fontSize="small" /> },
};

export function ScanStatusChip({ status }: { status: ScanStatus }) {
  const { t } = useTranslation('libraries');
  const meta = META[status];
  // Never colour-only: the label and icon carry the meaning too.
  return (
    <Chip
      size="small"
      color={meta.color}
      variant="outlined"
      icon={meta.icon}
      label={t(`scan.states.${status}`)}
    />
  );
}
