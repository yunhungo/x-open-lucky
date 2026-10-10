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
    admin, proxy, tls = free_port(), free_port(), free_port()
    upstream = ThreadingHTTPServer(('127.0.0.1', 0), Upstream)
    threading.Thread(target=upstream.serve_forever, daemon=True).start()
    checks = []
    with tempfile.TemporaryDirectory(prefix='openlucky-smoke-') as directory:
        root = Path(directory)
        env = dict(os.environ, OPENLUCKY_DATA=str(root / 'data'), OPENLUCKY_ADMIN_ADDR=f'127.0.0.1:{admin}', OPENLUCKY_HTTP_ADDR=f'127.0.0.1:{proxy}', OPENLUCKY_HTTPS_ADDR=f'127.0.0.1:{tls}', OPENLUCKY_PUBLIC_HTTPS_PORT=str(tls))
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
            password = secrets.token_urlsafe(24)
            setup = (root / 'data/setup-token').read_text()
            assert not call('GET', '/auth/status')['initialized']
            call('GET', '/overview', expected=401)
            call('POST', '/auth/setup', {'username': 'admin', 'password': password, 'setup_token': 'incorrect'}, expected=403)
            call('POST', '/auth/setup', {'username': 'admin', 'password': password, 'setup_token': setup})
            assert not (root / 'data/setup-token').exists()
            assert call('GET', '/auth/me')['username'] == 'admin'
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
                assert db.execute('SELECT COUNT(*) FROM _sqlx_migrations').fetchone()[0] == 2
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
            call('DELETE', f'/resources/certificates/{cert_id}', expected=400)
            check('certificate validation, SNI TLS handshake and HTTPS redirect')
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
            new_password = secrets.token_urlsafe(24)
            call('PUT', '/auth/password', {'current_password': password, 'new_password': new_password})
            new_cookie = cookie
            call('GET', '/auth/me', expected=401, extra={'Cookie': old_cookie})
            cookie = new_cookie
            call('GET', '/auth/me')
            call('POST', '/auth/login', {'username': 'admin', 'password': password}, expected=401)
            call('POST', '/auth/login', {'username': 'admin', 'password': new_password})
            check('password change revokes old sessions')
            process.terminate(); process.wait(timeout=40); assert process.returncode == 0
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
