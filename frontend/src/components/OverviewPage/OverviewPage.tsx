/**
 * @created 2026-10-11
 * @description 展示资源、进程性能、访问统计和近期运行事件。
 * @author yunhungo
 */
import { Link } from 'react-router-dom';
import {
  ArrowUpRight,
  Globe2,
  Network,
  ShieldCheck,
  Clock3,
  Server,
  CircleCheck,
} from 'lucide-react';
import { useLogs, useOverview } from '@/lib/api';
import { dateTime, uptime } from '@/lib/contracts';
import { QueryState } from '@/components/QueryState/QueryState';
import { PerformancePanel } from '@/components/PerformancePanel/PerformancePanel';
import { TrafficPanel } from '@/components/TrafficPanel/TrafficPanel';
import { GroupSummary } from '@/components/GroupSummary/GroupSummary';
import { CollectionNotice } from './CollectionNotice';
import './OverviewPage.scss';
export function OverviewPage() {
  const summary = useOverview();
  const logs = useLogs();
  if (!summary.data)
    return (
      <QueryState error={summary.error} retry={() => void summary.refetch()} />
    );
  const data = summary.data;
  const cards = [
    { kind: 'ddns', name: '动态域名', icon: Globe2, label: '已启用' },
    { kind: 'web', name: 'Web 服务', icon: Network, label: '已启用' },
    {
      kind: 'certificates',
      name: 'SSL 证书',
      icon: ShieldCheck,
      label: '有效',
    },
  ] as const;
  return (
    <>
      <h1 className='sr-only'>概览</h1>
      {summary.isError && (
        <p role='alert' className='mb-3 text-xs text-destructive'>
          数据刷新失败{' '}
          <button className='underline' onClick={() => void summary.refetch()}>
            重试
          </button>
        </p>
      )}
      <div className='overview-metrics'>
        {cards.map((card) => (
          <Link
            className='panel overview-metric'
            key={card.kind}
            to={`/${card.kind}`}
          >
            <div className='flex justify-between items-center'>
              <span className='flex items-center gap-2.5 text-[13px] text-muted-foreground'>
                <card.icon size={17} strokeWidth={1.6} />
                {card.name}
              </span>
              <ArrowUpRight size={15} className='text-muted-foreground/60' />
            </div>
            <div className='overview-metric__value'>
              {data.counts[card.kind].total}
              <span>个</span>
            </div>
            <div className='flex items-center justify-between text-xs'>
              <span className='text-muted-foreground'>
                {data.counts[card.kind].enabled} {card.label}
              </span>
              {data.counts[card.kind].errors > 0 ? (
                <span className='text-destructive'>
                  {data.counts[card.kind].errors} 异常
                </span>
              ) : data.counts[card.kind].total > 0 ? (
                <span className='flex items-center gap-1 text-primary'>
                  <CircleCheck size={12} />
                  正常
                </span>
              ) : null}
            </div>
          </Link>
        ))}
      </div>
      <PerformancePanel data={data.performance} />
      <CollectionNotice />
      <TrafficPanel />
      <GroupSummary />
      <div className='overview-detail'>
        <section className='panel'>
          <div className='panel-heading'>
            <span className='flex items-center gap-2'>
              <Server size={15} />
              服务
            </span>
            <span className='text-[11px] font-normal text-muted-foreground'>
              v{data.version}
            </span>
          </div>
          <dl className='overview-list'>
            <div>
              <dt>管理面板</dt>
              <dd>{data.admin_addr}</dd>
            </div>
            <div>
              <dt>HTTP</dt>
              <dd>{data.http_addr}</dd>
            </div>
            <div>
              <dt>HTTPS</dt>
              <dd>{data.https_addr}</dd>
            </div>
            {data.listeners.map((entry) => (
              <div key={entry.id}>
                <dt>{entry.id}</dt>
                <dd>{entry.addr}</dd>
              </div>
            ))}
            <div>
              <dt className='flex items-center gap-1.5'>
                <Clock3 size={13} />
                运行时间
              </dt>
              <dd>{uptime(data.uptime)}</dd>
            </div>
            <div>
              <dt>构建版本</dt>
              <dd>
                {data.build_sha === 'dev'
                  ? '开发版'
                  : data.build_sha.slice(0, 12)}
              </dd>
            </div>
          </dl>
        </section>
        <section className='panel'>
          <div className='panel-heading'>
            <span>近期活动</span>
            <Link
              to='/logs?tab=events'
              className='flex items-center gap-1 text-[11px] font-normal text-muted-foreground hover:text-primary'
            >
              全部
              <ArrowUpRight size={13} />
            </Link>
          </div>
          {logs.isError ? (
            <QueryState error={logs.error} retry={() => void logs.refetch()} />
          ) : (
            <ul className='activity-list'>
              {logs.data?.slice(0, 6).map((log) => (
                <li key={log.id}>
                  <span
                    className={`activity-list__dot ${log.level === 'error' ? 'is-error' : ''}`}
                  />
                  <div>
                    <p>{log.message}</p>
                    <time>{dateTime(log.time)}</time>
                  </div>
                </li>
              ))}
              {logs.data?.length === 0 && (
                <li className='text-muted-foreground'>暂无运行记录</li>
              )}
            </ul>
          )}
        </section>
      </div>
      {cards.every((c) => data.counts[c.kind].total === 0) && (
        <section className='overview-start'>
          <span className='text-xs text-muted-foreground'>开始配置</span>
          <div>
            <Link to='/settings/dns'>
              添加 DNS 凭据
              <ArrowUpRight size={14} />
            </Link>
            <Link to='/ddns'>
              添加动态域名
              <ArrowUpRight size={14} />
            </Link>
            <Link to='/web'>
              添加 Web 服务
              <ArrowUpRight size={14} />
            </Link>
          </div>
        </section>
      )}
    </>
  );
}
