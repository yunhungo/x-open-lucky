/**
 * @created 2026-10-13
 * @description 就地展示分组操作进度、错误及明确的刷新恢复结果。
 * @author yunhungo
 */
import { LoaderCircle } from 'lucide-react';
import { errorText } from '@/lib/api';
export function GroupFeedback({
  pending,
  error,
  refreshError,
  refreshing,
  refresh,
  label = '刷新分组',
}: {
  pending: boolean;
  error: unknown;
  refreshError: unknown;
  refreshing: boolean;
  refresh: () => Promise<void>;
  label?: string;
}) {
  if (pending)
    return (
      <p role='status' className='group-manager__feedback'>
        <LoaderCircle size={14} className='animate-spin' />
        保存中…
      </p>
    );
  if (!error) return null;
  return (
    <div role='alert' className='group-manager__feedback text-destructive'>
      <span>{errorText(error)}</span>
      <button
        type='button'
        disabled={refreshing}
        onClick={() => void refresh()}
      >
        {refreshing ? '刷新中…' : label}
      </button>
      {Boolean(refreshError) && <span>刷新失败：{errorText(refreshError)}</span>}
    </div>
  );
}
