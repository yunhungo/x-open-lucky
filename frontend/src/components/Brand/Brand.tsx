/**
 * @created 2026-10-11
 * @description 显示 OpenLucky 标识。
 * @author yunhungo
 */
import { Clover } from 'lucide-react';
export function Brand() {
  return (
    <div className='flex items-center gap-3'>
      <span className='grid size-9 place-items-center rounded-xl bg-primary text-primary-foreground'>
        <Clover size={23} strokeWidth={1.7} />
      </span>
      <span className='text-[17px] font-semibold tracking-tight'>
        OpenLucky
      </span>
    </div>
  );
}
