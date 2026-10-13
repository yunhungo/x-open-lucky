/**
 * @created 2026-10-13
 * @description 逐行编辑请求头操作并插入动态变量或条件示例。
 * @author yunhungo
 */
import { Plus, X } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  headerVariables,
  parseHeaderRows,
  type HeaderRow,
} from '@/lib/header-rules';
import './HeaderRulesEditor.scss';
export function HeaderRulesEditor({
  value,
  onChange,
  disabled,
  response = false,
}: {
  value: string;
  onChange: (value: string) => void;
  disabled: boolean;
  response?: boolean;
}) {
  const label = response ? '响应头' : '请求头';
  const rows = parseHeaderRows(value);
  const update = (id: string, patch: Partial<HeaderRow>) =>
    onChange(
      JSON.stringify(
        rows.map((row) => (row.id === id ? { ...row, ...patch } : row)),
      ),
    );
  return (
    <fieldset
      className='header-rules'
      disabled={disabled}
      aria-label={`自定义${label}`}
    >
      <div className='header-rules__heading'>
        <span>自定义{label}</span>
        <Button
          type='button'
          size='sm'
          variant='ghost'
          disabled={disabled || rows.length >= 32}
          onClick={() =>
            onChange(
              JSON.stringify([
                ...rows,
                { id: crypto.randomUUID(), name: '', action: 'set', value: '' },
              ]),
            )
          }
          aria-label={`添加${label}`}
        >
          <Plus size={14} />
          添加
        </Button>
      </div>
      {rows.map((row, index) => (
        <div key={row.id} className='header-rules__row'>
          <div className='header-rules__name'>
            <Input
              aria-label={`${label}名称 ${index + 1}`}
              placeholder='X-Custom'
              maxLength={128}
              value={row.name}
              onChange={(e) => update(row.id, { name: e.target.value })}
              disabled={disabled}
            />
            <select
              className='form-select'
              aria-label={`${label}操作 ${index + 1}`}
              value={row.action}
              disabled={disabled}
              onChange={(e) =>
                update(row.id, {
                  action: e.target.value === 'remove' ? 'remove' : 'set',
                })
              }
            >
              <option value='set'>设置</option>
              <option value='remove'>删除</option>
            </select>
            <Button
              type='button'
              variant='ghost'
              size='icon'
              disabled={disabled}
              aria-label={`移除${label}规则 ${index + 1}`}
              onClick={() =>
                onChange(
                  JSON.stringify(rows.filter((item) => item.id !== row.id)),
                )
              }
            >
              <X size={15} />
            </Button>
          </div>
          {row.action === 'set' && (
            <div className='header-rules__value'>
              <Input
                aria-label={`${label}值 ${index + 1}`}
                placeholder='固定值或 {{ request.host }}'
                maxLength={8192}
                value={row.value}
                onChange={(e) => update(row.id, { value: e.target.value })}
                disabled={disabled}
              />
              <select
                className='form-select'
                aria-label={`插入${label}变量 ${index + 1}`}
                value=''
                disabled={disabled}
                onChange={(e) =>
                  update(row.id, { value: row.value + e.target.value })
                }
              >
                <option value=''>插入变量</option>
                {headerVariables.map((variable) => (
                  <option value={variable.value} key={variable.label}>
                    {variable.label}
                  </option>
                ))}
                {response && (
                  <option value='{{ response.status }}'>响应状态码</option>
                )}
              </select>
            </div>
          )}
        </div>
      ))}
    </fieldset>
  );
}
