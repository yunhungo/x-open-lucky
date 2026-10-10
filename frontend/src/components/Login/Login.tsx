/**
 * @created 2026-10-11
 * @description 提供管理员登录和一次性初始化表单。
 * @author yunhungo
 */
import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { z } from 'zod';
import { LoaderCircle, Moon, Sun } from 'lucide-react';
import { Brand } from '@/components/Brand/Brand';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { QueryState } from '@/components/QueryState/QueryState';
import { errorText, okSchema, request } from '@/lib/api';
import './Login.scss';
export function Login({ dark, toggle }: { dark: boolean; toggle: () => void }) {
  const queryClient = useQueryClient();
  const status = useQuery({
    queryKey: ['auth-status'],
    queryFn: () =>
      request('/auth/status', z.object({ initialized: z.boolean() })),
  });
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [setupToken, setSetupToken] = useState('');
  const setup = status.data?.initialized === false;
  const mutation = useMutation({
    mutationFn: () =>
      request(setup ? '/auth/setup' : '/auth/login', okSchema, {
        method: 'POST',
        body: JSON.stringify({ username, password, setup_token: setupToken }),
      }),
    onSuccess: () => {
      setPassword('');
      setSetupToken('');
      void queryClient.invalidateQueries();
    },
  });
  return (
    <main className='login'>
      <button
        className='login__theme'
        onClick={toggle}
        aria-label={dark ? '浅色模式' : '深色模式'}
      >
        {dark ? <Sun size={19} /> : <Moon size={19} />}
      </button>
      <section className='login__card'>
        <Brand />
        <h1>{setup ? '创建管理员' : '登录'}</h1>
        {status.isPending || status.isError ? (
          <QueryState
            error={status.error}
            retry={() => void status.refetch()}
          />
        ) : (
          <form
            onSubmit={(e) => {
              e.preventDefault();
              mutation.mutate();
            }}
            className='space-y-5'
          >
            <div className='space-y-2'>
              <Label htmlFor='username'>用户名</Label>
              <Input
                id='username'
                autoFocus
                autoComplete='username'
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                required
                minLength={setup ? 3 : 1}
                maxLength={50}
              />
            </div>
            <div className='space-y-2'>
              <Label htmlFor='password'>密码</Label>
              <Input
                id='password'
                type='password'
                autoComplete={setup ? 'new-password' : 'current-password'}
                value={password}
                onChange={(e) => setPassword(e.target.value)}
                required
                minLength={setup ? 12 : 1}
                maxLength={256}
              />
              {setup && (
                <p className='text-xs text-muted-foreground'>至少 12 个字符</p>
              )}
            </div>
            {setup && (
              <div className='space-y-2'>
                <Label htmlFor='setup-token'>初始化密钥</Label>
                <Input
                  id='setup-token'
                  type='password'
                  autoComplete='off'
                  value={setupToken}
                  onChange={(e) => setSetupToken(e.target.value)}
                  required
                />
                <p className='text-xs text-muted-foreground'>
                  服务数据目录中的 setup-token 文件
                </p>
              </div>
            )}
            {mutation.isError && (
              <p role='alert' className='text-sm text-destructive'>
                {errorText(mutation.error)}
              </p>
            )}
            <Button
              className='w-full'
              type='submit'
              disabled={mutation.isPending}
            >
              {mutation.isPending && (
                <LoaderCircle className='animate-spin' size={16} />
              )}
              {setup ? '创建并登录' : '登录'}
            </Button>
          </form>
        )}
      </section>
    </main>
  );
}
