/**
 * @created 2026-10-13
 * @description 显示访问条目并按需展开请求关联信息和错误原因。
 * @author yunhungo
 */
import { Link } from 'react-router-dom';
import { z } from 'zod';
import { dateTime, type Resource } from '@/lib/contracts';
import { accessSchema, milliseconds } from '@/lib/observability';
export function AccessLogTable({
  items,
  services,
  scoped,
}: {
  items: z.infer<typeof accessSchema>[];
  services: Resource[];
  scoped: boolean;
}) {
  return (
    <div className='access-table__scroll'>
      <table className={`access-table ${scoped ? 'access-table--scoped' : ''}`}>
        <thead>
          <tr>
            <th>时间</th>
            {!scoped && <th>服务 / 域名</th>}
            <th>请求</th>
            <th>客户端</th>
            <th>状态</th>
            <th>响应头耗时</th>
          </tr>
        </thead>
        <tbody>
          {items.map((row) => (
            <tr key={row.id}>
              <td className='access-table__time'>
                <time>{dateTime(row.time)}</time>
              </td>
              {!scoped && (
                <td className='access-table__service'>
                  {row.service_id &&
                  services.some((s) => s.id === row.service_id) ? (
                    <Link to={`/web/${row.service_id}`}>
                      {services.find((s) => s.id === row.service_id)?.name}
                    </Link>
                  ) : (
                    <span>{row.service_id ? '已删除服务' : '未匹配服务'}</span>
                  )}
                  <small>{row.host || '—'}</small>
                </td>
              )}
              <td className='access-table__request'>
                <details>
                  <summary>
                    <b>{row.method}</b> <span>{row.path}</span>
                  </summary>
                  <dl>
                    <dt>请求 ID</dt>
                    <dd>{row.request_id}</dd>
                    <dt>入口</dt>
                    <dd>
                      {row.listener} · {row.tls ? 'HTTPS' : 'HTTP'}
                    </dd>
                    {row.phase !== 'request' && (
                      <>
                        <dt>阶段</dt>
                        <dd>
                          {row.phase === 'stream' ? '响应流' : '升级隧道'}
                        </dd>
                      </>
                    )}
                  </dl>
                </details>
                {row.error && (
                  <p className='access-table__error'>{row.error}</p>
                )}
              </td>
              <td className='access-table__client font-mono'>
                {row.client_ip}
              </td>
              <td className='access-table__status'>
                <span
                  className={
                    row.status >= 500 || row.phase !== 'request'
                      ? 'text-destructive'
                      : row.status >= 400
                        ? 'text-amber-600'
                        : 'text-primary'
                  }
                >
                  {row.status}
                </span>
              </td>
              <td className='access-table__duration'>
                {row.phase === 'request' ? milliseconds(row.duration_ms) : '—'}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
