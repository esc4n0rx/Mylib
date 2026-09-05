import { useQuery } from '@tanstack/react-query';
import { api } from '@/api';

export function useSetupStatus() {
  return useQuery({
    queryKey: ['setup', 'status'],
    queryFn: api.setup.status,
    staleTime: 60_000,
    retry: 1,
  });
}
