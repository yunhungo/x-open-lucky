/**
 * @created 2026-10-13
 * @description 提供有游标分页的访问日志、错误筛选及单页导出。
 * @author yunhungo
 */
import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Download, RefreshCw, Search } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { errorText, request, useResources } from '@/lib/api';
import { accessPageSchema } from '@/lib/observability';
import { AccessLogTable } from './AccessLogTable';
import { TelemetryStatus } from './TelemetryStatus';
import './AccessLogPanel.scss';
export function AccessLogPanel({ serviceId = '' }: { serviceId?: string }) {
  const services = useResources('web');
  const [filter, setFilter] = useState({
    service: '',
    errors: false,
    status: '',
    hours: '24',
    q: '',
  });
  const [draft, setDraft] = useState('');
  const [cursors, setCursors] = useState<number[]>([]);
  const [anchor, setAnchor] = useState(() => Math.floor(Date.now() / 1000));
  const before = cursors.at(-1);
  const params = new URLSearchParams({
    service_id: serviceId || filter.service,
    errors: String(filter.errors),
    q: filter.q,
  });
  if (before) params.set('before', String(before));
  if (filter.status) params.set('status', filter.status);
  if (filter.hours)
    params.set('since', String(anchor - Number(filter.hours) * 3600));
  const search = params.toString();
  const query = useQuery({
    queryKey: ['access-logs', search],
    queryFn: ({ signal }) =>
      request(`/access-logs?${search}`, accessPageSchema, { signal }),
    refetchInterval: before ? false : 10000,
  });
  const update = (value: Partial<typeof filter>) => {
    setFilter({ ...filter, ...value });
    setCursors([]);
    setAnchor(Math.floor(Date.now() / 1000));
  };
  const refresh = () => {
    setCursors([]);
    setAnchor(Math.floor(Date.now() / 1000));
    if (!before) void query.refetch();
  };
  const download = () => {
    if (!query.data) return;
    const url = URL.createObjectURL(
      new Blob([JSON.stringify(query.data.items, null, 2)], {
        type: 'application/json',
      }),
    );
    const link = document.createElement('a');
    link.href = url;
    link.download = 'openlucky-access-logs.json';
    link.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  };
  return (
    <section className='access-panel' aria-label='访问日志'>
      <div className='access-panel__toolbar'>
        <form
          className='access-panel__search'
          onSubmit={(e) => {
            e.preventDefault();
            update({ q: draft });
          }}
        >
          <input
            aria-label='搜索访问日志'
            placeholder='路径、IP、域名或请求 ID'
            maxLength={200}
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
          />
          <button aria-label='搜索' type='submit'>
            <Search size={15} />
          </button>
        </form>
        {!serviceId && (
          <select
            aria-label='筛选服务'
            value={filter.service}
            onChange={(e) => update({ service: e.target.value })}
          >
            <option value=''>全部服务</option>
            {services.data?.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name}
              </option>
            ))}
          </select>
        )}
        <select
          aria-label='日志时间范围'
          value={filter.hours}
          onChange={(e) => update({ hours: e.target.value })}
        >
          <option value='1'>近 1 小时</option>
          <option value='24'>近 24 小时</option>
          <option value='168'>近 7 天</option>
          <option value=''>全部保留记录</option>
        </select>
        <select
          aria-label='HTTP 状态码'
          value={filter.status}
          onChange={(e) => update({ status: e.target.value })}
        >
          <option value=''>全部状态码</option>
          {[
            101, 200, 204, 301, 302, 304, 307, 308, 400, 401, 403, 404, 429,
            500, 502, 503, 504, 508,
          ].map((s) => (
            <option key={s} value={s}>
              {s}
            </option>
          ))}
        </select>
        <label className='access-panel__errors'>
          <input
            type='checkbox'
            checked={filter.errors}
            onChange={(e) => update({ errors: e.target.checked })}
          />
          仅错误
        </label>
        <div className='access-panel__actions'>
          <TelemetryStatus />
          <Button
            variant='ghost'
            size='icon'
            aria-label='刷新访问日志'
            disabled={query.isFetching}
            onClick={refresh}
          >
            <RefreshCw
              size={14}
              className={query.isFetching ? 'animate-spin' : ''}
            />
          </Button>
          <Button
            variant='outline'
            size='sm'
            onClick={download}
            disabled={!query.data?.items.length}
          >
            <Download size={13} />
            导出本页
          </Button>
        </div>
      </div>
      {services.isError && !serviceId && (
        <p role='alert' className='telemetry-message text-destructive'>
          服务筛选加载失败{' '}
          <button onClick={() => void services.refetch()}>重试</button>
        </p>
      )}
      <div className='panel overflow-hidden'>
        {query.isError ? (
          <p role='alert' className='telemetry-message text-destructive'>
            {errorText(query.error)}{' '}
            <button onClick={() => void query.refetch()}>重试</button>
          </p>
        ) : query.isPending ? (
          <p role='status' className='telemetry-message'>
            加载中…
          </p>
        ) : query.data.items.length === 0 ? (
          <p className='access-panel__empty'>暂无匹配记录</p>
        ) : (
          <AccessLogTable
            items={query.data.items}
            services={services.data ?? []}
            scoped={Boolean(serviceId)}
          />
        )}
        <div className='access-panel__footer'>
          <span>{query.data?.items.length ?? 0} 条</span>
          <div className='flex items-center gap-2'>
            <Button
              variant='ghost'
              size='sm'
              disabled={!cursors.length || query.isFetching}
              onClick={() => setCursors(cursors.slice(0, -1))}
            >
              上一页
            </Button>
            <span>{cursors.length + 1}</span>
            <Button
              variant='ghost'
              size='sm'
              disabled={!query.data?.next_cursor || query.isFetching}
              onClick={() => {
                const next = query.data?.next_cursor;
                if (next) setCursors([...cursors, next]);
              }}
            >
              下一页
            </Button>
          </div>
        </div>
      </div>
    </section>
  );
}
