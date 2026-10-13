/**
 * @created 2026-10-13
 * @description 定义服务分组、成员版本和批量汇总的边界合同。
 * @author yunhungo
 */
import { z } from 'zod';
export const UNASSIGNED = 'unassigned';
export const groupSchema = z.object({
  id: z.string(),
  name: z.string(),
  position: z.number().int(),
  revision: z.number().int(),
});
export type ServiceGroup = z.infer<typeof groupSchema>;
export type VersionedService = { id: string; revision: number };
export const groupStatSchema = z.object({
  id: z.string(),
  name: z.string(),
  position: z.number(),
  services: z.number(),
  enabled: z.number(),
  errors: z.number(),
  requests: z.number(),
  client_errors: z.number(),
  server_errors: z.number(),
  stream_errors: z.number(),
  avg_duration_ms: z.number().nullable(),
});
export const groupStatsSchema = z.object({
  since: z.number(),
  until: z.number(),
  groups: z.array(groupStatSchema),
});
export type GroupStat = z.infer<typeof groupStatSchema>;
export function inGroup(groupId: string, filter: string): boolean {
  return !filter || groupId === (filter === UNASSIGNED ? '' : filter);
}
export function groupLabel(groups: ServiceGroup[], groupId: string): string {
  return groupId
    ? (groups.find((g) => g.id === groupId)?.name ?? '分组已删除')
    : '未分组';
}
