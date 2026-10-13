/**
 * @created 2026-10-13
 * @description 在受保护抽屉中创建、重命名、排序和删除服务分组。
 * @author yunhungo
 */
import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import {
  ArrowDown,
  ArrowUp,
  Check,
  Pencil,
  Plus,
  Trash2,
  X,
} from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
} from '@/components/ui/sheet';
import { QueryState } from '@/components/QueryState/QueryState';
import { errorText, idSchema, okSchema, request } from '@/lib/api';
import { refreshGroups, useGroups, useGroupStats } from '@/lib/groups-api';
import type { ServiceGroup } from '@/lib/groups';
import { GroupFeedback } from './GroupFeedback';
import { GroupDeleteDialog } from './GroupDeleteDialog';
import './GroupManager.scss';
type Action =
  | { kind: 'create'; name: string }
  | { kind: 'rename'; group: ServiceGroup; name: string }
  | { kind: 'order'; groups: ServiceGroup[] };
export function GroupManager() {
  const groups = useGroups();
  const stats = useGroupStats();
  const client = useQueryClient();
  const [editing, setEditing] = useState<ServiceGroup | null>(null);
  const [name, setName] = useState('');
  const [newName, setNewName] = useState('');
  const [deleting, setDeleting] = useState<ServiceGroup | null>(null);
  const [deletePending, setDeletePending] = useState(false);
  const [refreshError, setRefreshError] = useState<unknown>(null);
  const [refreshing, setRefreshing] = useState(false);
  const mutation = useMutation({
    onMutate: () => setRefreshError(null),
    mutationFn: async (action: Action) => {
      if (action.kind === 'order') {
        return request('/groups/order', okSchema, {
          method: 'PUT',
          body: JSON.stringify({
            groups: action.groups.map(({ id, revision }) => ({ id, revision })),
          }),
        });
      }
      return request(
        `/groups${action.kind === 'rename' ? `/${action.group.id}` : ''}`,
        idSchema,
        {
          method: action.kind === 'create' ? 'POST' : 'PUT',
          body: JSON.stringify({
            name: action.name,
            ...(action.kind === 'rename'
              ? { revision: action.group.revision }
              : {}),
          }),
        },
      );
    },
    onSuccess: async (_, action) => {
      await refreshGroups(client);
      if (action.kind === 'create') setNewName('');
      if (action.kind === 'rename') setEditing(null);
      toast.success(
        action.kind === 'order'
          ? '分组顺序已更新'
          : action.kind === 'create'
            ? '分组已创建'
            : '分组已保存',
      );
    },
  });
  const busy = mutation.isPending || deletePending || refreshing;
  const reload = async () => {
    setRefreshing(true);
    const result = await groups.refetch();
    setRefreshing(false);
    if (!result.isSuccess) {
      setRefreshError(result.error);
      return;
    }
    if (editing) {
      const latest = result.data.find((group) => group.id === editing.id);
      setEditing(latest ?? null);
      if (latest) setName(latest.name);
    }
    setRefreshError(null);
    mutation.reset();
  };
  const feedback = {
    pending: mutation.isPending,
    error: mutation.error,
    refreshError,
    refreshing,
    refresh: reload,
  };

  const move = (index: number, direction: number) => {
    const ordered = [...(groups.data ?? [])];
    const item = ordered[index];
    const next = ordered[index + direction];
    if (!item || !next) return;
    ordered[index] = next;
    ordered[index + direction] = item;
    mutation.mutate({ kind: 'order', groups: ordered });
  };
  return (
    <SheetContent
      className='group-manager'
      showCloseButton={!busy}
      onEscapeKeyDown={(e) => {
        if (busy || deleting) e.preventDefault();
      }}
      onInteractOutside={(e) => {
        if (busy || deleting) e.preventDefault();
      }}
    >
      <SheetHeader>
        <SheetTitle>管理分组</SheetTitle>
        <SheetDescription className='sr-only'>
          创建、编辑、排序和删除服务分组
        </SheetDescription>
      </SheetHeader>
      {mutation.variables?.kind === 'order' && (
        <div className='group-manager__fixed-feedback'>
          <GroupFeedback {...feedback} />
        </div>
      )}
      {groups.data && groups.isError && !mutation.isError && (
        <p role='alert' className='group-manager__query-error text-destructive'>
          {errorText(groups.error)}{' '}
          <button type='button' onClick={() => void groups.refetch()}>
            重试
          </button>
        </p>
      )}
      <div className='group-manager__body'>
        {!groups.data ? (
          <QueryState
            error={groups.error}
            retry={() => void groups.refetch()}
          />
        ) : (
          <ul className='group-manager__list'>
            {groups.data.map((group, index) => (
              <li key={group.id}>
                {editing?.id === group.id ? (
                  <div className='group-manager__editing'>
                    <form
                      onSubmit={(e) => {
                        e.preventDefault();
                        if (editing)
                          mutation.mutate({
                            kind: 'rename',
                            group: editing,
                            name,
                          });
                      }}
                    >
                      <Input
                        autoFocus
                        aria-label='分组名称'
                        maxLength={40}
                        required
                        value={name}
                        disabled={busy}
                        onChange={(e) => setName(e.target.value)}
                      />
                      <Button
                        size='icon'
                        variant='ghost'
                        type='submit'
                        aria-label='保存分组名称'
                        disabled={busy}
                      >
                        <Check size={15} />
                      </Button>
                      <Button
                        size='icon'
                        variant='ghost'
                        type='button'
                        aria-label='取消重命名'
                        disabled={busy}
                        onClick={() => {
                          setEditing(null);
                          mutation.reset();
                        }}
                      >
                        <X size={15} />
                      </Button>
                    </form>
                    {mutation.variables?.kind === 'rename' && (
                      <GroupFeedback {...feedback} label='载入最新名称' />
                    )}
                  </div>
                ) : (
                  <>
                    <span className='group-manager__name'>
                      {group.name}
                      <small>
                        {stats.data?.groups.find((s) => s.id === group.id)
                          ?.services ?? '—'}{' '}
                        个服务
                      </small>
                    </span>
                    <div className='group-manager__actions'>
                      <Button
                        size='icon'
                        variant='ghost'
                        aria-label={`上移 ${group.name}`}
                        disabled={busy || Boolean(editing) || index === 0}
                        onClick={() => move(index, -1)}
                      >
                        <ArrowUp size={14} />
                      </Button>
                      <Button
                        size='icon'
                        variant='ghost'
                        aria-label={`下移 ${group.name}`}
                        disabled={
                          busy ||
                          Boolean(editing) ||
                          index === (groups.data?.length ?? 0) - 1
                        }
                        onClick={() => move(index, 1)}
                      >
                        <ArrowDown size={14} />
                      </Button>
                      <Button
                        size='icon'
                        variant='ghost'
                        aria-label={`编辑分组 ${group.name}`}
                        disabled={busy || Boolean(editing)}
                        onClick={() => {
                          mutation.reset();
                          setEditing(group);
                          setName(group.name);
                        }}
                      >
                        <Pencil size={14} />
                      </Button>
                      <Button
                        size='icon'
                        variant='ghost'
                        aria-label={`删除分组 ${group.name}`}
                        disabled={busy || Boolean(editing)}
                        onClick={() => setDeleting(group)}
                      >
                        <Trash2 size={14} />
                      </Button>
                    </div>
                  </>
                )}
              </li>
            ))}
          </ul>
        )}
        <form
          className='group-manager__create'
          onSubmit={(e) => {
            e.preventDefault();
            mutation.mutate({ kind: 'create', name: newName });
          }}
        >
          <Input
            aria-label='新分组名称'
            placeholder='新分组名称'
            maxLength={40}
            required
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            disabled={busy || Boolean(editing)}
          />
          <Button
            type='submit'
            disabled={
              busy ||
              Boolean(editing) ||
              !groups.data ||
              groups.data.length >= 64
            }
          >
            <Plus size={15} />
            创建
          </Button>
        </form>
        {mutation.variables?.kind === 'create' && (
          <GroupFeedback {...feedback} />
        )}
      </div>
      <GroupDeleteDialog
        group={deleting}
        count={stats.data?.groups.find((s) => s.id === deleting?.id)?.services}
        close={() => setDeleting(null)}
        pending={setDeletePending}
      />
    </SheetContent>
  );
}
