/**
 * @created 2026-10-13
 * @description 按顺序展示可折叠服务组并记住当前浏览器的折叠状态。
 * @author yunhungo
 */
import { useState } from 'react';
import { ChevronDown } from 'lucide-react';
import {
  ResourceTable,
  type TableActions,
  type TableSelection,
} from '@/components/ResourcePage/ResourceTable';
import type { WebService } from '@/lib/contracts';
import { type ServiceGroup, type GroupStat, UNASSIGNED } from '@/lib/groups';
import { milliseconds, number } from '@/lib/observability';
import './WebGroupList.scss';
export function WebGroupList({
  items,
  groups,
  stats,
  actions,
  selection,
  expanded,
}: {
  items: WebService[];
  groups: ServiceGroup[];
  stats: GroupStat[];
  actions: TableActions;
  selection: TableSelection;
  expanded: boolean;
}) {
  const [collapsed, setCollapsed] = useState<string[]>(() => {
    try {
      const saved: unknown = JSON.parse(
        localStorage.getItem('openlucky-collapsed-groups') ?? '[]',
      );
      return Array.isArray(saved)
        ? saved.filter((id): id is string => typeof id === 'string')
        : [];
    } catch {
      return [];
    }
  });
  const toggle = (id: string) => {
    const next = collapsed.includes(id)
      ? collapsed.filter((value) => value !== id)
      : [...collapsed, id];
    setCollapsed(next);
    try {
      localStorage.setItem('openlucky-collapsed-groups', JSON.stringify(next));
    } catch {
      /* 浏览器禁用存储时仍可折叠。 */
    }
  };
  const ordered = [...groups, { id: UNASSIGNED, name: '未分组' }];
  return (
    <div className='web-group-list'>
      {ordered.map((group) => {
        const members = items.filter(
          (item) => (item.group_id || UNASSIGNED) === group.id,
        );
        if (!members.length) return null;
        const stat = stats.find((item) => item.id === group.id);
        const open = expanded || !collapsed.includes(group.id);
        return (
          <section
            className='panel web-group'
            key={group.id}
            aria-label={`${group.name}服务组`}
          >
            <button
              type='button'
              className='web-group__heading'
              aria-expanded={open}
              aria-controls={`group-${group.id}`}
              onClick={() => toggle(group.id)}
              disabled={expanded}
            >
              <ChevronDown
                size={16}
                className={open ? '' : 'web-group__chevron--closed'}
              />
              <span className='web-group__name'>{group.name}</span>
              <span className='web-group__count'>{members.length}</span>
              {stat && (
                <span className='web-group__stats'>
                  24h 请求 {number(stat.requests)}
                  <span>响应 {milliseconds(stat.avg_duration_ms)}</span>
                  {stat.errors > 0 && (
                    <span className='text-destructive'>
                      {stat.errors} 个异常
                    </span>
                  )}
                </span>
              )}
            </button>
            {open && (
              <div id={`group-${group.id}`}>
                <ResourceTable
                  kind='web'
                  items={members}
                  actions={actions}
                  selection={selection}
                  label={`选择 ${group.name} 全部服务`}
                />
              </div>
            )}
          </section>
        );
      })}
    </div>
  );
}
