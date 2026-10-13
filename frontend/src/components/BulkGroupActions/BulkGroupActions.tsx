/**
 * @created 2026-10-13
 * @description 以选中时的服务版本批量归组，冲突时保留选择并提供刷新。
 * @author yunhungo
 */
import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { LoaderCircle } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { errorText, okSchema, request } from '@/lib/api';
import { refreshGroups } from '@/lib/groups-api';
import { UNASSIGNED, type ServiceGroup } from '@/lib/groups';
import './BulkGroupActions.scss';
export function BulkGroupActions({
  selected,
  groups,
  clear,
  pending,
}: {
  selected: Record<string, number>;
  groups: ServiceGroup[];
  clear: () => void;
  pending: (value: boolean) => void;
}) {
  const [target, setTarget] = useState('');
  const client = useQueryClient();
  const mutation = useMutation({
    mutationFn: () =>
      request('/groups/assign', okSchema, {
        method: 'PUT',
        body: JSON.stringify({
          group_id: target === UNASSIGNED ? '' : target,
          services: Object.entries(selected).map(([id, revision]) => ({
            id,
            revision,
          })),
        }),
      }),
    onMutate: () => pending(true),
    onSuccess: async () => {
      await refreshGroups(client);
      clear();
      toast.success('服务分组已更新');
    },
    onSettled: () => pending(false),
  });
  return (
    <div className='bulk-groups' role='region' aria-label='批量归组'>
      <span>已选 {Object.keys(selected).length} 项</span>
      <select
        aria-label='移动到分组'
        className='form-select'
        value={target}
        onChange={(e) => setTarget(e.target.value)}
        disabled={mutation.isPending}
      >
        <option value='' disabled>
          移动到分组…
        </option>
        {groups.map((group) => (
          <option key={group.id} value={group.id}>
            {group.name}
          </option>
        ))}
        <option value={UNASSIGNED}>未分组</option>
      </select>
      <Button
        size='sm'
        onClick={() => mutation.mutate()}
        disabled={!target || mutation.isPending}
      >
        {mutation.isPending && (
          <LoaderCircle size={14} className='animate-spin' />
        )}
        应用
      </Button>
      <Button
        size='sm'
        variant='ghost'
        disabled={mutation.isPending}
        onClick={clear}
      >
        取消选择
      </Button>
      {mutation.isError && (
        <p role='alert'>
          {errorText(mutation.error)}{' '}
          <button
            onClick={async () => {
              await refreshGroups(client);
              clear();
            }}
          >
            刷新并重新选择
          </button>
        </p>
      )}
    </div>
  );
}
