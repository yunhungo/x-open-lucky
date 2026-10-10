/**
 * @created 2026-10-11
 * @description 提供六个核心页面的可访问导航。
 * @author yunhungo
 */
import {
  LayoutDashboard,
  Globe2,
  Network,
  ShieldCheck,
  ScrollText,
  Settings2,
} from 'lucide-react';
import { NavLink } from 'react-router-dom';
export const navigation = [
  { path: '/', name: '概览', icon: LayoutDashboard },
  { path: '/ddns', name: '动态域名', icon: Globe2 },
  { path: '/web', name: 'Web 服务', icon: Network },
  { path: '/certificates', name: 'SSL 证书', icon: ShieldCheck },
  { path: '/logs', name: '运行日志', icon: ScrollText },
  { path: '/settings', name: '设置', icon: Settings2 },
];
export function Navigation({ onNavigate }: { onNavigate?: () => void }) {
  return (
    <nav aria-label='主导航' className='navigation'>
      {navigation.map((item) => (
        <NavLink
          key={item.path}
          to={item.path}
          end={item.path === '/'}
          onClick={onNavigate}
          className={({ isActive }) =>
            isActive ? 'navigation__link is-active' : 'navigation__link'
          }
        >
          <item.icon size={18} strokeWidth={1.65} />
          <span>{item.name}</span>
        </NavLink>
      ))}
    </nav>
  );
}
