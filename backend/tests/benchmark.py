#!/usr/bin/env python3
# @created 2026-10-13
# @description 在相同回环负载下比较两个 release 构建并采样代理内存与 CPU。
# @author yunhungo
import argparse
import http.client
import json
import os
from pathlib import Path
import secrets
import statistics
import subprocess
import tempfile
import threading
import time
from smoke import free_port

ROOT = Path(__file__).resolve().parents[2]


def sample(pid):
    row = subprocess.check_output(['ps', '-p', str(pid), '-o', 'rss=,time='], text=True).split()
    clock = row[1].split(':')
    seconds = float(clock[-1]) + int(clock[-2]) * 60
    if len(clock) == 3: seconds += int(clock[0]) * 3600
    return int(row[0]), seconds


def run(binary, name, upstream, rounds, requests):
    ports = [free_port() for _ in range(3)]
    with tempfile.TemporaryDirectory(prefix='openlucky-bench-') as folder:
        root = Path(folder)
        env = dict(os.environ, OPENLUCKY_DATA=str(root / 'data'), OPENLUCKY_ADMIN_ADDR=f'127.0.0.1:{ports[0]}', OPENLUCKY_HTTP_ADDR=f'127.0.0.1:{ports[1]}', OPENLUCKY_HTTPS_ADDR=f'127.0.0.1:{ports[2]}', OPENLUCKY_PUBLIC_HTTPS_PORT=str(ports[2]), OPENLUCKY_HTTP3_ADDR='', OPENLUCKY_PUBLIC_HTTP3_PORT='', OPENLUCKY_LISTENERS='[]')
        log = (root / 'server.log').open('w')
        process = subprocess.Popen([str(binary)], cwd=ROOT, env=env, stdout=log, stderr=log)
        cookie = ''
        def api(method, path, value=None):
            nonlocal cookie
            conn = http.client.HTTPConnection('127.0.0.1', ports[0], timeout=20)
            conn.request(method, '/api/v1' + path, None if value is None else json.dumps(value), {'Content-Type': 'application/json', 'X-OpenLucky': '1', 'Cookie': cookie})
            response = conn.getresponse(); data = response.read(); conn.close()
            if response.getheader('Set-Cookie'): cookie = response.getheader('Set-Cookie').split(';')[0]
            assert response.status == 200, (response.status, data[:200])
            return json.loads(data)
        try:
            for _ in range(100):
                if process.poll() is not None: raise AssertionError((root / 'server.log').read_text())
                if (root / 'data/setup-token').exists():
                    try:
                        api('GET', '/auth/status'); break
                    except OSError: pass
                time.sleep(.1)
            api('POST', '/auth/setup', {'username': 'admin', 'password': secrets.token_urlsafe(24), 'setup_token': (root / 'data/setup-token').read_text()})
            subprocess.run(['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes', '-keyout', str(root / 'key.pem'), '-out', str(root / 'cert.pem'), '-days', '2', '-subj', '/CN=bench.example.test', '-addext', 'subjectAltName=DNS:bench.example.test', '-addext', 'basicConstraints=critical,CA:FALSE'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)
            cert = api('POST', '/resources/certificates', {'name': 'Bench TLS', 'domains': ['bench.example.test'], 'email': '', 'credential_id': '', 'staging': False, 'auto_renew': False, 'imported': True})['id']
            api('POST', f'/certificates/{cert}/import', {'cert_pem': (root / 'cert.pem').read_text(), 'key_pem': (root / 'key.pem').read_text()})
            rule = {'name': 'Bench', 'domain': 'bench.example.test', 'target': f'http://127.0.0.1:{upstream}', 'enabled': True, 'certificate_id': cert, 'https_redirect': False, 'preserve_host': False}
            wid = api('POST', '/resources/web', rule)['id']
            for i in range(41): api('POST', '/resources/web', {**rule, 'name': f'Route {i}', 'domain': f'route-{i}.example.test', 'certificate_id': ''})
            def configure(dynamic):
                current = next(v for v in api('GET', '/resources/web') if v['id'] == wid)
                options = {**current.get('options', {}), 'http3': True, 'request_headers': {'X-Host': '{{ request.host }}', 'X-Origin': '{{ request.scheme | upper }}', 'X-Client': '{{ client.ip }}'} if dynamic else {}, 'response_headers': {'X-Status': '{{ response.status }}'} if dynamic else {}}
                api('PUT', '/resources/web/' + wid, {**current, 'options': options})
            if name == 'candidate': configure(False)
            def load(protocol, count, path='/bench', concurrency=32):
                h3 = protocol == 'http3'
                options = {'address': f'127.0.0.1:{ports[1] if protocol == "http1" else ports[2]}', 'requests': count, 'concurrency': concurrency, 'path': path}
                if h3: options.update(server_name='bench.example.test', ca=str(root / 'cert.pem'))
                else: options.update(domain='bench.example.test', version=protocol, ca=None if protocol == 'http1' else str(root / 'cert.pem'))
                binary_name = 'h3_probe' if h3 else 'proxy_load'
                result = subprocess.run([str(ROOT / 'target/release/examples' / binary_name)], input=json.dumps(options), text=True, capture_output=True, timeout=90)
                assert result.returncode == 0, result.stderr + '\n' + (root / 'server.log').read_text()[-8000:]
                value = json.loads(result.stdout)
                assert value['failures'] == 0, value
                value.pop('response', None)
                return value
            results = []
            for protocol in ['http1', 'http2'] + (['http3'] if name == 'candidate' else []):
                load(protocol, 1000)
                for iteration in range(rounds):
                    stop = threading.Event(); rss = []
                    before = sample(process.pid)
                    def monitor():
                        while not stop.wait(.1): rss.append(sample(process.pid)[0])
                    monitor_thread = threading.Thread(target=monitor); monitor_thread.start()
                    try: value = load(protocol, requests)
                    finally: stop.set(); monitor_thread.join()
                    after = sample(process.pid)
                    value.update(protocol=protocol, round=iteration + 1, peak_rss_mib=max([before[0], after[0], *rss])/1024, cpu_seconds=round(after[1]-before[1], 3))
                    results.append(value)
                    print(json.dumps({'build': name, **value}), flush=True)
            if name == 'candidate':
                configure(True)
                for iteration in range(rounds):
                    value = load('http1', requests)
                    value.update(protocol='http1_templates', round=iteration + 1)
                    results.append(value); print(json.dumps({'build': name, **value}), flush=True)
                configure(False)
                for protocol in ['http1', 'http3']:
                    before = sample(process.pid); rss = []; stop = threading.Event()
                    def monitor_large():
                        while not stop.wait(.05): rss.append(sample(process.pid)[0])
                    monitor_thread = threading.Thread(target=monitor_large); monitor_thread.start()
                    try: value = load(protocol, 16, '/large', 8)
                    finally: stop.set(); monitor_thread.join()
                    time.sleep(2)
                    after = sample(process.pid)
                    value.update(protocol=protocol + '_16MiB', peak_rss_mib=max([before[0], after[0], *rss])/1024, after_rss_mib=after[0]/1024)
                    results.append(value); print(json.dumps({'build': name, **value}), flush=True)
            return {'build': name, 'binary': str(binary), 'route_count': 42, 'results': results, 'telemetry': api('GET', '/telemetry')}
        finally:
            process.terminate()
            try: process.wait(timeout=40)
            except subprocess.TimeoutExpired: process.kill(); process.wait()
            log.close()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--baseline', type=Path, required=True)
    parser.add_argument('--candidate', type=Path, default=ROOT / 'target/release/openlucky')
    parser.add_argument('--rounds', type=int, default=3)
    parser.add_argument('--requests', type=int, default=30000)
    parser.add_argument('--candidate-only', action='store_true')
    parser.add_argument('--output', type=Path, default=ROOT / '.myself/qa/web-performance.json')
    args = parser.parse_args()
    upstream_port = free_port()
    upstream = subprocess.Popen([str(ROOT / 'target/release/examples/protocol_upstream'), f'127.0.0.1:{upstream_port}'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        output = []
        args.output.parent.mkdir(parents=True, exist_ok=True)
        builds = [] if args.candidate_only else [(args.baseline.resolve(), 'baseline')]
        for binary, name in builds + [(args.candidate.resolve(), 'candidate')]:
            output.append(run(binary, name, upstream_port, args.rounds, args.requests))
            args.output.write_text(json.dumps(output, indent=2))
        for build in output:
            for protocol in dict.fromkeys(v['protocol'] for v in build['results']):
                rows = [v for v in build['results'] if v['protocol'] == protocol]
                print(json.dumps({'summary': build['build'], 'protocol': protocol, 'median_rps': round(statistics.median(v['rps'] for v in rows)), 'median_p95_ms': round(statistics.median(v['p95_ms'] for v in rows), 3)}), flush=True)
    finally:
        upstream.terminate(); upstream.wait(timeout=5)


if __name__ == '__main__': main()
