/**
 * @created 2026-10-11
 * @description 持久化深浅主题并尊重系统色彩偏好。
 * @author yunhungo
 */
import { useEffect, useState } from 'react';
export function useTheme() {
  const [dark, setDark] = useState(() => {
    try {
      const saved = localStorage.getItem('openlucky.theme');
      if (saved) return saved === 'dark';
    } catch {
      /* 本次主题仍然有效。 */
    }
    return matchMedia('(prefers-color-scheme: dark)').matches;
  });
  useEffect(() => {
    document.documentElement.classList.toggle('dark', dark);
    document
      .querySelector('meta[name=theme-color]')
      ?.setAttribute('content', dark ? '#14181f' : '#f6f7f9');
    try {
      localStorage.setItem('openlucky.theme', dark ? 'dark' : 'light');
    } catch {
      /* 不阻止主题切换。 */
    }
  }, [dark]);
  return { dark, toggle: () => setDark((v) => !v) };
}
