/**
 * @created 2026-10-11
 * @description 验证并上传 PEM 证书与私钥。
 * @author yunhungo
 */
import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { toast } from 'sonner';
import { LoaderCircle } from 'lucide-react';
import {
  SheetContent,
  SheetHeader,
  SheetTitle,
  SheetDescription,
  SheetFooter,
} from '@/components/ui/sheet';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import { errorText, okSchema, request } from '@/lib/api';
import type { Resource } from '@/lib/contracts';
export function CertificateImport({
  item,
  close,
}: {
  item: Resource;
  close: () => void;
}) {
  const [cert, setCert] = useState('');
  const [key, setKey] = useState('');
  const queryClient = useQueryClient();
  const upload = useMutation({
    mutationFn: () =>
      request(`/certificates/${item.id}/import`, okSchema, {
        method: 'POST',
        body: JSON.stringify({ cert_pem: cert, key_pem: key }),
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['resources'] });
      toast.success('证书已导入');
      setKey('');
      close();
    },
  });
  return (
    <SheetContent className='sm:max-w-[520px] w-full'>
      <SheetHeader>
        <SheetTitle>上传证书 · {item.name}</SheetTitle>
        <SheetDescription className='sr-only'>
          上传 PEM 证书和私钥
        </SheetDescription>
      </SheetHeader>
      <form
        className='flex flex-1 min-h-0 flex-col'
        onSubmit={(e) => {
          e.preventDefault();
          upload.mutate();
        }}
      >
        <div className='flex-1 overflow-y-auto p-5 space-y-5'>
          <div className='space-y-2'>
            <Label htmlFor='cert-pem'>证书链（PEM）</Label>
            <Textarea
              id='cert-pem'
              value={cert}
              onChange={(e) => setCert(e.target.value)}
              className='min-h-44 font-mono text-xs'
              placeholder='-----BEGIN CERTIFICATE-----'
              required
            />
          </div>
          <div className='space-y-2'>
            <Label htmlFor='key-pem'>私钥（PEM）</Label>
            <Textarea
              id='key-pem'
              value={key}
              onChange={(e) => setKey(e.target.value)}
              className='min-h-44 font-mono text-xs'
              placeholder='-----BEGIN PRIVATE KEY-----'
              autoComplete='off'
              required
            />
          </div>
          {upload.isError && (
            <p role='alert' className='text-sm text-destructive'>
              {errorText(upload.error)}
            </p>
          )}
        </div>
        <SheetFooter className='flex-row justify-end border-t'>
          <Button
            type='button'
            variant='outline'
            onClick={close}
            disabled={upload.isPending}
          >
            取消
          </Button>
          <Button type='submit' disabled={upload.isPending}>
            {upload.isPending && (
              <LoaderCircle size={16} className='animate-spin' />
            )}
            导入
          </Button>
        </SheetFooter>
      </form>
    </SheetContent>
  );
}
