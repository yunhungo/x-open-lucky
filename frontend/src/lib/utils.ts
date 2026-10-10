/**
 * @created 2026-10-11
 * @description 合并 shadcn 和应用的 Tailwind 样式。
 * @author yunhungo
 */
import { clsx, type ClassValue } from 'clsx';
import { twMerge } from 'tailwind-merge';
export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
