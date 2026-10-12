/**
 * @created 2026-10-13
 * @description 展示 Web 服务配置、访问趋势与限定服务的访问日志。
 * @author yunhungo
 */
import { ArrowLeft } from 'lucide-react';
import { Link, useParams } from 'react-router-dom';
import { useResources } from '@/lib/api';
import { QueryState } from '@/components/QueryState/QueryState';
import { TrafficPanel } from '@/components/TrafficPanel/TrafficPanel';
import { AccessLogPanel } from '@/components/AccessLogPanel/AccessLogPanel';
import { StatusBadge } from '@/components/StatusBadge/StatusBadge';
import './ServiceDetailPage.scss';
export function ServiceDetailPage() {
  const { id } = useParams();
  const resources = useResources('web');
  if (!resources.data || resources.isError)
    return (
      <QueryState
        error={resources.error}
        retry={() => void resources.refetch()}
      />
    );
  const service = resources.data.find((r) => r.id === id);
  if (!service || !('target' in service))
    return (
      <div className='text-sm'>
        <Link to='/web' className='text-primary'>
          返回 Web 服务
        </Link>
        <p className='mt-4'>服务不存在或已删除</p>
      </div>
    );
  return (
    <>
      <div className='service-detail__heading'>
        <Link to='/web' aria-label='返回 Web 服务'>
          <ArrowLeft size={18} />
        </Link>
        <h1>{service.name}</h1>
        <StatusBadge resource={service} />
      </div>
      <section className='panel service-detail__config' aria-label='服务配置'>
        <dl>
          <div>
            <dt>域名</dt>
            <dd>{service.domain}</dd>
          </div>
          <div>
            <dt>目标地址</dt>
            <dd>{service.target}</dd>
          </div>
          <div>
            <dt>入口</dt>
            <dd>{service.listener}</dd>
          </div>
          <div>
            <dt>HTTPS 跳转</dt>
            <dd>{service.https_redirect ? '开启' : '关闭'}</dd>
          </div>
          <div>
            <dt>保留 Host</dt>
            <dd>{service.preserve_host ? '开启' : '关闭'}</dd>
          </div>
        </dl>
      </section>
      <TrafficPanel key={`traffic-${service.id}`} serviceId={service.id} />
      <AccessLogPanel key={`access-${service.id}`} serviceId={service.id} />
    </>
  );
}
