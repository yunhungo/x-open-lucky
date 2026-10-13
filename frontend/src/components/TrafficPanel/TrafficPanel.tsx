/**
 * @created 2026-10-13
 * @description 展示全站或指定服务的访问统计及逐小时趋势。
 * @author yunhungo
 */
import { useState } from 'react';
import { Link } from 'react-router-dom';
import { useTraffic } from '@/lib/observability-api';
import { errorText } from '@/lib/api';
import { milliseconds, number } from '@/lib/observability';
import { TrafficChart } from './TrafficChart';
import './TrafficPanel.scss';
export function TrafficPanel({ serviceId = '' }: { serviceId?: string }) {
  const [hours, setHours] = useState(24);
  const query = useTraffic(serviceId, hours);
  const data = query.data;
  const rate =
    data && data.requests
      ? `${((data.server_errors / data.requests) * 100).toFixed(1)}%`
      : '—';
  return (
    <section className='panel traffic-panel' aria-label='访问统计'>
      <div className='traffic-panel__heading'>
        <h2>访问情况</h2>
        <div className='flex items-center gap-4'>
          {!serviceId && (
            <Link
              className='text-muted-foreground hover:text-primary'
              to='/logs?tab=access'
            >
              访问日志
            </Link>
          )}
          <select
            aria-label='统计时间范围'
            value={hours}
            onChange={(e) => setHours(Number(e.target.value))}
          >
            <option value={24}>近 24 小时</option>
            <option value={168}>近 7 天</option>
            <option value={720}>近 30 天</option>
          </select>
        </div>
      </div>
      {query.isError && (
        <p role='alert' className='telemetry-message text-destructive'>
          {errorText(query.error)}{' '}
          <button onClick={() => void query.refetch()}>重试</button>
        </p>
      )}
      {!data ? (
        !query.isError && (
          <p role='status' className='telemetry-message'>
            加载中…
          </p>
        )
      ) : (
        <>
          <dl className='traffic-panel__stats'>
            <div>
              <dt>请求</dt>
              <dd>{number(data.requests)}</dd>
            </div>
            <div>
              <dt>
                服务端错误 <span>5xx</span>
              </dt>
              <dd className={data.server_errors ? 'text-destructive' : ''}>
                {number(data.server_errors)}
                <small>{rate}</small>
              </dd>
            </div>
            <div>
              <dt>
                客户端错误 <span>4xx</span>
              </dt>
              <dd>{number(data.client_errors)}</dd>
            </div>
            <div>
              <dt>平均响应头耗时</dt>
              <dd>{milliseconds(data.avg_duration_ms)}</dd>
            </div>
          </dl>
          {data.requests > 0 && <TrafficChart series={data.series} />}
          {data.stream_errors > 0 && (
            <p className='telemetry-message text-destructive'>
              流 / 隧道中断 {number(data.stream_errors)} 次
            </p>
          )}
        </>
      )}
    </section>
  );
}
