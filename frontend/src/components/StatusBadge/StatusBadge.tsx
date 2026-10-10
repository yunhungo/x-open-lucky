/**
 * @created 2026-10-11
 * @description 以文字和图标呈现真实资源运行状态。
 * @author yunhungo
 */
import {
  CircleCheck,
  CircleDashed,
  CirclePause,
  LoaderCircle,
  TriangleAlert,
} from 'lucide-react';
import type { Resource } from '@/lib/contracts';
export function StatusBadge({ resource }: { resource: Resource }) {
  const disabled = 'enabled' in resource && !resource.enabled;
  const state = disabled
    ? 'disabled'
    : 'provider' in resource
      ? 'ready'
      : (resource.status.state ?? ('target' in resource ? 'ready' : 'idle'));
  const info = {
    ready: ['运行中', CircleCheck, 'text-primary bg-primary/8'],
    idle: ['待配置', CircleDashed, 'text-muted-foreground bg-muted'],
    running: ['执行中', LoaderCircle, 'text-blue-600 bg-blue-500/8'],
    error: ['异常', TriangleAlert, 'text-destructive bg-destructive/8'],
    disabled: ['已停用', CirclePause, 'text-muted-foreground bg-muted'],
  } as const;
  const [label, Icon, color] = info[state];
  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded-md px-2 py-1 text-[11px] font-medium ${color}`}
    >
      <Icon size={12} className={state === 'running' ? 'animate-spin' : ''} />
      {'expires_at' in resource.status && state === 'ready'
        ? '有效'
        : 'provider' in resource
          ? '已保存'
          : state === 'ready' && 'target' in resource
            ? '已启用'
            : state === 'ready' && 'record_type' in resource
              ? '已同步'
              : label}
    </span>
  );
}
