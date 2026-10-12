/**
 * @created 2026-10-13
 * @description 当访问采集失败或数据被丢弃时提供可追踪的持久提示。
 * @author yunhungo
 */
import { Link } from 'react-router-dom';
import { useTelemetry } from '@/lib/observability-api';
export function CollectionNotice() {
  const query = useTelemetry();
  if (query.isError)
    return (
      <p role='alert' className='mt-3 text-xs text-destructive'>
        采集状态不可用{' '}
        <button className='underline' onClick={() => void query.refetch()}>
          重试
        </button>
      </p>
    );
  const data = query.data;
  if (
    !data ||
    data.dropped_logs + data.dropped_buckets + data.write_failures === 0
  )
    return null;
  return (
    <p role='alert' className='mt-3 text-xs text-destructive'>
      访问采集有丢失{' '}
      <Link className='underline' to='/logs?tab=access'>
        查看采集状态
      </Link>
    </p>
  );
}
