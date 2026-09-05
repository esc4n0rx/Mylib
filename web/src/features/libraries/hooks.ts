import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { api, type AutoSyncConfig, type CreateLibraryRequest } from '@/api';
import { TERMINAL_SCAN_STATUSES } from '@/api';

export const librariesKey = ['libraries'] as const;
export const libraryKey = (id: string) => ['libraries', id] as const;
export const scansKey = (id: string) => ['libraries', id, 'scans'] as const;
export const scanKey = (id: string, scanId: string) =>
  ['libraries', id, 'scans', scanId] as const;

export function useLibraries() {
  return useQuery({ queryKey: librariesKey, queryFn: api.libraries.list });
}

export function useLibrary(id: string) {
  return useQuery({ queryKey: libraryKey(id), queryFn: () => api.libraries.get(id) });
}

export function useLibraryStats(id: string) {
  return useQuery({ queryKey: [...libraryKey(id), 'stats'], queryFn: () => api.libraries.stats(id) });
}

export function usePathStatuses(id: string) {
  return useQuery({ queryKey: [...libraryKey(id), 'path-status'], queryFn: () => api.libraries.pathStatuses(id) });
}

export function useUpdateAutoSync(id: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (autoSync: AutoSyncConfig) => api.libraries.update(id, { autoSync }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: libraryKey(id) });
      void queryClient.invalidateQueries({ queryKey: librariesKey });
    },
  });
}

export function useCreateLibrary() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (body: CreateLibraryRequest) => api.libraries.create(body),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: librariesKey }),
  });
}

export function useScanHistory(libraryId: string) {
  return useQuery({
    queryKey: scansKey(libraryId),
    queryFn: () => api.scans.list(libraryId),
  });
}

export function useStartScan(libraryId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (scanType?: 'FULL' | 'INCREMENTAL') => api.scans.start(libraryId, scanType),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: scansKey(libraryId) });
      void queryClient.invalidateQueries({ queryKey: libraryKey(libraryId) });
    },
  });
}

/** Polls a running scan and stops automatically when it reaches a terminal state. */
export function useScanProgress(libraryId: string, scanId: string | null) {
  return useQuery({
    queryKey: scanId ? scanKey(libraryId, scanId) : ['scan', 'idle'],
    queryFn: () => api.scans.get(libraryId, scanId as string),
    enabled: Boolean(scanId),
    refetchInterval: (query) => {
      const status = query.state.data?.status;
      if (status && TERMINAL_SCAN_STATUSES.includes(status)) return false;
      return 2000;
    },
  });
}

export function useCancelScan(libraryId: string) {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (scanId: string) => api.scans.cancel(libraryId, scanId),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: scansKey(libraryId) }),
  });
}

export function useUnmatched(libraryId: string) {
  return useQuery({
    queryKey: ['libraries', libraryId, 'unmatched'],
    queryFn: () => api.media.unmatched(libraryId),
  });
}
