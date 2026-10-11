/**
 * @created 2026-10-11
 * @description 展示资源关键参数并提供启停、执行和编辑操作。
 * @author yunhungo
 */
import { Pencil, Trash2, RefreshCw, Upload, LoaderCircle } from 'lucide-react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { z } from 'zod';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Switch } from '@/components/ui/switch';
import { StatusBadge } from '@/components/StatusBadge/StatusBadge';
import { errorText, idSchema, request } from '@/lib/api';
import {
  dateOnly,
  dateTime,
  providers,
  type Kind,
  type Resource,
} from '@/lib/contracts';
export function ResourceRow({
  kind,
  item,
  edit,
  remove,
  upload,
}: {
  kind: Kind;
  item: Resource;
  edit: () => void;
  remove: () => void;
  upload: () => void;
}) {
  const queryClient = useQueryClient();
  const running = item.status.state === 'running';
  const refresh = () => {
    void queryClient.invalidateQueries({ queryKey: ['resources'] });
    void queryClient.invalidateQueries({ queryKey: ['overview'] });
  };
  const toggle = useMutation({
    mutationFn: (enabled: boolean) =>
      request(`/resources/${kind}/${item.id}`, idSchema, {
        method: 'PUT',
        body: JSON.stringify({ ...item, enabled }),
      }),
    onSuccess: () => {
      refresh();
      toast.success('启用状态已更新');
    },
    onError: (e) => toast.error(errorText(e)),
  });
  const run = useMutation({
    mutationFn: () =>
      request(
        `/resources/${kind}/${item.id}/run`,
        z.object({ queued: z.boolean() }),
        { method: 'POST' },
      ),
    onSuccess: () => {
      refresh();
      toast('任务已开始');
    },
    onError: (e) => toast.error(errorText(e)),
  });
  return (
    <tr>
      <td>
        <div className='font-medium text-[13px]'>{item.name}</div>
        <div className='mt-1.5 text-xs text-muted-foreground break-all'>
          {'domain' in item
            ? item.domain
            : 'domains' in item
              ? item.domains.join(', ')
              : providers[item.provider]}
        </div>
      </td>
      <td>
        {'record_type' in item ? (
          <>
            <span className='record-type'>{item.record_type}</span>
            <p className='mt-2 font-mono text-[11px] text-muted-foreground'>
              {item.status.ip ?? '—'}
            </p>
          </>
        ) : 'target' in item ? (
          <>
            <p className='font-mono text-[11px] break-all'>{item.target}</p>
            <p className='mt-2 text-[11px] text-muted-foreground'>
              {item.listener} · {item.certificate_id ? 'HTTP / HTTPS' : 'HTTP'}
            </p>
          </>
        ) : 'domains' in item ? (
          <>
            <p className='text-xs'>
              {item.imported
                ? 'PEM 导入'
                : item.staging
                  ? 'ACME · 测试'
                  : 'ACME · 正式'}
            </p>
            <p className='mt-2 text-[11px] text-muted-foreground'>
              {item.imported
                ? '手动更新'
                : item.auto_renew
                  ? '自动续期'
                  : '手动续期'}
            </p>
          </>
        ) : (
          <p className='text-xs text-muted-foreground'>凭据已加密</p>
        )}
      </td>
      <td>
        <StatusBadge resource={item} />
        {item.status.message && item.status.state === 'error' && (
          <p className='mt-2 max-w-56 text-[11px] text-destructive break-words'>
            {item.status.message}
          </p>
        )}
      </td>
      <td className='text-[11px] text-muted-foreground whitespace-nowrap'>
        {'domains' in item ? (
          <>
            <p>{dateOnly(item.status.expires_at)}</p>
            {item.status.expires_at &&
              item.status.expires_at - Date.now() / 1000 < 30 * 86400 && (
                <p className='mt-1 text-amber-600'>即将到期</p>
              )}
          </>
        ) : 'interval' in item ? (
          <>
            <p>{dateTime(item.status.last_attempt)}</p>
            <p className='mt-1'>{item.interval} 秒 / 次</p>
          </>
        ) : 'target' in item ? (
          'enabled' in item ? (
            <Switch
              aria-label={`${item.enabled ? '停用' : '启用'} ${item.name}`}
              checked={item.enabled}
              disabled={toggle.isPending || running}
              onCheckedChange={(enabled) => toggle.mutate(enabled)}
            />
          ) : null
        ) : (
          '—'
        )}
      </td>
      <td>
        <div className='flex items-center justify-end gap-1'>
          {'enabled' in item && kind === 'ddns' && (
            <Switch
              className='mr-3'
              aria-label={`${item.enabled ? '停用' : '启用'} ${item.name}`}
              checked={item.enabled}
              disabled={toggle.isPending || running}
              onCheckedChange={(enabled) => toggle.mutate(enabled)}
            />
          )}
          {kind === 'ddns' || kind === 'certificates' ? (
            <Button
              variant='ghost'
              size='icon'
              aria-label={
                kind === 'certificates' && 'imported' in item && item.imported
                  ? `上传 ${item.name} 证书`
                  : `执行 ${item.name}`
              }
              title={
                kind === 'ddns'
                  ? '立即同步'
                  : 'imported' in item && item.imported
                    ? '上传证书'
                    : '申请 / 续期'
              }
              disabled={running || run.isPending}
              onClick={() =>
                'imported' in item && item.imported ? upload() : run.mutate()
              }
            >
              {running || run.isPending ? (
                <LoaderCircle size={15} className='animate-spin' />
              ) : 'imported' in item && item.imported ? (
                <Upload size={15} />
              ) : (
                <RefreshCw size={15} />
              )}
            </Button>
          ) : null}
          <Button
            variant='ghost'
            size='icon'
            aria-label={`编辑 ${item.name}`}
            title='编辑'
            onClick={edit}
            disabled={running}
          >
            <Pencil size={14} />
          </Button>
          <Button
            variant='ghost'
            size='icon'
            aria-label={`删除 ${item.name}`}
            title='删除'
            className='text-muted-foreground hover:text-destructive'
            onClick={remove}
            disabled={running}
          >
            <Trash2 size={14} />
          </Button>
        </div>
      </td>
    </tr>
  );
}
