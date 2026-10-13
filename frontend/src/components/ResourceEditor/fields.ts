/**
 * @created 2026-10-11
 * @description 定义资源编辑字段、默认值与请求转换。
 * @author yunhungo
 */
import type { Kind, Resource } from '@/lib/contracts';
import {
  defaultWebOptions,
  optionValues,
  parseOptions,
} from '@/lib/web-options';
export type FormValues = Record<string, string | number | boolean>;
export interface Field {
  key: string;
  label: string;
  type: 'text' | 'password' | 'number' | 'select' | 'switch' | 'email';
  placeholder?: string;
  options?: { value: string; label: string }[];
  min?: number;
  max?: number;
  required?: boolean;
  visible?: (v: FormValues) => boolean;
}
export const defaults: Record<Kind, FormValues> = {
  credentials: {
    name: '',
    provider: 'cloudflare',
    token: '',
    key_id: '',
    key_secret: '',
  },
  ddns: {
    name: '',
    enabled: true,
    credential_id: '',
    zone: '',
    domain: '',
    record_type: 'A',
    source: 'public',
    source_value: '',
    interval: 300,
    ttl: 600,
    proxied: false,
  },
  web: {
    ...optionValues(defaultWebOptions),
    listener: 'default',
    name: '',
    enabled: true,
    domain: '',
    target: '',
    certificate_id: '',
    https_redirect: false,
    preserve_host: false,
  },
  certificates: {
    name: '',
    domains: '',
    email: '',
    credential_id: '',
    staging: true,
    auto_renew: true,
    imported: false,
  },
};
const name: Field = {
  key: 'name',
  label: '名称',
  type: 'text',
  required: true,
};
const credential: Field = {
  key: 'credential_id',
  label: 'DNS 凭据',
  type: 'select',
  required: true,
};
export const fields: Record<Kind, Field[]> = {
  credentials: [
    name,
    {
      key: 'provider',
      label: '服务商',
      type: 'select',
      options: [
        { value: 'cloudflare', label: 'Cloudflare' },
        { value: 'alidns', label: '阿里云 DNS' },
        { value: 'tencentcloud', label: '腾讯云 DNSPod' },
      ],
    },
    {
      key: 'token',
      label: 'API Token',
      type: 'password',
      visible: (v) => v.provider === 'cloudflare',
    },
    {
      key: 'key_id',
      label: 'Access Key / Secret ID',
      type: 'password',
      visible: (v) => v.provider !== 'cloudflare',
    },
    {
      key: 'key_secret',
      label: 'Access Key Secret / Secret Key',
      type: 'password',
      visible: (v) => v.provider !== 'cloudflare',
    },
  ],
  ddns: [
    name,
    credential,
    {
      key: 'zone',
      label: '主域名',
      type: 'text',
      placeholder: 'example.com',
      required: true,
    },
    {
      key: 'domain',
      label: '记录域名',
      type: 'text',
      placeholder: 'nas.example.com',
      required: true,
    },
    {
      key: 'record_type',
      label: '记录类型',
      type: 'select',
      options: [
        { value: 'A', label: 'A · IPv4' },
        { value: 'AAAA', label: 'AAAA · IPv6' },
      ],
    },
    {
      key: 'source',
      label: 'IP 来源',
      type: 'select',
      options: [
        { value: 'public', label: '公网 IP' },
        { value: 'interface', label: '网卡地址' },
        { value: 'static', label: '指定 IP' },
      ],
    },
    {
      key: 'source_value',
      label: '查询接口 / 网卡名称 / IP 地址',
      type: 'text',
      placeholder: '公网默认自动查询；自定义接口用空格分隔',
    },
    {
      key: 'interval',
      label: '更新间隔（秒）',
      type: 'number',
      min: 10,
      max: 86400,
    },
    {
      key: 'ttl',
      label: 'TTL（秒，1 为自动）',
      type: 'number',
      min: 1,
      max: 86400,
    },
    { key: 'proxied', label: 'Cloudflare 代理', type: 'switch' },
    { key: 'enabled', label: '启用', type: 'switch' },
  ],
  web: [
    name,
    {
      key: 'listener',
      label: '监听入口',
      type: 'text',
      required: true,
      placeholder: 'default',
    },
    {
      key: 'domain',
      label: '域名',
      type: 'text',
      placeholder: 'app.example.com',
      required: true,
    },
    {
      key: 'target',
      label: '目标地址',
      type: 'text',
      placeholder: 'http://192.168.1.85:3000',
      required: true,
    },
    { key: 'certificate_id', label: 'SSL 证书', type: 'select' },
    { key: 'enabled', label: '启用', type: 'switch' },
  ],
  certificates: [
    name,
    {
      key: 'imported',
      label: '来源',
      type: 'select',
      options: [
        { value: 'false', label: 'ACME · DNS-01' },
        { value: 'true', label: '导入 PEM 证书' },
      ],
    },
    {
      key: 'domains',
      label: '域名（逗号分隔）',
      type: 'text',
      placeholder: 'example.com, *.example.com',
      required: true,
    },
    {
      key: 'email',
      label: '联系邮箱',
      type: 'email',
      required: true,
      visible: (v) => !v.imported,
    },
    { ...credential, visible: (v) => !v.imported },
    {
      key: 'staging',
      label: '使用测试环境',
      type: 'switch',
      visible: (v) => !v.imported,
    },
    {
      key: 'auto_renew',
      label: '到期前 30 天自动续期',
      type: 'switch',
      visible: (v) => !v.imported,
    },
  ],
};
export function initial(kind: Kind, item?: Resource): FormValues {
  const values = { ...defaults[kind] };
  if (item)
    for (const [key, value] of Object.entries(item)) {
      if (
        key in values &&
        (typeof value === 'string' ||
          typeof value === 'boolean' ||
          typeof value === 'number')
      )
        values[key] = value;
      else if (key === 'domains' && Array.isArray(value))
        values[key] = value.join(', ');
    }
  if (item && 'options' in item)
    Object.assign(values, optionValues(item.options));
  return values;
}
export function payload(
  kind: Kind,
  values: FormValues,
): Record<string, unknown> {
  if (kind === 'web')
    return {
      ...Object.fromEntries(
        Object.entries(values).filter(([key]) => !key.startsWith('option_')),
      ),
      name: String(values.name).trim(),
      domain: String(values.domain).trim().toLowerCase(),
      options: parseOptions(values),
    };
  if (kind === 'credentials')
    return {
      name: String(values.name).trim(),
      provider: values.provider,
      secrets:
        values.provider === 'cloudflare'
          ? { token: values.token }
          : { key_id: values.key_id, key_secret: values.key_secret },
    };
  if (kind === 'certificates')
    return {
      ...values,
      domains: String(values.domains)
        .split(/[\s,，]+/)
        .filter(Boolean)
        .map((d) => d.toLowerCase()),
      auto_renew: values.imported ? false : values.auto_renew,
    };
  return {
    ...values,
    name: String(values.name).trim(),
    ...('domain' in values
      ? { domain: String(values.domain).trim().toLowerCase() }
      : {}),
    ...('zone' in values
      ? { zone: String(values.zone).trim().toLowerCase() }
      : {}),
  };
}
