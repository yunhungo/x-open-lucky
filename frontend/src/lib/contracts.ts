/**
 * @created 2026-10-11
 * @description 校验 API 边界并提供配置资源类型。
 * @author yunhungo
 */
import { z } from 'zod';
import { performanceSchema } from './observability';
export const kinds = ['ddns', 'web', 'certificates', 'credentials'] as const;
export type Kind = (typeof kinds)[number];
export const statusSchema = z.object({
  state: z.enum(['idle', 'running', 'ready', 'error']).optional(),
  ip: z.string().optional(),
  message: z.string().optional(),
  last_attempt: z.number().optional(),
  expires_at: z.number().optional(),
  issuer: z.string().optional(),
  domains: z.array(z.string()).optional(),
});
const base = {
  revision: z.number(),
  id: z.string(),
  name: z.string(),
  status: statusSchema.default({}),
};
export const credentialSchema = z.object({
  ...base,
  provider: z.enum(['cloudflare', 'alidns', 'tencentcloud']),
  has_secret: z.boolean(),
  secrets: z.record(z.string(), z.string()),
});
export const ddnsSchema = z.object({
  ...base,
  enabled: z.boolean(),
  credential_id: z.string(),
  zone: z.string(),
  domain: z.string(),
  record_type: z.enum(['A', 'AAAA']),
  source: z.enum(['public', 'interface', 'static']),
  source_value: z.string(),
  interval: z.number(),
  ttl: z.number(),
  proxied: z.boolean(),
});
export const webSchema = z.object({
  listener: z.string().default('default'),
  ...base,
  enabled: z.boolean(),
  domain: z.string(),
  target: z.string(),
  certificate_id: z.string(),
  https_redirect: z.boolean(),
  preserve_host: z.boolean(),
});
export const certificateSchema = z.object({
  ...base,
  domains: z.array(z.string()),
  email: z.string(),
  credential_id: z.string(),
  staging: z.boolean(),
  auto_renew: z.boolean(),
  imported: z.boolean(),
});
export const schemas = {
  ddns: ddnsSchema,
  web: webSchema,
  certificates: certificateSchema,
  credentials: credentialSchema,
};
export type Resource = z.infer<(typeof schemas)[Kind]>;
export type Credential = z.infer<typeof credentialSchema>;
export type Certificate = z.infer<typeof certificateSchema>;
export const logSchema = z.object({
  id: z.number(),
  time: z.number(),
  level: z.enum(['info', 'error', 'warn']),
  module: z.string(),
  message: z.string(),
});
export type LogEntry = z.infer<typeof logSchema>;
const count = z.object({
  total: z.number(),
  enabled: z.number(),
  errors: z.number(),
});
export const overviewSchema = z.object({
  performance: performanceSchema,
  counts: z.object({ ddns: count, web: count, certificates: count }),
  uptime: z.number(),
  build_sha: z.string(),
  version: z.string(),
  admin_addr: z.string(),
  http_addr: z.string(),
  https_addr: z.string(),
  listeners: z
    .array(
      z.object({
        id: z.string(),
        addr: z.string(),
        https_port: z.number(),
        mixed: z.boolean(),
      }),
    )
    .default([]),
});
export type Overview = z.infer<typeof overviewSchema>;
export const names: Record<Kind, string> = {
  ddns: '动态域名',
  web: 'Web 服务',
  certificates: 'SSL 证书',
  credentials: 'DNS 凭据',
};
export const providers: Record<string, string> = {
  cloudflare: 'Cloudflare',
  alidns: '阿里云 DNS',
  tencentcloud: '腾讯云 DNSPod',
};
export function dateTime(timestamp?: number) {
  return timestamp
    ? new Date(timestamp * 1000).toLocaleString('zh-CN', { hour12: false })
    : '—';
}
export function dateOnly(timestamp?: number) {
  return timestamp
    ? new Date(timestamp * 1000).toLocaleDateString('zh-CN')
    : '—';
}
export function uptime(seconds: number) {
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  return days
    ? `${days} 天 ${hours} 小时`
    : hours
      ? `${hours} 小时 ${minutes} 分钟`
      : `${minutes} 分钟`;
}
