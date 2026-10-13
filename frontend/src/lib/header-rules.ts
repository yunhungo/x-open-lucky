/**
 * @created 2026-10-13
 * @description 转换可视化请求头规则并校验重复名称及行内容。
 * @author yunhungo
 */
import { z } from 'zod';
export const headerRowsSchema = z
  .array(
    z.object({
      id: z.string(),
      name: z.string(),
      action: z.enum(['set', 'remove']),
      value: z.string(),
    }),
  )
  .max(32);
export type HeaderRow = z.infer<typeof headerRowsSchema>[number];
export function headerRows(values: Record<string, string | null>): string {
  return JSON.stringify(
    Object.entries(values).map(([name, value]) => ({
      id: crypto.randomUUID(),
      name,
      action: value === null ? 'remove' : 'set',
      value: value ?? '',
    })),
  );
}
export function parseHeaderRows(raw: string): HeaderRow[] {
  const value: unknown = JSON.parse(raw || '[]');
  return headerRowsSchema.parse(value);
}
export function headerPayload(raw: string): Record<string, string | null> {
  const result: Record<string, string | null> = {};
  const names = new Set<string>();
  for (const row of parseHeaderRows(raw)) {
    const name = row.name.trim();
    if (!name) throw new Error('请填写请求头名称，或移除空行');
    if (names.has(name.toLowerCase()))
      throw new Error(`请求头名称重复：${name}`);
    names.add(name.toLowerCase());
    Object.defineProperty(result, name, {
      value: row.action === 'remove' ? null : row.value,
      enumerable: true,
    });
  }
  return result;
}
export const headerVariables = [
  { label: '请求域名', value: '{{ request.host }}' },
  { label: '客户端 IP', value: '{{ client.ip }}' },
  { label: 'HTTP / HTTPS', value: '{{ request.scheme }}' },
  { label: '路径与查询', value: '{{ request.uri }}' },
  { label: '请求 ID', value: '{{ request.id }}' },
  { label: '上游 Host', value: '{{ upstream.host }}' },
  {
    label: '引用请求头',
    value: '{{ request.headers["x-custom"] | default("fallback") }}',
  },
  {
    label: '条件示例',
    value: '{{ "secure" if request.scheme == "https" else "plain" }}',
  },
];
