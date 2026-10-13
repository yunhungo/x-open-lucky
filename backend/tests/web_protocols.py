#!/usr/bin/env python3
# @created 2026-10-13
# @description 验证 Web 高级配置、真实 HTTP/3、h2c、流式传输与迁移兼容。
# @author yunhungo
import hashlib
import http.client
import json
import os
from pathlib import Path
import secrets
import socket
import sqlite3
import ssl
import subprocess
import tempfile
import time
from smoke import free_port

ROOT = Path(__file__).resolve().parents[2]


def main():
    admin, plain, tls, quic, mixed, upstream_port = [free_port() for _ in range(6)]
    with tempfile.TemporaryDirectory(prefix='openlucky-protocols-') as folder:
        root = Path(folder)
        log = (root / 'server.log').open('w')
        upstream = subprocess.Popen([str(ROOT / 'target/debug/examples/protocol_upstream'), f'127.0.0.1:{upstream_port}'], stdout=log, stderr=log)
        env = dict(os.environ, OPENLUCKY_DATA=str(root / 'data'), OPENLUCKY_ADMIN_ADDR=f'127.0.0.1:{admin}', OPENLUCKY_HTTP_ADDR=f'127.0.0.1:{plain}', OPENLUCKY_HTTPS_ADDR=f'127.0.0.1:{tls}', OPENLUCKY_PUBLIC_HTTPS_PORT=str(tls), OPENLUCKY_HTTP3_ADDR=f'127.0.0.1:{quic}', OPENLUCKY_PUBLIC_HTTP3_PORT=str(quic), OPENLUCKY_LISTENERS=json.dumps([{'id': 'mixed', 'addr': f'127.0.0.1:{mixed}', 'https_port': mixed, 'mixed': True}]))
        cookie = ''
        process = None
        def start():
            nonlocal process
            process = subprocess.Popen([str(ROOT / 'target/debug/openlucky')], cwd=ROOT, env=env, stdout=log, stderr=log)
            for _ in range(100):
                if process.poll() is not None:
                    raise AssertionError((root / 'server.log').read_text())
                try:
                    connection = http.client.HTTPConnection('127.0.0.1', admin, timeout=1)
                    connection.request('GET', '/healthz')
                    response = connection.getresponse(); response.read(); connection.close()
                    if response.status == 200: return
                except OSError: pass
                time.sleep(.1)
            raise AssertionError('Server readiness timeout')
        def api(method, path, data=None, expected=200):
            nonlocal cookie
            conn = http.client.HTTPConnection('127.0.0.1', admin, timeout=10)
            conn.request(method, '/api/v1' + path, None if data is None else json.dumps(data), {'Content-Type': 'application/json', 'X-OpenLucky': '1', 'Cookie': cookie})
            res = conn.getresponse(); body = res.read(); conn.close()
            if res.getheader('Set-Cookie'): cookie = res.getheader('Set-Cookie').split(';')[0]
            assert res.status == expected, (res.status, expected, body[:250])
            return json.loads(body)
        def get(path='/', port=plain, secure=False, ctx=None, extra=None):
            conn = http.client.HTTPConnection('127.0.0.1', port, timeout=10)
            if secure:
                raw = socket.create_connection(('127.0.0.1', port), timeout=10)
                conn.sock = (ctx or ssl.create_default_context(cafile=str(root / 'cert.pem'))).wrap_socket(raw, server_hostname='service.example.test')
            conn.request('GET', path, headers={'Host': f'service.example.test:{port}', **(extra or {})})
            res = conn.getresponse(); body = res.read(); headers = dict(res.getheaders()); code = res.status; conn.close()
            return code, body, {k.lower(): v for k, v in headers.items()}
        def h3(path='/', port=quic, expected=200, **options):
            args = {'address': f'127.0.0.1:{port}', 'server_name': 'service.example.test', 'ca': str(root / 'cert.pem'), 'path': path, **options}
            result = subprocess.run([str(ROOT / 'target/debug/examples/h3_probe')], input=json.dumps(args), text=True, capture_output=True, timeout=65)
            if expected is None:
                assert result.returncode != 0, result.stdout
                return
            assert result.returncode == 0, result.stderr
            value = json.loads(result.stdout)
            assert value['protocol'] == 'HTTP/3' and value['response']['status'] == expected, value
            return value
        try:
            start()
            api('POST', '/auth/setup', {'username': 'admin', 'password': secrets.token_urlsafe(24), 'setup_token': (root / 'data/setup-token').read_text()})
            rule = {'name': 'Protocols', 'domain': 'service.example.test', 'target': f'http://127.0.0.1:{upstream_port}', 'enabled': True, 'certificate_id': '', 'https_redirect': False, 'preserve_host': False}
            wid = api('POST', '/resources/web', rule)['id']
            current = api('GET', '/resources/web')[0]
            assert current['options']['http3'] is False and current['options']['connect_timeout'] == 10
            options = current['options']
            def save(changes=None, expected=200, **fields):
                current = next(v for v in api('GET', '/resources/web') if v['id'] == wid)
                value = {**current, **fields, 'options': {**current['options'], **(changes or {})}}
                return api('PUT', '/resources/web/' + wid, value, expected)
            for changes in [{'http3': True}, {'http1': False, 'http2': False}, {'connect_timeout': 0}, {'response_timeout': 601}, {'tls_min': '1.0'}, {'upstream_version': 'http3'}, {'request_headers': {'X-Test': 'bad\r\nInjected: header'}}, {'request_headers': {'Authorization': 'secret'}}, {'response_headers': {'Content-Length': '1'}}, {'request_headers': {'X-Test': '1', 'x-test': '2'}}, {'made_up_option': True}]:
                save(changes, expected=400)
            for version, expected in [('auto', 'HTTP/1.1'), ('http2', 'HTTP/2.0'), ('http1', 'HTTP/1.1')]:
                save({'upstream_version': version, 'request_headers': {'X-Custom': 'configured', 'X-Drop': None}, 'response_headers': {'X-Result': 'yes', 'Server': None}})
                code, body, headers = get(extra={'X-Drop': 'remove-me', 'X-Custom': 'untrusted'})
                data = json.loads(body)
                assert code == 200 and data['version'] == expected, data
                assert data['headers']['x-custom'] == 'configured' and 'x-drop' not in data['headers']
                assert headers['x-result'] == 'yes' and 'server' not in headers
            dynamic_request = {
                'X-Custom': 'configured',
                'X-Original': '{{ request.headers["x-custom"] | default("fallback") | upper }}',
                'X-Route': '{{ request.host }}|{{ request.method }}|{{ request.path }}|{{ request.uri }}|{{ client.ip }}',
                'X-Condition': '{{ "secure" if request.scheme == "https" else "plain" }}',
            }
            dynamic_response = {'X-Status': '{{ response.status }}', 'X-Original': '{{ request.headers["x-custom"] | default("missing") }}'}
            save({'request_headers': dynamic_request, 'response_headers': dynamic_response})
            code, body, headers = get('/dynamic?q=123', extra={'X-Custom': 'untrusted'})
            data = json.loads(body)
            assert data['headers']['x-original'] == 'UNTRUSTED' and data['headers']['x-custom'] == 'configured'
            assert data['headers']['x-route'] == 'service.example.test|GET|/dynamic|/dynamic?q=123|127.0.0.1'
            assert data['headers']['x-condition'] == 'plain' and headers['x-status'] == '200' and headers['x-original'] == 'untrusted'
            assert json.loads(get()[1])['headers']['x-original'] == 'FALLBACK'
            for value in ['{{ request.unknown }}', '{{ response.status }}', '{{ "x" * 99999999 }}', '{% for x in request.headers %}x{% endfor %}']:
                save({'request_headers': {'X-Invalid': value}}, expected=400)
            save({'request_headers': {'X-Dynamic': '{{ request.headers["x-required"] }}'}})
            assert get()[0] == 502
            save({'request_headers': dynamic_request})
            print('PASS compiled dynamic headers, original request context, defaults, conditions and bounded failures', flush=True)
            save({'response_timeout': 1})
            started = time.monotonic(); assert get('/delay')[0] == 504 and time.monotonic() - started < 1.8
            save({'response_timeout': 60})
            print('PASS upstream HTTP/1.1, h2c, custom headers, validation and timeout', flush=True)
            subprocess.run(['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(root / 'key.pem'), '-out', str(root / 'cert.pem'), '-days', '2', '-subj', '/CN=service.example.test', '-addext', 'subjectAltName=DNS:service.example.test', '-addext', 'basicConstraints=critical,CA:FALSE'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)
            cert = api('POST', '/resources/certificates', {'name': 'Protocol TLS', 'domains': ['service.example.test'], 'email': '', 'credential_id': '', 'staging': False, 'auto_renew': False, 'imported': True})['id']
            api('POST', f'/certificates/{cert}/import', {'cert_pem': (root / 'cert.pem').read_text(), 'key_pem': (root / 'key.pem').read_text()})
            save(certificate_id=cert)
            h3(expected=None)
            save({'http3': True})
            code, _, headers = get(port=tls, secure=True)
            assert code == 200 and headers['alt-svc'] == f'h3=":{quic}"; ma=300'
            sample = h3()['response']
            assert json.loads(sample['body'])['headers']['x-forwarded-proto'] == 'https'
            assert json.loads(sample['body'])['headers']['x-condition'] == 'secure'
            size = 4 * 1024 * 1024
            upload = json.loads(h3('/upload', upload_bytes=size)['response']['body'])
            assert upload['bytes'] == size and upload['sha256'] == hashlib.sha256(b'x' * size).hexdigest()
            large = h3('/large')['response']
            assert large['bytes'] == 16 * 1024 * 1024 and large['sha256'] == hashlib.sha256(b'x' * (16 * 1024 * 1024)).hexdigest()
            parallel = h3(requests=128, concurrency=16)
            assert parallel['failures'] == 0
            # The upstream responds before consuming the request body; validate both stream halves.
            early = h3('/bench', requests=4096, concurrency=32)
            assert early['failures'] == 0 and early['response']['bytes'] == 1024
            save({'upstream_version': 'http2'})
            assert json.loads(h3()['response']['body'])['version'] == 'HTTP/2.0'
            print('PASS real QUIC HTTP/3, 4 MiB upload, 16 MiB download, multiplexing and h2c upstream', flush=True)
            save({'tls_min': '1.3'})
            ctx = ssl.create_default_context(cafile=str(root / 'cert.pem')); ctx.maximum_version = ssl.TLSVersion.TLSv1_2
            try:
                get(port=tls, secure=True, ctx=ctx)
                raise AssertionError('TLS 1.2 accepted by TLS 1.3 policy')
            except ssl.SSLError: pass
            assert get(port=tls, secure=True)[0] == 200
            save({'http1': False})
            assert get(port=tls, secure=True)[0] == 421
            assert h3()['response']['status'] == 200
            save({'http1': True, 'access_mode': 'https'})
            assert get()[0] == 403
            save({'http1': False, 'http2': False}, https_redirect=True)
            assert get('/hello?q=1')[0] == 308
            assert h3()['response']['status'] == 200
            save({'http1': True, 'http2': True})
            save({'access_mode': 'both'}, https_redirect=False, listener='mixed')
            h3(expected=None)
            assert h3(port=mixed)['response']['status'] == 200
            assert get(port=mixed)[0] == 200 and get(port=mixed, secure=True)[0] == 200
            save({'http3': False})
            assert get(port=mixed, secure=True)[2]['alt-svc'] == 'clear'
            h3(port=mixed, expected=None)
            print('PASS TLS minimum, HTTP version gates, access modes, listener isolation and HTTP/3 disable', flush=True)
            # Validate old rows and old backups remain compatible after versioned migration.
            save(options, listener='default')
            password = secrets.token_urlsafe(24)
            backup = api('POST', '/backup/export', {'password': password})['backup']
            api('POST', '/backup/restore', {'password': password, 'backup': backup})
            assert api('GET', '/resources/web')[0]['options'] == options
            process.terminate(); process.wait(timeout=40); assert process.returncode == 0
            with sqlite3.connect(root / 'data/openlucky.sqlite') as db:
                db.execute("UPDATE resources SET data=json_remove(data, '$.options') WHERE kind='web'")
                db.execute('DELETE FROM _sqlx_migrations WHERE version=4')
            start()
            migrated = api('GET', '/resources/web')[0]
            assert migrated['options'] == options and get()[0] == 200
            print('PASS advanced option backup roundtrip, legacy migration and restart', flush=True)
        finally:
            for child in [process, upstream]:
                if child and child.poll() is None:
                    child.terminate()
                    try: child.wait(timeout=40)
                    except subprocess.TimeoutExpired: child.kill(); child.wait()
            log.close()


if __name__ == '__main__':
    main()
