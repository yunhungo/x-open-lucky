/**
 * @created 2026-10-13
 * @description 展示 Web 服务配置、访问趋势与限定服务的访问日志。
 * @author yunhungo
 */
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Sheet } from '@/components/ui/sheet';
import { ResourceEditor } from '@/components/ResourceEditor/ResourceEditor';
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
  const [editing, setEditing] = useState(false);
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
        <Button
          size='sm'
          variant='outline'
          className='ml-auto'
          onClick={() => setEditing(true)}
        >
          编辑
        </Button>
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
            <dt>HTTP 版本</dt>
            <dd>
              {[
                service.options.http1 && '1.1',
                service.options.http2 && '2',
                service.options.http3 && '3',
              ]
                .filter(Boolean)
                .join(' / ')}
            </dd>
          </div>
          <div>
            <dt>接入协议</dt>
            <dd>
              {
                { both: 'HTTP / HTTPS', http: 'HTTP', https: 'HTTPS' }[
                  service.options.access_mode
                ]
              }
            </dd>
          </div>
          <div>
            <dt>上游协议</dt>
            <dd>
              {
                { auto: '自动协商', http1: 'HTTP/1.1', http2: 'HTTP/2' }[
                  service.options.upstream_version
                ]
              }
            </dd>
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
      <Sheet open={editing} onOpenChange={setEditing}>
        {editing && (
          <ResourceEditor
            kind='web'
            item={service}
            close={() => setEditing(false)}
          />
        )}
      </Sheet>
      <TrafficPanel key={`traffic-${service.id}`} serviceId={service.id} />
      <AccessLogPanel key={`access-${service.id}`} serviceId={service.id} />
    </>
  );
}
