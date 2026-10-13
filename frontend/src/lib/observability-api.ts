/**
 * @created 2026-10-13
 * @description 管理访问统计与日志采集状态的刷新和取消请求。
 * @author yunhungo
 */
import { useQuery } from '@tanstack/react-query';
import { request } from './api';
import { trafficSchema, telemetrySchema } from './observability';
export function useTraffic(serviceId = '', hours = 24, groupId = '') {
  return useQuery({
    queryKey: ['traffic', serviceId, hours, groupId],
    queryFn: ({ signal }) =>
      request(
        `/traffic?${new URLSearchParams({ service_id: serviceId, ...(groupId ? { group_id: groupId } : {}), hours: String(hours) })}`,
        trafficSchema,
        { signal },
      ),
    refetchInterval: 10000,
  });
}
export function useTelemetry() {
  return useQuery({
    queryKey: ['telemetry'],
    queryFn: ({ signal }) => request('/telemetry', telemetrySchema, { signal }),
    refetchInterval: 10000,
  });
}
