/**
 * @created 2026-10-13
 * @description 复用资源表格并提供当前分组的服务多选。
 * @author yunhungo
 */
import { ResourceRow } from '@/components/ResourceRow/ResourceRow';
import type { Kind, Resource } from '@/lib/contracts';
export type TableActions = {
  edit: (item: Resource) => void;
  remove: (item: Resource) => void;
  upload: (item: Resource) => void;
};
export type TableSelection = {
  selected: Record<string, number>;
  disabled?: boolean;
  change: (items: Resource[], checked: boolean) => void;
};
export function ResourceTable({
  kind,
  items,
  actions,
  selection,
  label = '选择当前列表全部服务',
}: {
  kind: Kind;
  items: Resource[];
  actions: TableActions;
  selection?: TableSelection;
  label?: string;
}) {
  const labels = {
    ddns: ['域名', '记录 / IP', '状态', '最近检查'],
    web: ['服务 / 域名', '目标地址', '状态', '启用'],
    certificates: ['证书 / 域名', '来源', '状态', '到期日期'],
    credentials: ['名称 / 服务商', '存储', '状态', ''],
  };
  const count = selection
    ? items.filter((item) => item.id in selection.selected).length
    : 0;
  return (
    <div className='overflow-x-auto'>
      <table
        className={`resource-table${selection ? ' resource-table--selectable' : ''}`}
      >
        <thead>
          <tr>
            {selection && (
              <th className='resource-table__select'>
                <input
                  type='checkbox'
                  aria-label={label}
                  disabled={selection.disabled}
                  checked={count === items.length && count > 0}
                  ref={(node) => {
                    if (node)
                      node.indeterminate = count > 0 && count < items.length;
                  }}
                  onChange={(e) => selection.change(items, e.target.checked)}
                />
                <span className='resource-table__mobile-label'>全选本组</span>
              </th>
            )}
            {labels[kind].map((label, index) => (
              <th key={index}>{label}</th>
            ))}
            <th>操作</th>
          </tr>
        </thead>
        <tbody>
          {items.map((item) => (
            <ResourceRow
              key={item.id}
              kind={kind}
              item={item}
              edit={() => actions.edit(item)}
              upload={() => actions.upload(item)}
              remove={() => actions.remove(item)}
              selection={
                selection
                  ? {
                      checked: item.id in selection.selected,
                      disabled: selection.disabled,
                      change: (checked) => selection.change([item], checked),
                    }
                  : undefined
              }
            />
          ))}
        </tbody>
      </table>
    </div>
  );
}
