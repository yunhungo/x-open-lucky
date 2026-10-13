/**
 * @created 2026-10-13
 * @description 明确删除分组的成员迁移结果，并保留失败时的重试路径。
 * @author yunhungo
 */
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { LoaderCircle } from 'lucide-react';
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogCancel,
} from '@/components/ui/alert-dialog';
import { Button } from '@/components/ui/button';
import { errorText, okSchema, request } from '@/lib/api';
import { refreshGroups } from '@/lib/groups-api';
import type { ServiceGroup } from '@/lib/groups';
export function GroupDeleteDialog({
  group,
  count,
  close,
  pending,
}: {
  group: ServiceGroup | null;
  count?: number;
  close: () => void;
  pending: (value: boolean) => void;
}) {
  const client = useQueryClient();
  const mutation = useMutation({
    mutationFn: (item: ServiceGroup) =>
      request(`/groups/${item.id}`, okSchema, {
        method: 'DELETE',
        body: JSON.stringify({ revision: item.revision }),
      }),
    onMutate: () => pending(true),
    onSuccess: async () => {
      await refreshGroups(client);
      toast.success('分组已删除，成员已转为未分组');
      close();
    },
    onSettled: () => pending(false),
  });
  return (
    <AlertDialog
      open={Boolean(group)}
      onOpenChange={(open) => {
        if (!open && !mutation.isPending) {
          close();
          mutation.reset();
        }
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>删除分组 {group?.name}？</AlertDialogTitle>
          <AlertDialogDescription>
            {count === undefined ? '组内服务' : `${count} 个服务`}
            将转为未分组，服务配置保留。
          </AlertDialogDescription>
        </AlertDialogHeader>
        {mutation.isError && (
          <p role='alert' className='text-sm text-destructive'>
            {errorText(mutation.error)}{' '}
            <button
              className='underline'
              onClick={async () => {
                await refreshGroups(client);
                close();
                mutation.reset();
              }}
            >
              刷新并重新选择
            </button>
          </p>
        )}
        <AlertDialogFooter>
          <AlertDialogCancel disabled={mutation.isPending}>
            取消
          </AlertDialogCancel>
          <Button
            variant='destructive'
            disabled={mutation.isPending}
            onClick={() => {
              if (group) mutation.mutate(group);
            }}
          >
            {mutation.isPending && (
              <LoaderCircle size={14} className='animate-spin' />
            )}
            删除分组
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
