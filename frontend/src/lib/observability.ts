/**
 * @created 2026-10-13
 * @description 定义访问统计、日志和性能采样的 API 合同及展示格式。
 * @author yunhungo
 */
import { z } from 'zod';
export const performanceSchema = z.object({
  sampled_at: z.number().nullable(),
  cpu_percent: z.number().nullable(),
  memory_bytes: z.number().nullable(),
  container_memory_bytes: z.number().nullable(),
  memory_limit_bytes: z.number().nullable(),
  cpu_limit_cores: z.number().nullable(),
  history: z.array(
    z.object({
      time: z.number(),
      cpu_percent: z.number().nullable(),
      memory_bytes: z.number().nullable(),
    }),
  ),
});
export type Performance = z.infer<typeof performanceSchema>;
export const trafficSchema = z.object({
  since: z.number(),
  until: z.number(),
  requests: z.number(),
  client_errors: z.number(),
  server_errors: z.number(),
  stream_errors: z.number(),
  avg_duration_ms: z.number().nullable(),
  series: z.array(
    z.object({
      time: z.number(),
      requests: z.number(),
      client_errors: z.number(),
      server_errors: z.number(),
    }),
  ),
});
export type Traffic = z.infer<typeof trafficSchema>;
export const accessSchema = z.object({
  id: z.number(),
  time: z.number(),
  request_id: z.string(),
  service_id: z.string(),
  host: z.string(),
  listener: z.string(),
  client_ip: z.string(),
  method: z.string(),
  path: z.string(),
  status: z.number(),
  duration_ms: z.number(),
  error: z.string(),
  phase: z.enum(['request', 'stream', 'tunnel']),
  tls: z.boolean(),
});
export const accessPageSchema = z.object({
  items: z.array(accessSchema),
  next_cursor: z.number().nullable(),
});
export const telemetrySchema = z.object({
  policy: z.object({
    retention_days: z.number(),
    max_rows: z.number(),
    database_limit_bytes: z.number(),
    traffic_days: z.number(),
  }),
  rows: z.number(),
  database_bytes: z.number(),
  pending: z.number(),
  dropped_logs: z.number(),
  dropped_buckets: z.number(),
  write_failures: z.number(),
  last_flush: z.number(),
});
export function bytes(value: number | null) {
  if (value === null) return '—';
  return `${(value / 1024 / 1024).toFixed(1)} MiB`;
}
export function number(value: number) {
  return value.toLocaleString('zh-CN');
}
export function milliseconds(value: number | null) {
  if (value === null) return '—';
  return value < 1000
    ? `${value.toFixed(1)} ms`
    : `${(value / 1000).toFixed(2)} s`;
}
