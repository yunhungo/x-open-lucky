/**
 * @created 2026-10-11
 * @description 管理可搜索的资源列表和受保护删除流程。
 * @author yunhungo
 */
import { useState } from 'react';
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
import { ResourceRow } from '@/components/ResourceRow/ResourceRow';
import { ResourceEditor } from '@/components/ResourceEditor/ResourceEditor';
import { CertificateImport } from '@/components/CertificateImport/CertificateImport';
import { QueryState } from '@/components/QueryState/QueryState';
import { errorText, okSchema, request, useResources } from '@/lib/api';
import { names, type Kind, type Resource } from '@/lib/contracts';
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
  const [query, setQuery] = useState('');
  const [filter, setFilter] = useState('all');
  const [editor, setEditor] = useState<Editor | null>(null);
  const [deleting, setDeleting] = useState<Resource | null>(null);
  const queryClient = useQueryClient();
  const remove = useMutation({
    mutationFn: (id: string) =>
      request(`/resources/${kind}/${id}`, okSchema, { method: 'DELETE' }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['resources'] });
      void queryClient.invalidateQueries({ queryKey: ['overview'] });
      toast.success('已删除');
      setDeleting(null);
    },
  });
  const filtered = resources.data?.filter(
    (item) =>
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
  const labels = {
    ddns: ['域名', '记录 / IP', '状态', '最近检查'],
    web: ['服务 / 域名', '目标地址', '状态', '启用'],
    certificates: ['证书 / 域名', '来源', '状态', '到期日期'],
    credentials: ['名称 / 服务商', '存储', '状态', ''],
  };
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
            onChange={(e) => setQuery(e.target.value)}
          />
        </div>
        <div className='flex items-center gap-3 ml-auto'>
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
                onChange={(e) => setFilter(e.target.value)}
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
      <div className='panel overflow-hidden'>
        {resources.isPending || resources.isError ? (
          <div className='p-6'>
            <QueryState
              error={resources.error}
              retry={() => void resources.refetch()}
            />
          </div>
        ) : filtered?.length ? (
          <div className='overflow-x-auto'>
            <table className='resource-table'>
              <thead>
                <tr>
                  {labels[kind].map((label, i) => (
                    <th key={i}>{label}</th>
                  ))}
                  <th className='text-right'>操作</th>
                </tr>
              </thead>
              <tbody>
                {filtered.map((item) => (
                  <ResourceRow
                    key={item.id}
                    kind={kind}
                    item={item}
                    edit={() => setEditor({ mode: 'edit', item })}
                    upload={() => setEditor({ mode: 'import', item })}
                    remove={() => {
                      remove.reset();
                      setDeleting(item);
                    }}
                  />
                ))}
              </tbody>
            </table>
          </div>
        ) : (
          <div className='resource-empty'>
            <span>
              {query || filter !== 'all'
                ? '没有匹配的结果'
                : `暂无${names[kind]}`}
            </span>
            {query || filter !== 'all' ? (
              <Button
                variant='link'
                onClick={() => {
                  setQuery('');
                  setFilter('all');
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
