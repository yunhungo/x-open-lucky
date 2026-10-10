/**
 * @created 2026-10-11
 * @description 发起同源会话请求，校验响应并刷新资源缓存。
 * @author yunhungo
 */
import { z } from 'zod';
import { useQuery } from '@tanstack/react-query';
import { schemas, type Kind, logSchema, overviewSchema } from './contracts';
export class ApiError extends Error {
  constructor(
    message: string,
    public status: number,
  ) {
    super(message);
  }
}
export async function request<T>(
  path: string,
  schema: z.ZodType<T>,
  options?: RequestInit,
): Promise<T> {
  const deadline = AbortSignal.timeout(30_000);
  const signal = options?.signal
    ? AbortSignal.any([options.signal, deadline])
    : deadline;
  const interrupted = () =>
    new ApiError(
      deadline.aborted ? '请求超时，请刷新确认操作结果' : '请求已取消',
      deadline.aborted ? 408 : 499,
    );
  let response: Response;
  try {
    response = await fetch(`/api/v1${path}`, {
      ...options,
      signal,
      credentials: 'same-origin',
      headers: {
        'Content-Type': 'application/json',
        'X-OpenLucky': '1',
        ...options?.headers,
      },
    });
  } catch {
    if (signal.aborted) throw interrupted();
    throw new ApiError('无法连接服务，请检查网络后重试', 0);
  }
  let data: unknown;
  try {
    data = await response.json();
  } catch {
    if (signal.aborted) throw interrupted();
    throw new ApiError('服务返回异常响应', response.status);
  }
  if (!response.ok) {
    const parsed = z.object({ error: z.string() }).safeParse(data);
    throw new ApiError(
      parsed.success ? parsed.data.error : '请求失败',
      response.status,
    );
  }
  const parsed = schema.safeParse(data);
  if (!parsed.success) throw new ApiError('服务数据格式异常', 500);
  return parsed.data;
}
export const okSchema = z.object({ ok: z.boolean() });
export const idSchema = z.object({ id: z.string() });
export function useResources(kind: Kind) {
  return useQuery({
    queryKey: ['resources', kind],
    queryFn: ({ signal }) =>
      request(`/resources/${kind}`, z.array(schemas[kind]), { signal }),
    refetchInterval: 5000,
  });
}
export function useLogs(level = '', module = '') {
  return useQuery({
    queryKey: ['logs', level, module],
    queryFn: ({ signal }) =>
      request(
        `/logs?level=${encodeURIComponent(level)}&module=${encodeURIComponent(module)}`,
        z.array(logSchema),
        { signal },
      ),
    refetchInterval: 5000,
  });
}
export function useOverview() {
  return useQuery({
    queryKey: ['overview'],
    queryFn: ({ signal }) => request('/overview', overviewSchema, { signal }),
    refetchInterval: 10000,
  });
}
export function errorText(error: unknown) {
  return error instanceof Error ? error.message : '操作失败，请重试';
}
