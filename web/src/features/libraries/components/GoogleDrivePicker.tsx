import { useCallback, useEffect, useState } from 'react';
import {
  Alert,
  Box,
  Button,
  Checkbox,
  Chip,
  CircularProgress,
  IconButton,
  Stack,
  Typography,
} from '@mui/material';
import ExpandMoreIcon from '@mui/icons-material/ExpandMore';
import ChevronRightIcon from '@mui/icons-material/ChevronRight';
import FolderIcon from '@mui/icons-material/Folder';
import { useTranslation } from 'react-i18next';
import { api, ApiError, type GoogleDriveConnection, type GoogleDriveItem } from '@/api';
import { useToast } from '@/app/ToastProvider';
import { useGoogleDriveConnections } from '../remoteSourceHooks';

export interface DriveSelection {
  connectionId: string;
  folders: { folderId: string; displayName: string }[];
}

export function GoogleDrivePicker({
  value,
  onChange,
}: {
  value: DriveSelection | null;
  onChange: (next: DriveSelection | null) => void;
}) {
  const { t } = useTranslation('remoteSources');
  const { notify } = useToast();
  const connections = useGoogleDriveConnections();
  const [connecting, setConnecting] = useState(false);
  const items = connections.data?.items ?? [];
  const active =
    items.find((connection) => connection.id === value?.connectionId) ?? items[0];

  useEffect(() => {
    if (active && value?.connectionId !== active.id) {
      onChange({ connectionId: active.id, folders: value?.folders ?? [] });
    }
  }, [active, value, onChange]);

  useEffect(() => {
    function onMessage(event: MessageEvent) {
      if (event.data?.source === 'mylib' && String(event.data.event).startsWith('google-drive')) {
        void connections.refetch();
        setConnecting(false);
      }
    }
    window.addEventListener('message', onMessage);
    return () => window.removeEventListener('message', onMessage);
  }, [connections]);

  const connect = async () => {
    setConnecting(true);
    try {
      const { authorizationUrl } = await api.googleDrive.connect();
      window.open(authorizationUrl, 'mylib-google', 'width=520,height=680');
    } catch (error) {
      setConnecting(false);
      notify(
        error instanceof ApiError && error.code === 'GOOGLE_OAUTH_NOT_CONFIGURED'
          ? t('googleDrive.notConfigured')
          : error instanceof ApiError
            ? error.localizedMessage
            : t('googleDrive.notConfigured'),
        'error',
      );
    }
  };

  if (items.length === 0) {
    return (
      <Stack spacing={1.5}>
        <Typography variant="body2" color="text.secondary">
          {t('provider.driveDesc')}
        </Typography>
        <Button variant="contained" onClick={connect} disabled={connecting}>
          {connecting ? t('googleDrive.connecting') : t('googleDrive.connect')}
        </Button>
      </Stack>
    );
  }

  const selectedFolders = value?.folders ?? [];
  const toggleFolder = (folder: GoogleDriveItem) => {
    if (!active) return;
    const exists = selectedFolders.some((entry) => entry.folderId === folder.id);
    const folders = exists
      ? selectedFolders.filter((entry) => entry.folderId !== folder.id)
      : [...selectedFolders, { folderId: folder.id, displayName: folder.name }];
    onChange({ connectionId: active.id, folders });
  };

  return (
    <Stack spacing={2}>
      <ConnectionRow connection={active} onReconnect={connect} reconnecting={connecting} />
      {active?.status === 'AUTH_REQUIRED' ? (
        <Alert severity="warning">{t('googleDrive.reconnect')}</Alert>
      ) : null}
      {active ? (
        <Box sx={{ border: (th) => `1px solid ${th.tokens.outlineVariant}`, borderRadius: 2, p: 1 }}>
          <FolderNode
            connectionId={active.id}
            folder={{ id: 'root', name: t('googleDrive.myDrive'), mimeType: 'folder', isFolder: true }}
            depth={0}
            selected={selectedFolders}
            onToggle={toggleFolder}
          />
        </Box>
      ) : null}
      {selectedFolders.length > 0 ? (
        <Stack direction="row" spacing={1} flexWrap="wrap" useFlexGap>
          {selectedFolders.map((folder) => (
            <Chip
              key={folder.folderId}
              label={folder.displayName}
              onDelete={() =>
                onChange({
                  connectionId: active!.id,
                  folders: selectedFolders.filter((entry) => entry.folderId !== folder.folderId),
                })
              }
            />
          ))}
        </Stack>
      ) : null}
    </Stack>
  );
}

function ConnectionRow({
  connection,
  onReconnect,
  reconnecting,
}: {
  connection?: GoogleDriveConnection;
  onReconnect: () => void;
  reconnecting: boolean;
}) {
  const { t } = useTranslation('remoteSources');
  if (!connection) return null;
  return (
    <Stack direction="row" alignItems="center" justifyContent="space-between">
      <Stack direction="row" spacing={1} alignItems="center">
        <Box
          sx={{
            width: 8,
            height: 8,
            borderRadius: '50%',
            bgcolor: connection.status === 'CONNECTED' ? 'success.main' : 'warning.main',
          }}
        />
        <Typography variant="body2">{connection.accountEmail}</Typography>
      </Stack>
      <Button size="small" onClick={onReconnect} disabled={reconnecting}>
        {t('googleDrive.reconnect')}
      </Button>
    </Stack>
  );
}

function FolderNode({
  connectionId,
  folder,
  depth,
  selected,
  onToggle,
}: {
  connectionId: string;
  folder: GoogleDriveItem;
  depth: number;
  selected: { folderId: string }[];
  onToggle: (folder: GoogleDriveItem) => void;
}) {
  const { t } = useTranslation('remoteSources');
  const [open, setOpen] = useState(depth === 0);
  const [children, setChildren] = useState<GoogleDriveItem[] | null>(null);
  const [nextPageToken, setNextPageToken] = useState<string | undefined>();
  const [loading, setLoading] = useState(false);

  const load = useCallback(
    async (pageToken?: string) => {
      setLoading(true);
      try {
        const page = await api.googleDrive.browse(connectionId, {
          folderId: folder.id,
          pageToken,
          pageSize: 100,
        });
        const folders = page.items.filter((item) => item.isFolder);
        setChildren((current) => (pageToken ? [...(current ?? []), ...folders] : folders));
        setNextPageToken(page.nextPageToken);
      } finally {
        setLoading(false);
      }
    },
    [connectionId, folder.id],
  );

  useEffect(() => {
    if (open && children === null && !loading) void load();
  }, [open, children, loading, load]);

  const isSelected = selected.some((entry) => entry.folderId === folder.id);

  return (
    <Box sx={{ pl: depth === 0 ? 0 : 2 }}>
      <Stack direction="row" alignItems="center">
        <IconButton size="small" aria-label={folder.name} onClick={() => setOpen((value) => !value)}>
          {open ? <ExpandMoreIcon fontSize="small" /> : <ChevronRightIcon fontSize="small" />}
        </IconButton>
        {depth > 0 ? (
          <Checkbox size="small" checked={isSelected} onChange={() => onToggle(folder)} />
        ) : null}
        <FolderIcon fontSize="small" sx={{ mr: 0.5, color: 'text.secondary' }} />
        <Typography variant="body2">{folder.name}</Typography>
      </Stack>
      {open ? (
        <Box>
          {loading && children === null ? (
            <Stack direction="row" spacing={1} alignItems="center" sx={{ pl: 5, py: 0.5 }}>
              <CircularProgress size={14} />
              <Typography variant="caption" color="text.secondary">
                {t('googleDrive.loading')}
              </Typography>
            </Stack>
          ) : null}
          {children?.length === 0 ? (
            <Typography variant="caption" color="text.secondary" sx={{ pl: 5 }}>
              {t('googleDrive.noFolders')}
            </Typography>
          ) : null}
          {children?.map((child) => (
            <FolderNode
              key={child.id}
              connectionId={connectionId}
              folder={child}
              depth={depth + 1}
              selected={selected}
              onToggle={onToggle}
            />
          ))}
          {nextPageToken ? (
            <Button size="small" onClick={() => void load(nextPageToken)} disabled={loading}>
              {t('googleDrive.loadMore')}
            </Button>
          ) : null}
        </Box>
      ) : null}
    </Box>
  );
}
