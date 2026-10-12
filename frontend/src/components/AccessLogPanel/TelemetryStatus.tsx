/**
 * @created 2026-10-13
 * @description 按需展示日志容量策略并突出采集丢失和写入故障。
 * @author yunhungo
 */
import { useTelemetry } from '@/lib/observability-api';
import { bytes, number } from '@/lib/observability';
import { dateTime } from '@/lib/contracts';
export function TelemetryStatus() {
  const query = useTelemetry();
  const data = query.data;
  if (query.isError)
    return (
      <button className='text-destructive' onClick={() => void query.refetch()}>
        采集状态不可用 · 重试
      </button>
    );
  if (!data) return null;
  const incomplete =
    data.dropped_logs + data.dropped_buckets + data.write_failures > 0;
  return (
    <details className='telemetry-status'>
      <summary className={incomplete ? 'text-destructive' : ''}>
        {incomplete ? '采集有丢失' : '保留策略'}
      </summary>
      <dl>
        <dt>访问明细</dt>
        <dd>
          {data.policy.retention_days} 天 / {number(data.policy.max_rows)} 条
        </dd>
        <dt>访问统计</dt>
        <dd>{data.policy.traffic_days} 天</dd>
        <dt>已用空间</dt>
        <dd>
          {bytes(data.database_bytes)} /{' '}
          {bytes(data.policy.database_limit_bytes)}
        </dd>
        <dt>已存 / 待写</dt>
        <dd>
          {number(data.rows)} / {data.pending}
        </dd>
        <dt>最近写入</dt>
        <dd>{dateTime(data.last_flush)}</dd>
        {incomplete && (
          <>
            <dt>本次运行丢弃明细</dt>
            <dd>{number(data.dropped_logs)}</dd>
            <dt>丢弃统计桶</dt>
            <dd>{number(data.dropped_buckets)}</dd>
            <dt>写入失败</dt>
            <dd>{number(data.write_failures)}</dd>
          </>
        )}
      </dl>
    </details>
  );
}
