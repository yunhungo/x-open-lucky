/**
 * @created 2026-10-11
 * @description 在访问明细和系统事件之间切换运行日志视图。
 * @author yunhungo
 */
import { useSearchParams } from 'react-router-dom';
import { AccessLogPanel } from '@/components/AccessLogPanel/AccessLogPanel';
import { EventLogPanel } from './EventLogPanel';
import './LogsPage.scss';
export function LogsPage() {
  const [params, setParams] = useSearchParams();
  const tab = params.get('tab') === 'events' ? 'events' : 'access';
  const select = (value: string) => {
    const next = new URLSearchParams(params);
    next.set('tab', value);
    setParams(next);
    document.getElementById(`${value}-tab`)?.focus();
  };
  return (
    <>
      <h1 className='sr-only'>运行日志</h1>
      <div className='logs-tabs' role='tablist' aria-label='日志类型'>
        <button
          id='access-tab'
          role='tab'
          aria-selected={tab === 'access'}
          tabIndex={tab === 'access' ? 0 : -1}
          onKeyDown={(e) => {
            if (['ArrowLeft', 'ArrowRight'].includes(e.key)) {
              e.preventDefault();
              select('events');
            }
          }}
          aria-controls='access-logs-panel'
          onClick={() => select('access')}
        >
          访问日志
        </button>
        <button
          id='events-tab'
          role='tab'
          aria-selected={tab === 'events'}
          tabIndex={tab === 'events' ? 0 : -1}
          onKeyDown={(e) => {
            if (['ArrowLeft', 'ArrowRight'].includes(e.key)) {
              e.preventDefault();
              select('access');
            }
          }}
          aria-controls='events-logs-panel'
          onClick={() => select('events')}
        >
          系统事件
        </button>
      </div>
      <div
        id={`${tab}-logs-panel`}
        role='tabpanel'
        aria-labelledby={`${tab}-tab`}
      >
        {tab === 'access' ? <AccessLogPanel /> : <EventLogPanel />}
      </div>
    </>
  );
}
