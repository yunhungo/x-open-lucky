/**
 * @created 2026-10-13
 * @description 查询共享分组及汇总，并在归组后统一失效相关管理缓存。
 * @author yunhungo
 */
import { useQuery, type QueryClient } from '@tanstack/react-query';
import { z } from 'zod';
import { request } from './api';
import { groupSchema, groupStatsSchema } from './groups';
export function useGroups(enabled = true) {
  return useQuery({
    queryKey: ['groups'],
    enabled,
    queryFn: ({ signal }) =>
      request('/groups', z.array(groupSchema), { signal }),
    staleTime: 10_000,
    refetchInterval: 30_000,
  });
}
export function useGroupStats(enabled = true, hours = 24) {
  return useQuery({
    queryKey: ['group-stats', hours],
    enabled,
    queryFn: ({ signal }) =>
      request(`/group-stats?hours=${hours}`, groupStatsSchema, { signal }),
    refetchInterval: 10_000,
  });
}
export async function refreshGroups(client: QueryClient): Promise<void> {
  await Promise.all(
    [
      'groups',
      'group-stats',
      'resources',
      'overview',
      'traffic',
      'access-logs',
    ].map((key) => client.invalidateQueries({ queryKey: [key] })),
  );
}
