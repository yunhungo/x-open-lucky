/**
 * @created 2026-10-11
 * @description 切换登录状态并配置管理台路由。
 * @author yunhungo
 */
import { useQuery } from '@tanstack/react-query';
import { Toaster } from 'sonner';
import { Route, Routes, Navigate } from 'react-router-dom';
import { z } from 'zod';
import { ApiError, request } from '@/lib/api';
import { useTheme } from '@/hooks/useTheme';
import { Login } from '@/components/Login/Login';
import { Shell } from '@/components/Shell/Shell';
import { OverviewPage } from '@/components/OverviewPage/OverviewPage';
import { ResourcePage } from '@/components/ResourcePage/ResourcePage';
import { LogsPage } from '@/components/LogsPage/LogsPage';
import { SettingsPage } from '@/components/SettingsPage/SettingsPage';
import { QueryState } from '@/components/QueryState/QueryState';
export function App() {
  const { dark, toggle } = useTheme();
  const me = useQuery({
    queryKey: ['me'],
    queryFn: () => request('/auth/me', z.object({ username: z.string() })),
    retry: false,
    refetchInterval: 60000,
  });
  const expired = me.error instanceof ApiError && me.error.status === 401;
  const content = me.isPending ? (
    <div className='max-w-md mx-auto p-10'>
      <QueryState />
    </div>
  ) : !me.data || expired ? (
    me.error instanceof ApiError && me.error.status === 401 ? (
      <Login dark={dark} toggle={toggle} />
    ) : (
      <div className='max-w-md mx-auto p-10'>
        <QueryState error={me.error} retry={() => void me.refetch()} />
      </div>
    )
  ) : (
    <Routes>
      <Route
        element={
          <Shell username={me.data.username} dark={dark} toggle={toggle} />
        }
      >
        <Route index element={<OverviewPage />} />
        <Route path='/ddns' element={<ResourcePage key='ddns' kind='ddns' />} />
        <Route path='/web' element={<ResourcePage key='web' kind='web' />} />
        <Route
          path='/certificates'
          element={<ResourcePage key='certificates' kind='certificates' />}
        />
        <Route path='/logs' element={<LogsPage />} />
        <Route path='/settings/*' element={<SettingsPage />} />
        <Route path='*' element={<Navigate to='/' replace />} />
      </Route>
    </Routes>
  );
  return (
    <>
      {content}
      <Toaster
        theme={dark ? 'dark' : 'light'}
        position='bottom-right'
        richColors
        closeButton
        duration={4000}
      />
    </>
  );
}
