/**
 * @created 2026-10-13
 * @description 定义 Web 高级配置合同和表单转换。
 * @author yunhungo
 */
import { z } from 'zod';
import { headerPayload, headerRows } from './header-rules';
export const webOptionsSchema = z.object({
  access_mode: z.enum(['both', 'http', 'https']).default('both'),
  http1: z.boolean().default(true),
  http2: z.boolean().default(true),
  http3: z.boolean().default(false),
  tls_min: z.enum(['1.2', '1.3']).default('1.2'),
  upstream_version: z.enum(['auto', 'http1', 'http2']).default('auto'),
  connect_timeout: z.number().int().min(1).max(60).default(10),
  response_timeout: z.number().int().min(1).max(600).default(60),
  request_headers: z.record(z.string(), z.string().nullable()).default({}),
  response_headers: z.record(z.string(), z.string().nullable()).default({}),
});
export type WebOptions = z.infer<typeof webOptionsSchema>;
export const defaultWebOptions = webOptionsSchema.parse({});
export function optionValues(
  options: WebOptions,
): Record<string, string | number | boolean> {
  return Object.fromEntries(
    Object.entries(options).map(([key, value]) => [
      `option_${key}`,
      typeof value === 'object' ? headerRows(value) : value,
    ]),
  );
}
export function parseOptions(
  values: Record<string, string | number | boolean>,
): WebOptions {
  const options: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(values)) {
    if (!key.startsWith('option_')) continue;
    const name = key.slice(7);
    if (name.endsWith('_headers')) {
      options[name] = headerPayload(String(value));
    } else options[name] = value;
  }
  const result = webOptionsSchema.safeParse(options);
  if (!result.success)
    throw new Error('请检查高级配置中的超时范围与自定义头格式');
  return result.data;
}
