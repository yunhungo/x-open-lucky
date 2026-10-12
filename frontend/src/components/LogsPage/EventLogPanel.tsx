/**
 * @created 2026-10-13
 * @description 筛选真实运行事件并导出日志。
 * @author yunhungo
 */
import { useState } from 'react';
import { Download, RefreshCw } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { QueryState } from '@/components/QueryState/QueryState';
import { useLogs } from '@/lib/api';
import { dateTime } from '@/lib/contracts';
import './LogsPage.scss';
export function EventLogPanel() {
  const [level, setLevel] = useState('');
  const [module, setModule] = useState('');
  const logs = useLogs(level, module);
  const download = () => {
    const blob = new Blob([JSON.stringify(logs.data, null, 2)], {
      type: 'application/json',
    });
    const url = URL.createObjectURL(blob);
    const link = document.createElement('a');
    link.href = url;
    link.download = 'openlucky-logs.json';
    link.click();
    URL.revokeObjectURL(url);
  };
  return (
    <>
      <div className='flex flex-wrap items-center gap-4 mb-3'>
        <select
          className='filter-select'
          aria-label='日志级别'
          value={level}
          onChange={(e) => setLevel(e.target.value)}
        >
          <option value=''>全部级别</option>
          <option value='info'>信息</option>
          <option value='warn'>警告</option>
          <option value='error'>错误</option>
        </select>
        <select
          className='filter-select'
          aria-label='日志模块'
          value={module}
          onChange={(e) => setModule(e.target.value)}
        >
          <option value=''>全部模块</option>
          <option value='system'>系统</option>
          <option value='auth'>登录</option>
          <option value='ddns'>动态域名</option>
          <option value='web'>Web 服务</option>
          <option value='certificates'>SSL 证书</option>
          <option value='credentials'>DNS 凭据</option>
        </select>
        <Button
          variant='ghost'
          size='icon'
          className='ml-auto'
          aria-label='刷新日志'
          disabled={logs.isFetching}
          onClick={() => void logs.refetch()}
        >
          <RefreshCw
            size={15}
            className={logs.isFetching ? 'animate-spin' : ''}
          />
        </Button>
        <Button
          variant='outline'
          onClick={download}
          disabled={!logs.data?.length}
        >
          <Download size={14} />
          导出
        </Button>
      </div>
      <section className='panel overflow-hidden'>
        {logs.isPending || logs.isError ? (
          <div className='p-6'>
            <QueryState error={logs.error} retry={() => void logs.refetch()} />
          </div>
        ) : logs.data.length === 0 ? (
          <div className='p-6 text-xs text-center text-muted-foreground'>
            暂无日志
          </div>
        ) : (
          <ul className='logs-list'>
            {logs.data.map((log) => (
              <li key={log.id}>
                <time>{dateTime(log.time)}</time>
                <span className={`logs-level logs-level--${log.level}`}>
                  {log.level === 'error'
                    ? '错误'
                    : log.level === 'warn'
                      ? '警告'
                      : '信息'}
                </span>
                <span className='logs-module'>{log.module}</span>
                <p>{log.message}</p>
              </li>
            ))}
          </ul>
        )}
      </section>
    </>
  );
}
