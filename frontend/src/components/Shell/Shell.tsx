/**
 * @created 2026-10-11
 * @description 组合桌面侧栏、移动导航和管理员操作。
 * @author yunhungo
 */
import { useState } from 'react';
import { Outlet, useLocation } from 'react-router-dom';
import { LogOut, Menu, Moon, Sun, UserRound } from 'lucide-react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { Brand } from '@/components/Brand/Brand';
import { Navigation, navigation } from '@/components/Navigation/Navigation';
import { Button } from '@/components/ui/button';
import {
  Sheet,
  SheetContent,
  SheetTitle,
  SheetDescription,
  SheetHeader,
} from '@/components/ui/sheet';
import { errorText, okSchema, request } from '@/lib/api';
import './Shell.scss';
export function Shell({
  username,
  dark,
  toggle,
}: {
  username: string;
  dark: boolean;
  toggle: () => void;
}) {
  const [mobileOpen, setMobileOpen] = useState(false);
  const location = useLocation();
  const queryClient = useQueryClient();
  const logout = useMutation({
    mutationFn: () => request('/auth/logout', okSchema, { method: 'POST' }),
    onSuccess: () => {
      queryClient.clear();
      void queryClient.invalidateQueries();
    },
    onError: (e) => toast.error(errorText(e)),
  });
  const page =
    navigation.find((n) =>
      n.path === '/'
        ? location.pathname === '/'
        : location.pathname.startsWith(n.path),
    )?.name ?? '设置';
  return (
    <div className='shell'>
      <aside className='shell__sidebar'>
        <Brand />
        <Navigation />
        <div className='shell__account'>
          <span className='grid size-8 place-items-center rounded-full bg-muted text-muted-foreground'>
            <UserRound size={16} />
          </span>
          <span className='min-w-0 flex-1 truncate text-xs'>{username}</span>
          <Button
            variant='ghost'
            size='icon'
            aria-label='退出登录'
            disabled={logout.isPending}
            onClick={() => logout.mutate()}
          >
            <LogOut size={15} />
          </Button>
        </div>
      </aside>
      <div className='shell__body'>
        <header className='shell__header'>
          <div className='flex items-center gap-3'>
            <Button
              variant='ghost'
              size='icon'
              className='md:hidden'
              aria-label='打开导航'
              onClick={() => setMobileOpen(true)}
            >
              <Menu size={19} />
            </Button>
            <span className='text-xs text-muted-foreground'>工作台</span>
            <span className='text-xs text-border'>/</span>
            <span className='text-xs'>{page}</span>
          </div>
          <div className='flex items-center gap-4'>
            <Button
              variant='ghost'
              size='icon'
              aria-label={dark ? '浅色模式' : '深色模式'}
              onClick={toggle}
            >
              {dark ? <Sun size={17} /> : <Moon size={17} />}
            </Button>
          </div>
        </header>
        <main className='shell__main'>
          <Outlet />
        </main>
      </div>
      <Sheet open={mobileOpen} onOpenChange={setMobileOpen}>
        <SheetContent side='left' className='w-64 p-5'>
          <SheetHeader className='px-0'>
            <SheetTitle>
              <Brand />
            </SheetTitle>
            <SheetDescription className='sr-only'>页面导航</SheetDescription>
          </SheetHeader>
          <Navigation onNavigate={() => setMobileOpen(false)} />
          <Button
            variant='ghost'
            className='mt-auto justify-start'
            onClick={() => logout.mutate()}
          >
            <LogOut size={16} />
            退出登录
          </Button>
        </SheetContent>
      </Sheet>
    </div>
  );
}
