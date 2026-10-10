/**
 * @created 2026-10-11
 * @description 导出密码保护的完整配置并确认恢复备份。
 * @author yunhungo
 */
import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { z } from 'zod';
import { Download, LoaderCircle } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  AlertDialog,
  AlertDialogContent,
  AlertDialogHeader,
  AlertDialogTitle,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogCancel,
} from '@/components/ui/alert-dialog';
import { errorText, okSchema, request } from '@/lib/api';
export function BackupPanel() {
  const [password, setPassword] = useState('');
  const [file, setFile] = useState<File | null>(null);
  const [confirm, setConfirm] = useState(false);
  const client = useQueryClient();
  const exportBackup = useMutation({
    mutationFn: () =>
      request('/backup/export', z.object({ backup: z.string() }), {
        method: 'POST',
        body: JSON.stringify({ password }),
      }),
    onSuccess: (data) => {
      const url = URL.createObjectURL(
        new Blob([data.backup], { type: 'application/json' }),
      );
      const link = document.createElement('a');
      link.href = url;
      link.download = `openlucky-${new Date().toISOString().slice(0, 10)}.json`;
      link.click();
      URL.revokeObjectURL(url);
      toast.success('加密备份已导出');
    },
  });
  const restore = useMutation({
    mutationFn: async () => {
      if (!file) throw new Error('请选择备份文件');
      if (file.size > 4 * 1024 * 1024) throw new Error('备份文件超过 4 MB');
      return request('/backup/restore', okSchema, {
        method: 'POST',
        body: JSON.stringify({ password, backup: await file.text() }),
      });
    },
    onSuccess: () => {
      setConfirm(false);
      setPassword('');
      void client.invalidateQueries();
      toast.success('配置已恢复');
    },
  });
  return (
    <section className='panel max-w-2xl'>
      <h2 className='panel-heading'>加密备份</h2>
      <div className='p-6 space-y-6'>
        <p className='text-xs text-muted-foreground leading-relaxed'>
          包含 DNS 凭据、动态域名、Web
          服务和证书私钥。恢复会替换这些配置，保留当前管理员账户。
        </p>
        <div className='space-y-2'>
          <Label htmlFor='backup-password'>备份密码</Label>
          <Input
            id='backup-password'
            type='password'
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            minLength={12}
            maxLength={256}
            autoComplete='new-password'
            placeholder='至少 12 个字符'
          />
        </div>
        <Button
          disabled={password.length < 12 || exportBackup.isPending}
          onClick={() => exportBackup.mutate()}
        >
          {exportBackup.isPending ? (
            <LoaderCircle size={15} className='animate-spin' />
          ) : (
            <Download size={15} />
          )}
          导出备份
        </Button>
        {exportBackup.isError && (
          <p role='alert' className='text-sm text-destructive'>
            {errorText(exportBackup.error)}
          </p>
        )}
        <div className='border-t pt-6 space-y-4'>
          <Label htmlFor='backup-file'>恢复备份</Label>
          <Input
            id='backup-file'
            type='file'
            accept='.json'
            onChange={(e) => {
              setFile(e.target.files?.[0] ?? null);
              restore.reset();
            }}
          />
          <Button
            variant='outline'
            disabled={!file || password.length < 12}
            onClick={() => setConfirm(true)}
          >
            恢复配置
          </Button>
        </div>
      </div>
      <AlertDialog open={confirm} onOpenChange={setConfirm}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>替换当前配置？</AlertDialogTitle>
            <AlertDialogDescription>
              建议先导出当前配置。恢复后立即使用备份中的 DNS、Web 和证书设置。
            </AlertDialogDescription>
          </AlertDialogHeader>
          {restore.isError && (
            <p role='alert' className='text-sm text-destructive'>
              {errorText(restore.error)}
            </p>
          )}
          <AlertDialogFooter>
            <AlertDialogCancel disabled={restore.isPending}>
              取消
            </AlertDialogCancel>
            <Button
              variant='destructive'
              onClick={() => restore.mutate()}
              disabled={restore.isPending}
            >
              {restore.isPending && (
                <LoaderCircle size={15} className='animate-spin' />
              )}
              恢复
            </Button>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </section>
  );
}
