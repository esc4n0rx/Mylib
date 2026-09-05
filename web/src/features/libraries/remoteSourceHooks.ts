import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { api, ApiError, type RemoteSelection } from '@/api';
import { libraryKey } from './hooks';

export const remoteSourcesKey = (libraryId: string) =>
  ['libraries', libraryId, 'remote-sources'] as const;
export const remoteSourceKey = (id: string) => ['remote-sources', id] as const;

export function useRemoteSources(libraryId: string) {
  return useQuery({
    queryKey: remoteSourcesKey(libraryId),
    queryFn: () => api.remoteSources.list(libraryId),
    // Poll while any source is mid-sync so the UI reflects progress/outcome.
    refetchInterval: (query) =>
      query.state.data?.items.some((source) => source.status === 'SYNCING') ? 3000 : false,
  });
}

const ALREADY_RUNNING = 'REMOTE_SYNC_ALREADY_RUNNING';

/** Fire a background sync; a 409 (already running) is a no-op, not an error. */
export async function startRemoteSync(id: string): Promise<'started' | 'already-running'> {
  try {
    await api.remoteSources.sync(id, false);
    return 'started';
  } catch (error) {
    if (error instanceof ApiError && error.code === ALREADY_RUNNING) return 'already-running';
    throw error;
  }
}

export function useRemoteSourceEntries(
  id: string,
  query?: Record<string, string | number | boolean | undefined>,
  enabled = true,
) {
  return useQuery({
    queryKey: [...remoteSourceKey(id), 'entries', query],
    queryFn: () => api.remoteSources.entries(id, query),
    enabled,
  });
}

export function useRemoteSourceSelections(id: string, enabled = true) {
  return useQuery({
    queryKey: [...remoteSourceKey(id), 'selections'],
    queryFn: () => api.remoteSources.selections(id),
    enabled,
  });
}

export function useCreateRemoteSource(libraryId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (body: Parameters<typeof api.remoteSources.create>[1]) =>
      api.remoteSources.create(libraryId, body),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: remoteSourcesKey(libraryId) });
      void queryClient.invalidateQueries({ queryKey: libraryKey(libraryId) });
    },
  });
}

export function useRemoteSourceMutations(libraryId: string, sourceId: string) {
  const queryClient = useQueryClient();
  const invalidate = () => {
    void queryClient.invalidateQueries({ queryKey: remoteSourcesKey(libraryId) });
    void queryClient.invalidateQueries({ queryKey: remoteSourceKey(sourceId) });
    void queryClient.invalidateQueries({ queryKey: libraryKey(libraryId) });
  };
  return {
    update: useMutation({
      mutationFn: (body: Parameters<typeof api.remoteSources.update>[1]) =>
        api.remoteSources.update(sourceId, body),
      onSuccess: invalidate,
    }),
    remove: useMutation({
      mutationFn: () => api.remoteSources.remove(sourceId),
      onSuccess: invalidate,
    }),
    sync: useMutation({
      mutationFn: () => startRemoteSync(sourceId),
      onSuccess: invalidate,
    }),
    setSelections: useMutation({
      mutationFn: (selections: RemoteSelection[]) =>
        api.remoteSources.setSelections(sourceId, selections),
      onSuccess: invalidate,
    }),
  };
}

export function useGoogleDriveConnections(enabled = true) {
  return useQuery({
    queryKey: ['google-drive', 'connections'],
    queryFn: () => api.googleDrive.connections(),
    enabled,
  });
}
