/**
 * @created 2026-10-13
 * @description 在总览中集中呈现服务分组状态及近一天的访问性能。
 * @author yunhungo
 */
import { Link } from 'react-router-dom';
import { useGroupStats } from '@/lib/groups-api';
import { errorText } from '@/lib/api';
import { number, milliseconds } from '@/lib/observability';
import './GroupSummary.scss';
export function GroupSummary() {
  const query = useGroupStats();
  const groups = query.data?.groups.filter((group) => group.services > 0) ?? [];
  if (query.data && !groups.length && !query.isError) return null;
  return (
    <section className='panel group-summary' aria-label='服务分组概况'>
      <div className='panel-heading'>
        <h2>服务分组</h2>
        <span>近 24 小时</span>
      </div>
      {query.isError && (
        <p role='alert' className='telemetry-message text-destructive'>
          {errorText(query.error)}{' '}
          <button onClick={() => void query.refetch()}>重试</button>
        </p>
      )}
      {!query.data && !query.isError && (
        <p role='status' className='telemetry-message'>
          加载中…
        </p>
      )}
      {groups.length > 0 && (
        <div className='overflow-x-auto'>
          <table>
            <thead>
              <tr>
                <th>分组</th>
                <th>服务 / 启用</th>
                <th>异常服务</th>
                <th>请求</th>
                <th>5xx</th>
                <th>平均响应头耗时</th>
              </tr>
            </thead>
            <tbody>
              {groups.map((group) => (
                <tr key={group.id}>
                  <td>
                    <Link to={`/web?group=${encodeURIComponent(group.id)}`}>
                      {group.name}
                    </Link>
                  </td>
                  <td>
                    {group.services} / {group.enabled}
                  </td>
                  <td className={group.errors ? 'text-destructive' : ''}>
                    {group.errors || '—'}
                  </td>
                  <td>{number(group.requests)}</td>
                  <td className={group.server_errors ? 'text-destructive' : ''}>
                    {group.server_errors || '—'}
                  </td>
                  <td>{milliseconds(group.avg_duration_ms)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}
