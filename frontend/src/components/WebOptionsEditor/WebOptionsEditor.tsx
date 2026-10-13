/**
 * @created 2026-10-13
 * @description 编辑服务接入协议、上游版本、超时与自定义头。
 * @author yunhungo
 */
import { useOverview } from '@/lib/api';
import { HeaderRulesEditor } from '@/components/HeaderRulesEditor/HeaderRulesEditor';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import type { FormValues } from '@/components/ResourceEditor/fields';
import './WebOptionsEditor.scss';
export function WebOptionsEditor({
  values,
  change,
  disabled,
}: {
  values: FormValues;
  change: (key: string, value: string | number | boolean) => void;
  disabled: boolean;
}) {
  const overview = useOverview();
  const listener = overview.data?.listeners.find(
    (item) => item.id === values.listener,
  );
  const port =
    values.listener === 'default'
      ? overview.data?.public_http3_port
      : (listener?.http3_port ?? listener?.https_port);
  return (
    <div className='web-options'>
      <div className='web-options__row'>
        <Label htmlFor='web-access'>接入协议</Label>
        <select
          id='web-access'
          className='form-select'
          disabled={disabled}
          value={String(values.option_access_mode)}
          onChange={(e) => change('option_access_mode', e.target.value)}
        >
          <option value='both'>HTTP / HTTPS</option>
          <option value='http'>仅 HTTP</option>
          <option value='https'>仅 HTTPS</option>
        </select>
      </div>
      <fieldset className='web-options__versions' disabled={disabled}>
        <legend>HTTP 版本</legend>
        {(['http1', 'http2', 'http3'] as const).map((key, i) => (
          <label key={key}>
            <input
              type='checkbox'
              checked={Boolean(values[`option_${key}`])}
              onChange={(e) => change(`option_${key}`, e.target.checked)}
            />
            HTTP/{['1.1', '2', '3'][i]}
          </label>
        ))}
      </fieldset>
      {values.option_http3 && (
        <p className='web-options__hint'>
          HTTP/3 需要证书及 UDP{port ? ` ${port}` : ' 入口端口'} 放行。
        </p>
      )}
      <details>
        <summary>高级配置</summary>
        <div className='web-options__advanced'>
          <div className='web-options__pair'>
            <div>
              <Label htmlFor='web-tls'>最低 TLS 版本</Label>
              <select
                id='web-tls'
                className='form-select'
                value={String(values.option_tls_min)}
                disabled={disabled}
                onChange={(e) => change('option_tls_min', e.target.value)}
              >
                <option value='1.2'>TLS 1.2</option>
                <option value='1.3'>TLS 1.3</option>
              </select>
            </div>
            <div>
              <Label htmlFor='web-upstream'>上游 HTTP 版本</Label>
              <select
                id='web-upstream'
                className='form-select'
                value={String(values.option_upstream_version)}
                disabled={disabled}
                onChange={(e) =>
                  change('option_upstream_version', e.target.value)
                }
              >
                <option value='auto'>自动协商</option>
                <option value='http1'>HTTP/1.1</option>
                <option value='http2'>HTTP/2（含 h2c）</option>
              </select>
            </div>
          </div>
          <div className='web-options__pair'>
            {[
              { key: 'connect_timeout', label: '连接超时（秒）', max: 60 },
              { key: 'response_timeout', label: '响应头超时（秒）', max: 600 },
            ].map(({ key, label, max }) => (
              <div key={key}>
                <Label htmlFor={`web-${key}`}>{label}</Label>
                <Input
                  id={`web-${key}`}
                  type='number'
                  min={1}
                  max={max}
                  required
                  disabled={disabled}
                  value={String(values[`option_${key}`])}
                  onChange={(e) =>
                    change(`option_${key}`, Number(e.target.value))
                  }
                />
              </div>
            ))}
          </div>
          {[
            { key: 'https_redirect', label: 'HTTP 跳转至 HTTPS' },
            { key: 'preserve_host', label: '保留原始 Host' },
          ].map(({ key, label }) => (
            <div className='web-options__row' key={key}>
              <Label htmlFor={`web-${key}`}>{label}</Label>
              <Switch
                id={`web-${key}`}
                disabled={disabled}
                checked={Boolean(values[key])}
                onCheckedChange={(value) => change(key, value)}
              />
            </div>
          ))}
          <HeaderRulesEditor
            value={String(values.option_request_headers)}
            onChange={(value) => change('option_request_headers', value)}
            disabled={disabled}
          />
          <HeaderRulesEditor
            response
            value={String(values.option_response_headers)}
            onChange={(value) => change('option_response_headers', value)}
            disabled={disabled}
          />
          <p className='web-options__hint'>
            认证与 Cookie 头不在此保存。
          </p>
          <details className='web-options__help'>
            <summary>模板用法</summary>
            <p>
              可用字段：request.host / authority / scheme / method / path / uri
              / id、client.ip、upstream.host；响应头还支持 response.status。
            </p>
            <code>
              {'{{ request.headers["x-custom"] | default("fallback") }}'}
            </code>
            <code>
              {'{{ "secure" if request.scheme == "https" else "plain" }}'}
            </code>
            <p>
              支持 default、lower、upper、trim、replace
              过滤器。原始请求头名称使用小写。
            </p>
          </details>
        </div>
      </details>
    </div>
  );
}
