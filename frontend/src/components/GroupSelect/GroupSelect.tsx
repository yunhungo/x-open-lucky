/**
 * @created 2026-10-13
 * @description 在筛选与编辑中一致展示自定义分组及未分组状态。
 * @author yunhungo
 */
import { UNASSIGNED, type ServiceGroup } from '@/lib/groups';
export function GroupSelect({
  groups,
  value,
  onChange,
  label,
  filter = false,
  disabled = false,
  className = 'filter-select',
}: {
  groups: ServiceGroup[];
  value: string;
  onChange: (value: string) => void;
  label: string;
  filter?: boolean;
  disabled?: boolean;
  className?: string;
}) {
  const valid =
    !value ||
    (filter && value === UNASSIGNED) ||
    groups.some((g) => g.id === value);
  return (
    <select
      aria-label={label}
      value={value}
      onChange={(event) => onChange(event.target.value)}
      disabled={disabled}
      className={className}
    >
      {filter && <option value=''>全部分组</option>}
      {groups.map((group) => (
        <option key={group.id} value={group.id}>
          {group.name}
        </option>
      ))}
      <option value={filter ? UNASSIGNED : ''}>未分组</option>
      {!valid && (
        <option value={value} disabled>
          分组已删除
        </option>
      )}
    </select>
  );
}
