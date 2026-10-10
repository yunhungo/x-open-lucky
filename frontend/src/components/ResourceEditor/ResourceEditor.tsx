/**
 * @created 2026-10-11
 * @description 在侧边表单创建或编辑资源并保留失败输入。
 * @author yunhungo
 */
import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { LoaderCircle } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import {
  SheetContent,
  SheetHeader,
  SheetTitle,
  SheetDescription,
  SheetFooter,
} from '@/components/ui/sheet';
import { errorText, idSchema, request, useResources } from '@/lib/api';
import { names, type Kind, type Resource } from '@/lib/contracts';
import { defaults, fields, initial, payload } from './fields';
export function ResourceEditor({
  kind,
  item,
  close,
}: {
  kind: Kind;
  item?: Resource;
  close: () => void;
}) {
  const [values, setValues] = useState(() => initial(kind, item));
  const queryClient = useQueryClient();
  const credentials = useResources('credentials');
  const certificates = useResources('certificates');
  const mutation = useMutation({
    mutationFn: () =>
      request(`/resources/${kind}${item ? `/${item.id}` : ''}`, idSchema, {
        method: item ? 'PUT' : 'POST',
        body: JSON.stringify({
          ...payload(kind, values),
          ...(item ? { revision: item.revision } : {}),
        }),
      }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ['resources'] });
      void queryClient.invalidateQueries({ queryKey: ['overview'] });
      toast.success(item ? '已保存' : '已添加');
      close();
    },
  });
  return (
    <SheetContent className='sm:max-w-[480px] w-full gap-0'>
      <SheetHeader className='border-b px-6 py-6'>
        <SheetTitle>
          {item ? '编辑' : '添加'}
          {names[kind]}
        </SheetTitle>
        <SheetDescription className='sr-only'>
          填写资源参数后保存
        </SheetDescription>
      </SheetHeader>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          mutation.mutate();
        }}
        className='flex min-h-0 flex-1 flex-col'
      >
        <div className='flex-1 overflow-y-auto px-6 py-6 space-y-5'>
          {fields[kind]
            .filter((field) => !field.visible || field.visible(values))
            .map((field) => {
              const id = `edit-${field.key}`;
              const locked =
                Boolean(item) &&
                ((kind === 'credentials' && field.key === 'provider') ||
                  (kind === 'certificates' &&
                    ['domains', 'staging', 'imported'].includes(field.key)));
              let options = field.options;
              if (field.key === 'credential_id')
                options = credentials.data?.map((c) => ({
                  value: c.id,
                  label: c.name,
                }));
              if (field.key === 'certificate_id')
                options = certificates.data?.map((c) => ({
                  value: c.id,
                  label: c.name,
                }));
              return (
                <div
                  key={field.key}
                  className={
                    field.type === 'switch'
                      ? 'flex justify-between items-center gap-4 py-1'
                      : 'space-y-2'
                  }
                >
                  <Label htmlFor={id}>{field.label}</Label>
                  {field.type === 'switch' ? (
                    <Switch
                      id={id}
                      checked={Boolean(values[field.key])}
                      disabled={locked || mutation.isPending}
                      onCheckedChange={(checked) =>
                        setValues((v) => ({ ...v, [field.key]: checked }))
                      }
                    />
                  ) : field.type === 'select' ? (
                    <select
                      className='form-select'
                      id={id}
                      value={String(values[field.key])}
                      required={field.required}
                      disabled={locked || mutation.isPending}
                      onChange={(e) =>
                        setValues((v) => ({
                          ...v,
                          [field.key]:
                            typeof defaults[kind][field.key] === 'boolean'
                              ? e.target.value === 'true'
                              : e.target.value,
                        }))
                      }
                    >
                      {!field.options && (
                        <option value=''>
                          {field.required ? '请选择' : '不使用'}
                        </option>
                      )}
                      {options?.map((option) => (
                        <option key={option.value} value={option.value}>
                          {option.label}
                        </option>
                      ))}
                    </select>
                  ) : (
                    <Input
                      id={id}
                      type={field.type}
                      value={String(values[field.key] ?? '')}
                      placeholder={
                        item && field.type === 'password'
                          ? '留空保留现有凭据'
                          : field.placeholder
                      }
                      min={field.min}
                      max={field.max}
                      maxLength={field.type === 'password' ? 2048 : 1024}
                      required={
                        field.required ||
                        (kind === 'credentials' &&
                          field.type === 'password' &&
                          !item)
                      }
                      autoComplete={
                        field.type === 'password' ? 'new-password' : 'off'
                      }
                      disabled={locked || mutation.isPending}
                      onChange={(e) =>
                        setValues((v) => ({
                          ...v,
                          [field.key]:
                            field.type === 'number'
                              ? Number(e.target.value)
                              : e.target.value,
                        }))
                      }
                    />
                  )}
                  {field.key === 'credential_id' &&
                    credentials.data?.length === 0 && (
                      <p className='text-xs text-muted-foreground'>
                        请先在设置中添加 DNS 凭据
                      </p>
                    )}
                </div>
              );
            })}
          {mutation.isError && (
            <p role='alert' className='text-sm text-destructive'>
              {errorText(mutation.error)}
            </p>
          )}
          {credentials.isError && (
            <p role='alert' className='text-sm text-destructive'>
              {errorText(credentials.error)}
            </p>
          )}
        </div>
        <SheetFooter className='flex-row justify-end border-t px-6 py-4'>
          <Button
            type='button'
            variant='outline'
            onClick={close}
            disabled={mutation.isPending}
          >
            取消
          </Button>
          <Button type='submit' disabled={mutation.isPending}>
            {mutation.isPending && (
              <LoaderCircle size={16} className='animate-spin' />
            )}
            保存
          </Button>
        </SheetFooter>
      </form>
    </SheetContent>
  );
}
