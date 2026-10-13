#!/usr/bin/env python3
# @created 2026-10-11
# @description 对真实服务验证登录、权限、代理、WebSocket、TLS、备份和重启。
# @author yunhungo
import base64
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
import threading
import time
from concurrent.futures import ThreadPoolExecutor
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ROOT = Path(__file__).resolve().parents[2]


def free_port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        return sock.getsockname()[1]


class Upstream(BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.1'

    def log_message(self, *args):
        pass

    def do_GET(self):
        if self.headers.get('Upgrade') == 'tailscale-control-protocol':
            self.send_response(101)
            self.send_header('Upgrade', 'tailscale-control-protocol')
            self.send_header('Connection', 'Upgrade')
            self.end_headers()
            self.wfile.write(self.rfile.read(4))
            self.wfile.flush()
            self.close_connection = True
            return
        if self.headers.get('Upgrade', '').lower() == 'websocket':
            accept = base64.b64encode(hashlib.sha1((self.headers['Sec-WebSocket-Key'] + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11').encode()).digest()).decode()
            self.send_response(101)
            self.send_header('Upgrade', 'websocket')
            self.send_header('Connection', 'Upgrade')
            self.send_header('Sec-WebSocket-Accept', accept)
            self.end_headers()
            data = self.rfile.read(2)
            size = data[1] & 127
            mask = self.rfile.read(4)
            payload = self.rfile.read(size)
            payload = bytes(value ^ mask[index % 4] for index, value in enumerate(payload))
            self.wfile.write(bytes([129, len(payload)]) + payload)
            self.wfile.flush()
            self.close_connection = True
            return
        if self.path.startswith('/broken-stream'):
            self.send_response(200)
            self.send_header('Content-Length', '100')
            self.end_headers()
            self.wfile.write(b'partial')
            self.wfile.flush()
            self.close_connection = True
            return
        if self.path.startswith('/stream'):
            self.send_response(200)
            self.send_header('Transfer-Encoding', 'chunked')
            self.end_headers()
            for payload in (b'first', b'second'):
                self.wfile.write(f'{len(payload):x}\r\n'.encode() + payload + b'\r\n')
                self.wfile.flush()
                time.sleep(0.04)
            self.wfile.write(b'0\r\n\r\n')
            return
        payload = json.dumps({'path': self.path, 'host': self.headers.get('Host'), 'forwarded_for': self.headers.get('X-Forwarded-For'), 'proto': self.headers.get('X-Forwarded-Proto'), 'injected': self.headers.get('X-Remove')}).encode()
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(payload)))
        self.end_headers()
        self.wfile.write(payload)


def main():
    admin, proxy, tls, public = free_port(), free_port(), free_port(), free_port()
    upstream = ThreadingHTTPServer(('127.0.0.1', 0), Upstream)
    threading.Thread(target=upstream.serve_forever, daemon=True).start()
    checks = []
    with tempfile.TemporaryDirectory(prefix='openlucky-smoke-') as directory:
        root = Path(directory)
        frontend = root / 'frontend'
        frontend.mkdir()
        index = b'<!doctype html><title>OpenLucky smoke</title><div id="root"></div>'
        (frontend / 'index.html').write_bytes(index)
        env = dict(os.environ, OPENLUCKY_DATA=str(root / 'data'), OPENLUCKY_ADMIN_ADDR=f'127.0.0.1:{admin}', OPENLUCKY_HTTP_ADDR=f'127.0.0.1:{proxy}', OPENLUCKY_HTTPS_ADDR=f'127.0.0.1:{tls}', OPENLUCKY_PUBLIC_HTTPS_PORT=str(tls))
        env['OPENLUCKY_FRONTEND'] = str(frontend)
        env['OPENLUCKY_LISTENERS'] = json.dumps([{'id': 'public', 'addr': f'127.0.0.1:{public}', 'https_port': public, 'mixed': True}])
        log = (root / 'server.log').open('w')
        process = None
        cookie = ''

        def start():
            nonlocal process
            process = subprocess.Popen([str(ROOT / 'target/debug/openlucky')], env=env, cwd=ROOT, stdout=log, stderr=log)
            for _ in range(100):
                if process.poll() is not None:
                    raise RuntimeError('Server exited during startup')
                try:
                    connection = http.client.HTTPConnection('127.0.0.1', admin, timeout=1)
                    connection.request('GET', '/healthz')
                    response = connection.getresponse()
                    response.read()
                    connection.close()
                    if response.status == 200:
                        return
                except OSError:
                    time.sleep(0.1)
            raise RuntimeError('Server failed readiness')

        def call(method, path, body=None, expected=200, session=True, extra=None):
            nonlocal cookie
            if method == 'PUT' and path.startswith('/resources/') and isinstance(body, dict) and 'revision' not in body:
                kind, resource_id = path.split('/')[2:4]
                current = next(item for item in call('GET', f'/resources/{kind}') if item['id'] == resource_id)
                body = {**body, 'revision': current['revision']}
            connection = http.client.HTTPConnection('127.0.0.1', admin, timeout=20)
            headers = {'Content-Type': 'application/json', 'X-OpenLucky': '1'}
            if session and cookie:
                headers['Cookie'] = cookie
            if extra:
                headers.update(extra)
            connection.request(method, '/api/v1' + path, None if body is None else json.dumps(body), headers)
            response = connection.getresponse()
            raw = response.read()
            assert response.status == expected, f'{method} {path}: {response.status}, expected {expected}: {raw[:250]!r}'
            new_cookie = response.getheader('Set-Cookie')
            if new_cookie:
                cookie = new_cookie.split(';')[0]
            connection.close()
            return json.loads(raw)

        def get_proxy(path, host='echo.example.test', headers=None):
            connection = http.client.HTTPConnection('127.0.0.1', proxy, timeout=10)
            connection.request('GET', path, headers={'Host': host, **(headers or {})})
            response = connection.getresponse()
            data = response.read()
            status = response.status
            location = response.getheader('Location')
            connection.close()
            return status, data, location

        def check(name):
            checks.append(name)
            print('PASS', name, flush=True)

        try:
            start()
            for path in ['/', '/web?group=group-network', '/web/example', '/logs?group=group-network']:
                connection = http.client.HTTPConnection('127.0.0.1', admin, timeout=10)
                connection.request('GET', path)
                response = connection.getresponse()
                assert response.status == 200, (path, response.status)
                assert response.getheader('Content-Type').startswith('text/html')
                assert response.read() == index
                connection.close()
            check('SPA direct links return the page with HTTP 200')
            password = secrets.token_urlsafe(24)
            setup = (root / 'data/setup-token').read_text()
            assert not call('GET', '/auth/status')['initialized']
            call('GET', '/overview', expected=401)
            for endpoint in ['/access-logs', '/traffic', '/telemetry']:
                call('GET', endpoint, expected=401)
            call('POST', '/auth/setup', {'username': 'admin', 'password': password, 'setup_token': 'incorrect'}, expected=403)
            call('POST', '/auth/setup', {'username': 'admin', 'password': password, 'setup_token': setup})
            assert not (root / 'data/setup-token').exists()
            assert call('GET', '/auth/me')['username'] == 'admin'
            call('GET', '/missing-endpoint', expected=404)
            call('POST', '/auth/setup', {'username': 'admin', 'password': password, 'setup_token': setup}, expected=403)
            call('POST', '/auth/login', {'username': 'admin', 'password': 'wrong'}, expected=401)
            call('POST', '/auth/login', {'username': 'admin', 'password': password})
            check('one-time setup, authenticated login and session')
            call('POST', '/resources/credentials', {}, expected=403, extra={'Origin': 'https://evil.example'})
            call('POST', '/resources/credentials', {}, expected=403, extra={'X-OpenLucky': '0'})
            credential = {'name': 'Smoke Cloudflare', 'provider': 'cloudflare', 'secrets': {'token': secrets.token_urlsafe(24)}}
            cid = call('POST', '/resources/credentials', credential)['id']
            listed = call('GET', '/resources/credentials')
            assert listed[0]['secrets'] == {} and 'encrypted' not in listed[0]
            with sqlite3.connect(root / 'data/openlucky.sqlite') as db:
                raw = db.execute('SELECT data FROM resources WHERE kind="credentials"').fetchone()[0]
                assert credential['secrets']['token'] not in raw
                assert db.execute('SELECT COUNT(*) FROM _sqlx_migrations').fetchone()[0] == 5
            call('PUT', f'/resources/credentials/{cid}', {**credential, 'secrets': {'token': ''}})
            call('PUT', f'/resources/credentials/{cid}', {**credential, 'revision': 0}, expected=409)
            check('CSRF checks, encrypted at-rest credentials and SQL migrations')
            ddns = {'name': 'DNS rule', 'enabled': False, 'credential_id': cid, 'zone': 'example.test', 'domain': 'echo.example.test', 'record_type': 'A', 'source': 'static', 'source_value': '203.0.113.8', 'interval': 300, 'ttl': 600, 'proxied': False}
            did = call('POST', '/resources/ddns', ddns)['id']
            call('POST', '/resources/ddns', ddns, expected=400)
            call('DELETE', f'/resources/credentials/{cid}', expected=400)
            call('PUT', f'/resources/ddns/{did}', {**ddns, 'domain': 'invalid..test'}, expected=400)
            assert len(call('GET', '/resources/ddns')) == 1
            check('resource validation and dependency protection')
            rule = {'name': 'Echo', 'enabled': True, 'domain': 'echo.example.test', 'target': f'http://127.0.0.1:{upstream.server_port}', 'certificate_id': '', 'https_redirect': False, 'preserve_host': False}
            wid = call('POST', '/resources/web', rule)['id']
            status, body, _ = get_proxy('/hello?value=1', headers={'X-Forwarded-For': '1.2.3.4', 'Connection': 'X-Remove', 'X-Remove': 'sensitive'})
            echo = json.loads(body)
            assert status == 200 and echo['path'] == '/hello?value=1' and echo['forwarded_for'] == '127.0.0.1' and echo['injected'] is None
            assert get_proxy('/', host='unknown.example.test')[0] == 404
            assert get_proxy('/stream')[1] == b'firstsecond'
            check('HTTP streaming proxy, domain routing and hop-header removal')
            # 日志只保存路径，不保存请求中的查询参数或任意头。
            get_proxy('/privacy?token=never-store-query', headers={'Authorization': 'Bearer never-store-auth', 'Cookie': 'secret=never-store-cookie'})
            broken = http.client.HTTPConnection('127.0.0.1', proxy, timeout=10)
            broken.request('GET', '/broken-stream', headers={'Host': 'echo.example.test'})
            try:
                broken.getresponse().read()
            except (http.client.HTTPException, OSError):
                pass
            finally:
                broken.close()
            bad_rule = {**rule, 'name': 'Broken upstream', 'domain': 'broken.example.test', 'target': f'http://127.0.0.1:{free_port()}'}
            bad_id = call('POST', '/resources/web', bad_rule)['id']
            assert get_proxy('/failure', host='broken.example.test')[0] == 502
            for _ in range(100):
                telemetry = call('GET', '/traffic')
                if telemetry['requests'] >= 6 and telemetry['stream_errors'] > 0:
                    break
                time.sleep(0.1)
            assert telemetry['requests'] >= 6 and telemetry['server_errors'] == 1
            assert telemetry['client_errors'] == 1 and telemetry['stream_errors'] >= 1
            logs = call('GET', f'/access-logs?service_id={wid}')['items']
            privacy = next(row for row in logs if row['path'] == '/privacy')
            assert privacy['client_ip'] == '127.0.0.1' and privacy['status'] == 200 and privacy['duration_ms'] >= 0
            assert not any(secret in json.dumps(logs) for secret in ['never-store-query', 'never-store-auth', 'never-store-cookie'])
            assert any(row['phase'] == 'stream' and row['error'] for row in logs)
            errors = call('GET', f'/access-logs?service_id={bad_id}&errors=true')['items']
            assert len(errors) == 1 and errors[0]['status'] == 502 and '上游连接失败' in errors[0]['error']
            first_page = call('GET', '/access-logs?limit=2')
            assert first_page['next_cursor'] and len(first_page['items']) == 2
            second_page = call('GET', f"/access-logs?limit=2&before={first_page['next_cursor']}")
            assert not ({row['id'] for row in first_page['items']} & {row['id'] for row in second_page['items']})
            assert len(call('GET', f"/access-logs?q={privacy['request_id']}")['items']) == 1
            for query in ['limit=201', 'status=999', 'before=-1', 'since=100&until=1']:
                call('GET', '/access-logs?' + query, expected=400)
            call('GET', '/traffic?hours=999999', expected=400)
            health = call('GET', '/telemetry')
            assert health['policy']['max_rows'] == 20000 and health['write_failures'] == 0
            assert 'cpu_percent' in call('GET', '/overview')['performance']
            with sqlite3.connect(root / 'data/telemetry.sqlite') as db:
                assert db.execute('SELECT COUNT(*) FROM _sqlx_migrations').fetchone()[0] == 1
            call('DELETE', f'/resources/web/{bad_id}')
            check('access privacy, failed upstream, stream errors, service statistics, pagination and limits')
            with socket.create_connection(('127.0.0.1', proxy), timeout=5) as sock:
                key = base64.b64encode(os.urandom(16)).decode()
                sock.sendall(f'GET /ws HTTP/1.1\r\nHost: echo.example.test\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: {key}\r\n\r\n'.encode())
                data = b''
                while b'\r\n\r\n' not in data:
                    data += sock.recv(1024)
                assert b'101 Switching Protocols' in data
                payload = b'websocket'; mask = os.urandom(4)
                sock.sendall(bytes([129, 128 + len(payload)]) + mask + bytes(c ^ mask[i % 4] for i, c in enumerate(payload)))
                frame = sock.recv(1024)
                assert frame[0] == 129 and frame[2:] == payload
            check('real WebSocket upgrade and bidirectional tunnel')
            with socket.create_connection(('127.0.0.1', proxy), timeout=5) as sock:
                sock.sendall(b'GET /ts2021 HTTP/1.1\r\nHost: echo.example.test\r\nConnection: Upgrade\r\nUpgrade: tailscale-control-protocol\r\n\r\n')
                headers = b''
                while b'\r\n\r\n' not in headers:
                    headers += sock.recv(1)
                assert b'101 Switching Protocols' in headers
                sock.sendall(b'ping')
                assert sock.recv(4) == b'ping'
            check('Headscale control protocol upgrade and bidirectional tunnel')
            subprocess.run(['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(root / 'key.pem'), '-out', str(root / 'cert.pem'), '-days', '2', '-subj', '/CN=echo.example.test', '-addext', 'subjectAltName=DNS:echo.example.test'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)
            cert = {'name': 'Echo TLS', 'domains': ['echo.example.test'], 'email': '', 'credential_id': '', 'staging': False, 'auto_renew': False, 'imported': True}
            cert_id = call('POST', '/resources/certificates', cert)['id']
            pem = {'cert_pem': (root / 'cert.pem').read_text(), 'key_pem': (root / 'key.pem').read_text()}
            call('POST', f'/certificates/{cert_id}/import', {**pem, 'key_pem': 'invalid'}, expected=400)
            call('POST', f'/certificates/{cert_id}/import', pem)
            rule.update(certificate_id=cert_id, https_redirect=True)
            call('PUT', f'/resources/web/{wid}', rule)
            status, _, location = get_proxy('/hello?q=1')
            assert status == 308 and location == f'https://echo.example.test:{tls}/hello?q=1'
            ctx = ssl.create_default_context(cafile=str(root / 'cert.pem'))
            with socket.create_connection(('127.0.0.1', tls), timeout=5) as sock:
                with ctx.wrap_socket(sock, server_hostname='echo.example.test') as secure:
                    secure.sendall(b'GET /secure HTTP/1.1\r\nHost: echo.example.test\r\nConnection: close\r\n\r\n')
                    data = b''
                    while True:
                        chunk = secure.recv(4096)
                        if not chunk: break
                        data += chunk
                    assert b'200 OK' in data and b'"proto": "https"' in data
            for preserve in [False, True]:
                rule['preserve_host'] = preserve
                call('PUT', f'/resources/web/{wid}', rule)
                h2 = subprocess.run([
                    'curl', '--noproxy', '*', '--http2', '--silent', '--show-error',
                    '--fail', '--max-time', '10', '--cacert', str(root / 'cert.pem'),
                    '--resolve', f'echo.example.test:{tls}:127.0.0.1',
                    '--write-out', '\n%{http_version}',
                    f'https://echo.example.test:{tls}/http2',
                ], capture_output=True, text=True)
                assert h2.returncode == 0, (h2.stderr, h2.stdout)
                body, protocol = h2.stdout.rsplit('\n', 1)
                assert protocol == '2', h2.stdout
                forwarded = json.loads(body)
                expected_host = f'echo.example.test:{tls}' if preserve else f'127.0.0.1:{upstream.server_port}'
                assert forwarded['host'] == expected_host and forwarded['proto'] == 'https', forwarded
            check('HTTP/2 authority routing and preserved/replaced upstream Host')
            call('DELETE', f'/resources/certificates/{cert_id}', expected=400)
            check('certificate validation, SNI TLS handshake and HTTPS redirect')
            # A TLS SNI for one listener must never reveal another listener's route/certificate.
            with socket.create_connection(('127.0.0.1', public), timeout=5) as sock:
                try:
                    ctx.wrap_socket(sock, server_hostname='echo.example.test')
                    raise AssertionError('Private certificate exposed through public listener')
                except ssl.SSLError:
                    pass
            call('PUT', f'/resources/web/{wid}', {**rule, 'listener': 'missing'}, expected=400)
            call('PUT', f'/resources/web/{wid}', {**rule, 'listener': 'public'})
            assert get_proxy('/')[0] == 404
            connection = http.client.HTTPConnection('127.0.0.1', public, timeout=5)
            connection.request('GET', '/mixed?q=1', headers={'Host': 'echo.example.test'})
            response = connection.getresponse()
            assert response.status == 308 and response.getheader('Location') == f'https://echo.example.test:{public}/mixed?q=1'
            response.read(); connection.close()
            mixed = subprocess.run(['curl', '--noproxy', '*', '--http2', '--fail', '--silent', '--show-error', '--max-time', '10', '--cacert', str(root / 'cert.pem'), '--resolve', f'echo.example.test:{public}:127.0.0.1', f'https://echo.example.test:{public}/mixed'], capture_output=True, text=True)
            assert mixed.returncode == 0 and json.loads(mixed.stdout)['proto'] == 'https', mixed.stderr
            # HTTP/2 authority cannot route to a private service through a public TLS connection.
            private_id = call('POST', '/resources/web', {**rule, 'domain': 'private.example.test', 'certificate_id': '', 'https_redirect': False})['id']
            blocked = subprocess.run(['curl', '--noproxy', '*', '--http2', '--silent', '--show-error', '--max-time', '10', '--cacert', str(root / 'cert.pem'), '--resolve', f'echo.example.test:{public}:127.0.0.1', '-H', 'Host: private.example.test', '-o', '/dev/null', '-w', '%{http_code}', f'https://echo.example.test:{public}/'], capture_output=True, text=True)
            assert blocked.returncode == 0 and blocked.stdout == '404', blocked
            call('DELETE', f'/resources/web/{private_id}')
            call('PUT', f'/resources/web/{wid}', {**rule, 'listener': 'default'})
            wildcard = call('POST', '/resources/ddns', {**ddns, 'domain': '*.s.example.test', 'interval': 36, 'source': 'public', 'source_value': 'https://6.ipw.cn http://myip6.ipip.net'})['id']
            call('DELETE', f'/resources/ddns/{wildcard}')
            check('isolated listeners, mixed HTTP/TLS, HTTP/2 host isolation and wildcard DDNS')

            backup_password = secrets.token_urlsafe(24)
            backup = call('POST', '/backup/export', {'password': backup_password})['backup']
            assert credential['secrets']['token'] not in backup and 'BEGIN PRIVATE KEY' not in backup
            call('POST', '/backup/restore', {'password': 'incorrect-password-value', 'backup': backup}, expected=400)
            assert len(call('GET', '/resources/web')) == 1
            call('POST', '/backup/restore', {'password': backup_password, 'backup': backup})
            assert len(call('GET', '/resources/web')) == 1 and len(call('GET', '/resources/credentials')) == 1
            assert call('GET', '/resources/web')[0]['id'] != wid
            check('encrypted portable backup and transactional restore')
            old_cookie = cookie
            new_password = ('秘密😀' * 50) + secrets.token_hex(8)
            call('PUT', '/auth/password', {'current_password': password, 'new_password': new_password})
            new_cookie = cookie
            call('GET', '/auth/me', expected=401, extra={'Cookie': old_cookie})
            cookie = new_cookie
            call('GET', '/auth/me')
            call('POST', '/auth/login', {'username': 'admin', 'password': password}, expected=401)
            call('POST', '/auth/login', {'username': 'admin', 'password': new_password})
            check('password change revokes old sessions')
            concurrent_password = secrets.token_urlsafe(24)
            cookie_before_race = cookie

            def concurrent_request(is_change):
                connection = http.client.HTTPConnection('127.0.0.1', admin, timeout=20)
                path = '/auth/password' if is_change else '/auth/login'
                payload = {'current_password': new_password, 'new_password': concurrent_password} if is_change else {'username': 'admin', 'password': new_password}
                connection.request('PUT' if is_change else 'POST', '/api/v1' + path, json.dumps(payload), {'Content-Type': 'application/json', 'X-OpenLucky': '1', 'Cookie': cookie_before_race})
                response = connection.getresponse(); response.read()
                session_cookie = response.getheader('Set-Cookie')
                status = response.status; connection.close()
                return is_change, status, session_cookie.split(';')[0] if session_cookie else None

            with ThreadPoolExecutor(max_workers=7) as executor:
                outcomes = list(executor.map(concurrent_request, [False] * 3 + [True] + [False] * 3))
            for is_change, status, session_cookie in outcomes:
                if is_change:
                    assert status == 200 and session_cookie
                    cookie = session_cookie
                else:
                    assert status in (200, 401, 429)
                    if status == 200:
                        call('GET', '/auth/me', expected=401, extra={'Cookie': session_cookie})
            call('GET', '/auth/me')
            new_password = concurrent_password
            check('concurrent old-password logins cannot survive password revocation')
            process.terminate(); process.wait(timeout=40)
            assert process.returncode == 0, (process.returncode, (root / 'server.log').read_text())
            start()
            call('GET', '/auth/me')
            assert len(call('GET', '/resources/certificates')) == 1
            assert get_proxy('/')[0] == 308
            call('POST', '/auth/logout'); call('GET', '/auth/me', expected=401)
            check('graceful shutdown, restart persistence and logout')
        finally:
            if process and process.poll() is None:
                process.terminate()
                try: process.wait(timeout=40)
                except subprocess.TimeoutExpired: process.kill(); process.wait()
            upstream.shutdown(); log.close()
        print(f'{len(checks)} integration groups passed')


if __name__ == '__main__':
    main()
