/**
 * @created 2026-10-11
 * @description 展示可恢复的加载或请求失败状态。
 * @author yunhungo
 */
import { AlertCircle } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Skeleton } from '@/components/ui/skeleton';
import { errorText } from '@/lib/api';
export function QueryState({
  error,
  retry,
}: {
  error?: unknown;
  retry?: () => void;
}) {
  if (error)
    return (
      <div
        role='alert'
        className='flex min-h-48 flex-col items-center justify-center gap-4 text-sm'
      >
        <AlertCircle className='text-destructive' size={24} />
        <p>{errorText(error)}</p>
        <Button variant='outline' onClick={retry}>
          重试
        </Button>
      </div>
    );
  return (
    <div aria-label='正在加载' className='space-y-4'>
      <Skeleton className='h-10 w-44' />
      <Skeleton className='h-28 w-full' />
      <Skeleton className='h-28 w-full' />
    </div>
  );
}
