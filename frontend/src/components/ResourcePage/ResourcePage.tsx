/**
 * @created 2026-10-11
 * @description 管理可搜索的资源列表和受保护删除流程。
 * @author yunhungo
 */
import { useEffect, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { GroupSelect } from '@/components/GroupSelect/GroupSelect';
import { GroupManager } from '@/components/GroupManager/GroupManager';
import { WebGroupList } from '@/components/WebGroupList/WebGroupList';
import { BulkGroupActions } from '@/components/BulkGroupActions/BulkGroupActions';
import { TrafficPanel } from '@/components/TrafficPanel/TrafficPanel';
import { useGroups, useGroupStats, refreshGroups } from '@/lib/groups-api';
import { UNASSIGNED, inGroup, groupLabel } from '@/lib/groups';
import { ResourceTable } from './ResourceTable';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { Plus, Search, SlidersHorizontal, LoaderCircle } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Sheet } from '@/components/ui/sheet';
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogCancel,
} from '@/components/ui/alert-dialog';
import { ResourceEditor } from '@/components/ResourceEditor/ResourceEditor';
import { CertificateImport } from '@/components/CertificateImport/CertificateImport';
import { QueryState } from '@/components/QueryState/QueryState';
import { errorText, okSchema, request, useResources } from '@/lib/api';
import {
  names,
  type Kind,
  type Resource,
  type WebService,
} from '@/lib/contracts';
import './ResourcePage.scss';
type Editor =
  { mode: 'edit'; item?: Resource } | { mode: 'import'; item: Resource };
export function ResourcePage({
  kind,
  embedded = false,
}: {
  kind: Kind;
  embedded?: boolean;
}) {
  const resources = useResources(kind);
  const groups = useGroups(kind === 'web');
  const stats = useGroupStats(kind === 'web');
  const [params, setParams] = useSearchParams();
  const group = kind === 'web' ? (params.get('group') ?? '') : '';
  const [managing, setManaging] = useState(false);
  const [selected, setSelected] = useState<Record<string, number>>({});
  const [assigning, setAssigning] = useState(false);
  useEffect(() => {
    setSelected({});
  }, [group]);
  const changeGroup = (value: string) => {
    const next = new URLSearchParams(params);
    if (value) next.set('group', value);
    else next.delete('group');
    setParams(next, { replace: true });
    setSelected({});
  };
  const selection = {
    selected,
    disabled: assigning,
    change: (items: Resource[], checked: boolean) => {
      setSelected((current) => {
        const next = { ...current };
        for (const item of items) {
          if (checked) next[item.id] ??= item.revision;
          else delete next[item.id];
        }
        return next;
      });
    },
  };
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState('all');
  const [editor, setEditor] = useState<Editor | null>(null);
  const [deleting, setDeleting] = useState<Resource | null>(null);
  const queryClient = useQueryClient();
  const remove = useMutation({
    mutationFn: (id: string) =>
      request(`/resources/${kind}/${id}`, okSchema, { method: 'DELETE' }),
    onSuccess: () => {
      if (kind === 'web') void refreshGroups(queryClient);
      setSelected({});
      void queryClient.invalidateQueries({ queryKey: ['resources'] });
      void queryClient.invalidateQueries({ queryKey: ['overview'] });
      toast.success('已删除');
      setDeleting(null);
    },
  });
  const filtered = resources.data?.filter(
    (item) =>
      (kind !== 'web' ||
        ('group_id' in item && inGroup(item.group_id, group))) &&
      [
        item.name,
        'domain' in item
          ? item.domain
          : 'domains' in item
            ? item.domains.join(' ')
            : item.provider,
        'target' in item ? item.target : '',
      ]
        .join(' ')
        .toLowerCase()
        .includes(query.trim().toLowerCase()) &&
      (filter === 'all' ||
        (filter === 'error' && item.status.state === 'error') ||
        (filter === 'disabled' && 'enabled' in item && !item.enabled) ||
        (filter === 'enabled' && 'enabled' in item && item.enabled)),
  );
  const actions = {
    edit: (item: Resource) => setEditor({ mode: 'edit', item }),
    upload: (item: Resource) => setEditor({ mode: 'import', item }),
    remove: (item: Resource) => {
      remove.reset();
      setDeleting(item);
    },
  };
  const webItems = (filtered ?? []).filter(
    (item): item is WebService => 'group_id' in item,
  );
  return (
    <>
      {embedded ? (
        <h2 className='sr-only'>{names[kind]}</h2>
      ) : (
        <h1 className='sr-only'>{names[kind]}</h1>
      )}
      <div className='resource-toolbar'>
        <div className='resource-search'>
          <Search size={16} />
          <input
            aria-label={`搜索${names[kind]}`}
            type='search'
            placeholder='搜索名称、域名…'
            value={query}
            disabled={assigning}
            onChange={(e) => {
              setQuery(e.target.value);
              setSelected({});
            }}
          />
        </div>
        <div className='resource-toolbar__actions'>
          {kind === 'web' && (
            <>
              <GroupSelect
                groups={groups.data ?? []}
                value={group}
                onChange={changeGroup}
                label='筛选分组'
                filter
                disabled={assigning || !groups.data}
              />
              <Button
                variant='outline'
                onClick={() => setManaging(true)}
                disabled={assigning}
              >
                管理分组
              </Button>
            </>
          )}
          {kind !== 'credentials' && (
            <div className='flex items-center gap-2'>
              <SlidersHorizontal
                size={14}
                className='text-muted-foreground hidden sm:block'
              />
              <select
                aria-label='筛选状态'
                className='filter-select'
                value={filter}
                disabled={assigning}
                onChange={(e) => {
                  setFilter(e.target.value);
                  setSelected({});
                }}
              >
                <option value='all'>全部状态</option>
                {kind !== 'certificates' && (
                  <>
                    <option value='enabled'>已启用</option>
                    <option value='disabled'>已停用</option>
                  </>
                )}
                <option value='error'>异常</option>
              </select>
            </div>
          )}
          <Button onClick={() => setEditor({ mode: 'edit' })}>
            <Plus size={15} />
            添加
          </Button>
        </div>
      </div>
      {kind === 'web' && (groups.isError || stats.isError) && (
        <p role='alert' className='mb-4 text-sm text-destructive'>
          {errorText(groups.error ?? stats.error)}{' '}
          <button
            onClick={() => {
              void groups.refetch();
              void stats.refetch();
            }}
          >
            重试
          </button>
        </p>
      )}
      {kind === 'web' && Object.keys(selected).length > 0 && (
        <BulkGroupActions
          selected={selected}
          groups={groups.data ?? []}
          clear={() => setSelected({})}
          pending={setAssigning}
        />
      )}
      {group && (
        <TrafficPanel
          groupId={group}
          title={`${group === UNASSIGNED ? '未分组' : groupLabel(groups.data ?? [], group)} · 访问情况`}
        />
      )}
      <div
        className={
          kind === 'web' && filtered?.length ? '' : 'panel overflow-hidden'
        }
      >
        {resources.isPending ||
        resources.isError ||
        (kind === 'web' && !groups.data) ? (
          <div className='p-6'>
            <QueryState
              error={resources.error ?? groups.error}
              retry={() => {
                void resources.refetch();
                if (kind === 'web') void groups.refetch();
              }}
            />
          </div>
        ) : filtered?.length ? (
          kind === 'web' ? (
            <WebGroupList
              items={webItems}
              groups={groups.data ?? []}
              stats={stats.data?.groups ?? []}
              actions={actions}
              selection={selection}
              expanded={Boolean(query || group || filter !== 'all')}
            />
          ) : (
            <ResourceTable kind={kind} items={filtered} actions={actions} />
          )
        ) : (
          <div className='resource-empty'>
            <span>
              {query || group || filter !== 'all'
                ? '没有匹配的结果'
                : `暂无${names[kind]}`}
            </span>
            {query || group || filter !== 'all' ? (
              <Button
                variant='link'
                onClick={() => {
                  setQuery('');
                  setFilter('all');
                  changeGroup('');
                }}
              >
                清除筛选
              </Button>
            ) : (
              <Button
                variant='outline'
                onClick={() => setEditor({ mode: 'edit' })}
              >
                <Plus size={14} />
                添加{names[kind]}
              </Button>
            )}
          </div>
        )}
      </div>
      <Sheet open={managing} onOpenChange={setManaging}>
        {managing && <GroupManager />}
      </Sheet>
      <Sheet
        open={Boolean(editor)}
        onOpenChange={(open) => {
          if (!open) setEditor(null);
        }}
      >
        {editor?.mode === 'edit' ? (
          <ResourceEditor
            key={`${kind}-${editor.item?.id ?? 'new'}`}
            kind={kind}
            item={editor.item}
            defaultGroupId={group === UNASSIGNED ? '' : group}
            close={() => setEditor(null)}
          />
        ) : editor?.mode === 'import' ? (
          <CertificateImport item={editor.item} close={() => setEditor(null)} />
        ) : null}
      </Sheet>
      <AlertDialog
        open={Boolean(deleting)}
        onOpenChange={(open) => {
          if (!open && !remove.isPending) setDeleting(null);
        }}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>删除 {deleting?.name}？</AlertDialogTitle>
            <AlertDialogDescription>
              配置将被删除。关联资源需要先解除绑定。
            </AlertDialogDescription>
          </AlertDialogHeader>
          {remove.isError && (
            <p role='alert' className='text-sm text-destructive'>
              {errorText(remove.error)}
            </p>
          )}
          <AlertDialogFooter>
            <AlertDialogCancel disabled={remove.isPending}>
              取消
            </AlertDialogCancel>
            <Button
              variant='destructive'
              disabled={remove.isPending}
              onClick={() => {
                if (deleting) remove.mutate(deleting.id);
              }}
            >
              {remove.isPending && (
                <LoaderCircle size={15} className='animate-spin' />
              )}
              删除
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
