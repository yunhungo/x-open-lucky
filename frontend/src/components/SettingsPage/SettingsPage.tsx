/**
 * @created 2026-10-11
 * @description 管理 DNS 凭据、密码、会话和服务参数。
 * @author yunhungo
 */
import { useState } from 'react';
import { Link, useLocation } from 'react-router-dom';
import { useMutation } from '@tanstack/react-query';
import { LoaderCircle } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { ResourcePage } from '@/components/ResourcePage/ResourcePage';
import { BackupPanel } from '@/components/BackupPanel/BackupPanel';
import { errorText, okSchema, request, useOverview } from '@/lib/api';
import './SettingsPage.scss';
export function SettingsPage() {
  const location = useLocation();
  const dns = location.pathname.endsWith('/dns');
  const backup = location.pathname.endsWith('/backup');
  const [current, setCurrent] = useState('');
  const [password, setPassword] = useState('');
  const [confirmation, setConfirmation] = useState('');
  const [validation, setValidation] = useState('');
  const overview = useOverview();
  const change = useMutation({
    mutationFn: () =>
      request('/auth/password', okSchema, {
        method: 'PUT',
        body: JSON.stringify({
          current_password: current,
          new_password: password,
        }),
      }),
    onSuccess: () => {
      setCurrent('');
      setPassword('');
      setConfirmation('');
      setValidation('');
      toast.success('密码已更新，旧会话已失效');
    },
  });
  const revoke = useMutation({
    mutationFn: () =>
      request('/auth/sessions/revoke', okSchema, { method: 'POST' }),
    onSuccess: () => toast.success('其他会话已退出'),
    onError: (e) => toast.error(errorText(e)),
  });
  return (
    <>
      <h1 className='sr-only'>设置</h1>
      <nav className='settings-tabs' aria-label='设置分类'>
        <Link
          to='/settings'
          aria-current={!dns && !backup ? 'page' : undefined}
        >
          账户
        </Link>
        <Link to='/settings/dns' aria-current={dns ? 'page' : undefined}>
          DNS 凭据
        </Link>
        <Link to='/settings/backup' aria-current={backup ? 'page' : undefined}>
          备份
        </Link>
      </nav>
      {dns ? (
        <ResourcePage kind='credentials' embedded />
      ) : backup ? (
        <BackupPanel />
      ) : (
        <div className='settings-columns'>
          <section className='panel'>
            <h2 className='panel-heading'>修改密码</h2>
            <form
              className='space-y-5 p-6'
              onSubmit={(e) => {
                e.preventDefault();
                if (password !== confirmation) {
                  setValidation('两次输入的密码不一致');
                  return;
                }
                setValidation('');
                change.mutate();
              }}
            >
              <div className='space-y-2'>
                <Label htmlFor='current-password'>当前密码</Label>
                <Input
                  id='current-password'
                  type='password'
                  autoComplete='current-password'
                  value={current}
                  onChange={(e) => setCurrent(e.target.value)}
                  required
                  maxLength={256}
                />
              </div>
              <div className='space-y-2'>
                <Label htmlFor='new-password'>新密码</Label>
                <Input
                  id='new-password'
                  type='password'
                  autoComplete='new-password'
                  value={password}
                  onChange={(e) => setPassword(e.target.value)}
                  required
                  minLength={12}
                  maxLength={256}
                />
                <p className='text-[11px] text-muted-foreground'>
                  至少 12 个字符
                </p>
              </div>
              <div className='space-y-2'>
                <Label htmlFor='confirm-password'>确认新密码</Label>
                <Input
                  id='confirm-password'
                  type='password'
                  autoComplete='new-password'
                  value={confirmation}
                  onChange={(e) => setConfirmation(e.target.value)}
                  required
                  minLength={12}
                  maxLength={256}
                />
              </div>
              {(validation || change.isError) && (
                <p role='alert' className='text-sm text-destructive'>
                  {validation || errorText(change.error)}
                </p>
              )}
              <Button type='submit' disabled={change.isPending}>
                {change.isPending && (
                  <LoaderCircle className='animate-spin' size={15} />
                )}
                更新密码
              </Button>
            </form>
          </section>
          <div className='space-y-5'>
            <section className='panel'>
              <h2 className='panel-heading'>会话</h2>
              <div className='p-6'>
                <p className='text-xs text-muted-foreground mb-5'>
                  登录会话有效期为 12 小时。
                </p>
                <Button
                  variant='outline'
                  onClick={() => revoke.mutate()}
                  disabled={revoke.isPending}
                >
                  退出其他会话
                </Button>
              </div>
            </section>
            <section className='panel'>
              <h2 className='panel-heading'>版本</h2>
              <dl className='overview-list'>
                <div>
                  <dt>OpenLucky</dt>
                  <dd>{overview.data?.version ?? '—'}</dd>
                </div>
                <div>
                  <dt>提交</dt>
                  <dd className='text-[10px]'>
                    {overview.data?.build_sha.slice(0, 12) ?? '—'}
                  </dd>
                </div>
              </dl>
            </section>
          </div>
        </div>
      )}
    </>
  );
}
